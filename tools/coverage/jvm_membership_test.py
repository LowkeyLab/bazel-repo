"""Check JVM baselines, measured execution, and failures through coverage commands."""

import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--execution", choices=["local", "configured"], default="local")
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    root = Path(os.environ.get("BUILD_WORKSPACE_DIRECTORY", os.getcwd())).resolve()
    evidence = args.evidence or Path(tempfile.mkdtemp(prefix="jvm-coverage-evidence-"))
    if args.evidence:
        evidence.mkdir(mode=0o700)
    evidence = evidence.resolve()
    if evidence.is_relative_to(root):
        parser.error("The evidence directory must be outside the checkout.")
    options = (
        ["--remote_cache=", "--remote_executor=", "--bes_backend="]
        if args.execution == "local"
        else ["--config=remote-linux"]
    )
    environment = {
        key: value
        for key, value in os.environ.items()
        if key not in {"PYTHONHOME", "PYTHONEXECUTABLE"}
    }
    sequence = 0

    def run(*arguments: str, success: bool = True) -> str:
        nonlocal sequence
        result = subprocess.run(
            ["bazel", arguments[0], *options, *arguments[1:]],
            cwd=root,
            env=environment,
            text=True,
            capture_output=True,
            check=False,
        )
        sequence += 1
        output = re.sub(
            r"(?i)(x-buildbuddy-api-key[=:])[^\s\"'\\]+",
            r"\1<REDACTED>",
            result.stdout + result.stderr,
        )
        (evidence / f"command-{sequence}.log").write_text(output)
        for events in evidence.glob("*.bep.json"):
            events.write_text(
                re.sub(
                    r"(?i)(x-buildbuddy-api-key[=:])[^\s\"'\\]+",
                    r"\1<REDACTED>",
                    events.read_text(),
                )
            )
        with (evidence / "commands.jsonl").open("a") as stream:
            stream.write(
                json.dumps(
                    {
                        "arguments": ["bazel", arguments[0], *options, *arguments[1:]],
                        "exit": result.returncode,
                    }
                )
                + "\n"
            )
        assert (result.returncode == 0) == success, output
        return result.stdout

    def write(path: Path, text: str) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        run("run", "//:gazelle")

    def cover(test: str, name: str) -> dict[str, str]:
        run(
            "run",
            "//tools/coverage",
            "--",
            test,
            *options,
            "--nocache_test_results",
            f"--build_event_json_file={evidence / (name + '.bep.json')}",
        )
        shutil.copyfile(root / "coverage-report.lcov", evidence / (name + ".lcov"))
        shutil.copyfile(
            root / "coverage-inventory.json", evidence / (name + "-inventory.json")
        )
        shutil.copytree(root / "coverage-jvm", evidence / (name + "-analysis"))
        found = {}
        for record in (
            (root / "coverage-report.lcov").read_text().split("end_of_record")
        ):
            if not record.strip():
                continue
            source = next(
                line[3:] for line in record.splitlines() if line.startswith("SF:")
            )
            assert source not in found, source
            found[source] = record
        return found

    (evidence / "revision.txt").write_text(
        subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=root,
            text=True,
            capture_output=True,
            check=True,
        ).stdout
    )
    with (
        tempfile.TemporaryDirectory(prefix="jvm_membership_", dir=root) as ordinary,
        tempfile.TemporaryDirectory(prefix=".jvm_membership_", dir=root) as hidden,
    ):
        directories = (Path(ordinary), Path(hidden))
        expected = []
        for index, directory in enumerate(directories):
            (directory / "BUILD.bazel").write_text(
                '# gazelle:ignore\nload("//tools/coverage:jvm.bzl", "java_library")\njava_library(name="lib", srcs=["Unused.java", "Empty.java"], tags=["manual", "no-coverage"])\njava_library(name="copy", srcs=["Twin.java"])\njava_library(name="variant", srcs=["Variant.java"])\n'
            )
            write(
                directory / "Unused.java",
                f"package fixture{index};\npublic class Unused {{\n  public static int value(boolean enabled) {{\n    if (enabled) {{\n      return 7;\n    }}\n    return 3;\n  }}\n}}\n",
            )
            write(
                directory / "Empty.java",
                f"package declaration{index};\n// This source declares no class.\n",
            )
            write(
                directory / "Twin.java",
                "package collision;\npublic class Twin {\n  public static int value() { return 13; }\n}\n",
            )
            write(
                directory / "Variant.java",
                f"package versions;\npublic class Variant {{\n  public static int value() {{ return {13 + index}; }}\n}}\n",
            )
            expected.append(str((directory / "Unused.java").relative_to(root)))
        selected = directories[0]
        selected_build = selected / "BUILD.bazel"
        selected_build.write_text(
            selected_build.read_text()
            + """
config_setting(name="inactive", define_values={"jvm_fixture_inactive": "yes"})
java_library(name="selected", srcs=select({":inactive": ["Inactive.java"], "//conditions:default": ["Active.java"]}))
java_library(name="empty_selected", srcs=select({":inactive": ["InactiveEmpty.java"], "//conditions:default": []}))
"""
        )
        for name in ("Active", "Inactive", "InactiveEmpty"):
            write(
                selected / (name + ".java"),
                f"package selected;\npublic class {name} {{ public int value() {{ return 7; }} }}\n",
            )
        baseline = cover("//tools/coverage:inventory_test", "baseline")
        for name in expected:
            lines = [
                line for line in baseline[name].splitlines() if line.startswith("DA:")
            ]
            assert lines and all(line.endswith(",0") for line in lines), baseline[name]
        for directory in directories:
            name = str((directory / "Empty.java").relative_to(root))
            assert "LF:0" in baseline[name] and "DA:" not in baseline[name], baseline[
                name
            ]
        baseline_receipt = json.loads((root / "coverage-jvm/receipt.json").read_text())
        for name in ("Inactive", "InactiveEmpty"):
            source = str((selected / (name + ".java")).relative_to(root))
            assert source not in baseline
            assert any(
                limit["source"] == source
                and "configured source selection" in limit["missing_guarantee"]
                for limit in baseline_receipt["limits"]
            )
        assert str((selected / "Active.java").relative_to(root)) in baseline
        directory = directories[0]
        package = directory.name
        build = directory / "BUILD.bazel"
        build.write_text(
            build.read_text().replace(
                '"java_library")',
                '"java_library", "java_test", "java_binary", "kt_jvm_library", "kt_jvm_test")',
            )
            + """
java_binary(name="child", srcs=["Child.java"], main_class="child.Child")
java_test(name="execution", srcs=["Execution.java"], main_class="runner.Execution", use_testrunner=False, deps=[":lib", ":copy", ":variant", ":unique_duplicate", ":shared_executed"], data=[":child"])
kt_jvm_library(name="inline", srcs=["Inline.kt"])
kt_jvm_test(name="kotlin_test", srcs=["KotlinTest.kt"], main_class="runner.KotlinTestKt", deps=[":inline"])
java_library(name="conditional_runtime", srcs=["Conditional.java"], deps=["//PACKAGE/runtime_flags:flags"])
java_library(name="conditional_constant", srcs=["Conditional.java"], deps=["//PACKAGE/constant_flags:flags"])
java_library(name="shared_executed", srcs=["Shared.java"], javacopts=["-g"])
java_library(name="shared_unused", srcs=["Shared.java"], javacopts=["-g:source,lines"])
java_library(name="inactive_owner", srcs=select({":inactive": ["Unused.java"], "//conditions:default": []}))
java_library(name="unique_duplicate", srcs=["Same.java"])
java_test(name="excluded", srcs=[], main_class="clojure.main", use_testrunner=False, args=["PACKAGE/Excluded.clj"], data=["Excluded.clj"], runtime_deps=select({":inactive": [":lib"], "//conditions:default": ["@deps//:org_clojure_clojure"]}))
java_library(name="duplicates", srcs=["//PACKAGE/a:Same.java", "//PACKAGE/b:Same.java"])
""".replace("PACKAGE", package)
        )
        for name in ("a", "b"):
            subdirectory = directory / name
            subdirectory.mkdir()
            (subdirectory / "BUILD.bazel").write_text(
                '# gazelle:ignore\nexports_files(["Same.java"])\n'
            )
        write(
            directory / "Same.java",
            "package a;\npublic class Same { public int value() { return 7; } }\n",
        )
        for name, modifier in (("runtime_flags", ""), ("constant_flags", "final ")):
            flags = directory / name
            flags.mkdir()
            (flags / "BUILD.bazel").write_text(
                '# gazelle:ignore\nload("//tools/coverage:jvm.bzl", "java_library")\njava_library(name="flags", srcs=["Flags.java"], visibility=["//visibility:public"])\n'
            )
            write(
                flags / "Flags.java",
                f"package flags;\npublic class Flags {{ public static {modifier}boolean ENABLED = true; }}\n",
            )
        write(
            directory / "Conditional.java",
            "package conditional;\npublic class Conditional {\n  public static int value() {\n    if (flags.Flags.ENABLED) { return 7; }\n    return 3;\n  }\n}\n",
        )
        write(
            directory / "Shared.java",
            "package shared;\npublic class Shared {\n  public static int choose(boolean enabled) {\n    if (enabled) { return 7; }\n    return 3;\n  }\n}\n",
        )
        write(directory / "Excluded.clj", '(println "excluded-only")\n')
        write(
            directory / "Child.java",
            'package child;\npublic class Child {\n  public static void main(String[] args) {\n    if (args.length == 0) {\n      System.out.println("child-only");\n    }\n  }\n}\n',
        )
        write(
            directory / "Execution.java",
            f"""package runner;
public class Execution {{
  public static void main(String[] args) throws Exception {{
    if (new a.Same().value() != 7) {{ throw new AssertionError(); }}
    if (shared.Shared.choose(true) != 7) {{ throw new AssertionError(); }}
    if (fixture0.Unused.value(true) != 7) {{ throw new AssertionError(); }}
    if (collision.Twin.value() != 13 || versions.Variant.value() != 13) {{ throw new AssertionError(); }}
    String child = System.getenv("TEST_SRCDIR") + "/" + System.getenv("TEST_WORKSPACE") + "/{package}/child";
    if (new ProcessBuilder(child).inheritIO().start().waitFor() != 0) {{ throw new AssertionError(); }}
  }}
}}
""",
        )
        write(
            directory / "Inline.kt",
            "package inlinefixture\ninline fun choose(enabled: Boolean): Int {\n  return if (enabled) 7 else 3\n}\n",
        )
        write(
            directory / "KotlinTest.kt",
            "package runner\nimport inlinefixture.choose\nfun main() {\n  check(choose(true) == 7)\n}\n",
        )
        for name in ("a", "b"):
            write(
                directory / name / "Same.java",
                f"package {name};\npublic class Same {{ public int value() {{ return 7; }} }}\n",
            )
        shutil.copytree(directory, evidence / "ordinary-inputs")
        shutil.copytree(directories[1], evidence / "hidden-inputs")
        excluded = cover(f"//{package}:excluded", "excluded")
        assert expected[0] in excluded
        assert all(
            line.endswith(",0")
            for line in excluded[expected[0]].splitlines()
            if line.startswith("DA:")
        )
        assert f"{package}/Excluded.clj" not in excluded
        excluded_receipt = json.loads((root / "coverage-jvm/receipt.json").read_text())
        assert excluded_receipt["test_inputs"][f"//{package}:excluded"]["sources"] == []
        measured = cover(f"//{package}:execution", "measured")
        assert "DA:5,1" in measured[expected[0]], measured[expected[0]]
        assert all(
            line.endswith(",0")
            for line in measured[expected[1]].splitlines()
            if line.startswith("DA:")
        )
        child = f"{package}/Child.java"
        assert "DA:5,1" in measured[child], measured[child]
        measured_receipt = json.loads((root / "coverage-jvm/receipt.json").read_text())
        assert (
            f"{package}/Execution.java"
            in measured_receipt["test_inputs"][f"//{package}:execution"]["sources"]
        )
        assert (
            expected[0]
            in measured_receipt["test_inputs"][f"//{package}:execution"]["sources"]
        )
        assert any(
            limit["source"] == expected[0]
            and limit.get("rule") == f"//{package}:inactive_owner.jvm_coverage"
            and "configured source selection" in limit["missing_guarantee"]
            for limit in measured_receipt["limits"]
        )
        conditional = f"{package}/Conditional.java"
        assert conditional not in measured_receipt["branch_totals"]
        assert (
            measured_receipt["owner_branch_totals"][
                f"//{package}:conditional_runtime.jvm_coverage"
            ][conditional]["missed"]
            > 0
        )
        assert measured_receipt["owner_branch_totals"][
            f"//{package}:conditional_constant.jvm_coverage"
        ][conditional] == {"missed": 0, "covered": 0}
        assert (
            child
            in measured_receipt["test_inputs"][f"//{package}:execution"]["sources"]
        )
        shared = f"{package}/Shared.java"
        assert shared not in measured_receipt["branch_totals"]
        assert (
            measured_receipt["owner_branch_totals"][
                f"//{package}:shared_executed.jvm_coverage"
            ][shared]["covered"]
            > 0
        )
        assert (
            measured_receipt["owner_branch_totals"][
                f"//{package}:shared_unused.jvm_coverage"
            ][shared]["covered"]
            == 0
        )
        assert measured_receipt["branch_totals"][expected[0]] == {
            "missed": 1,
            "covered": 1,
        }
        assert any(
            limit["source"] == child
            and "unexecuted child JVM" in limit["missing_guarantee"]
            for limit in measured_receipt["limits"]
        )
        assert f"{package}/Same.java" not in measured
        assert any(
            limit["source"] == f"{package}/Same.java"
            and "class identity" in limit["missing_guarantee"]
            for limit in measured_receipt["limits"]
        )
        for directory in directories:
            name = str((directory / "Twin.java").relative_to(root))
            assert name not in measured, name
            assert any(
                limit["source"] == name
                and "class identity" in limit["missing_guarantee"]
                for limit in measured_receipt["limits"]
            ), name
        ordinary_variant = str((directories[0] / "Variant.java").relative_to(root))
        hidden_variant = str((directories[1] / "Variant.java").relative_to(root))
        assert "DA:3,1" in measured[ordinary_variant], measured[ordinary_variant]
        assert all(
            line.endswith(",0")
            for line in measured[hidden_variant].splitlines()
            if line.startswith("DA:")
        ), measured[hidden_variant]
        kotlin = cover(f"//{package}:kotlin_test", "kotlin")
        assert any(
            not line.endswith(",0")
            for line in kotlin[f"{package}/KotlinTest.kt"].splitlines()
            if line.startswith("DA:")
        )
        receipt = json.loads((root / "coverage-jvm/receipt.json").read_text())
        assert (
            f"{package}/KotlinTest.kt"
            in receipt["test_inputs"][f"//{package}:kotlin_test"]["sources"]
        )
        for name in (
            f"{package}/a/Same.java",
            f"{package}/b/Same.java",
            f"{package}/Inline.kt",
        ):
            assert any(limit["source"] == name for limit in receipt["limits"]), name
        assert any(name.endswith(".rs") for name in kotlin)
        assert any(name.endswith(".go") for name in kotlin)
        assert any(name.endswith(".py") for name in kotlin)
        run(
            "test",
            f"//{package}:execution",
            f"//{package}:kotlin_test",
            "--nocache_test_results",
        )
        print(f"JVM command acceptance passed. Evidence: {evidence}")


if __name__ == "__main__":
    main()
