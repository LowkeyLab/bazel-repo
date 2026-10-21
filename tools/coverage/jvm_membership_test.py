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
                '# gazelle:ignore\nload("//tools/coverage:jvm.bzl", "java_library")\njava_library(name="lib", srcs=["Unused.java", "Empty.java"], tags=["manual", "no-coverage"])\n'
            )
            write(
                directory / "Unused.java",
                f"package fixture{index};\npublic class Unused {{\n  public static int value(boolean enabled) {{\n    if (enabled) {{\n      return 7;\n    }}\n    return 3;\n  }}\n}}\n",
            )
            write(
                directory / "Empty.java",
                f"package declaration{index};\n// This source declares no class.\n",
            )
            expected.append(str((directory / "Unused.java").relative_to(root)))
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
java_test(name="execution", srcs=["Execution.java"], main_class="runner.Execution", use_testrunner=False, deps=[":lib"], data=[":child"])
kt_jvm_library(name="inline", srcs=["Inline.kt"])
kt_jvm_test(name="kotlin_test", srcs=["KotlinTest.kt"], main_class="runner.KotlinTestKt", deps=[":inline"])
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
            directory / "Child.java",
            'package child;\npublic class Child {\n  public static void main(String[] args) {\n    if (args.length == 0) {\n      System.out.println("child-only");\n    }\n  }\n}\n',
        )
        write(
            directory / "Execution.java",
            f"""package runner;
public class Execution {{
  public static void main(String[] args) throws Exception {{
    if (fixture0.Unused.value(true) != 7) {{ throw new AssertionError(); }}
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
        assert measured_receipt["branch_totals"][expected[0]] == {
            "missed": 1,
            "covered": 1,
        }
        assert any(
            limit["source"] == child
            and "unexecuted child JVM" in limit["missing_guarantee"]
            for limit in measured_receipt["limits"]
        )
        kotlin = cover(f"//{package}:kotlin_test", "kotlin")
        assert any(
            not line.endswith(",0")
            for line in kotlin[f"{package}/KotlinTest.kt"].splitlines()
            if line.startswith("DA:")
        )
        receipt = json.loads((root / "coverage-jvm/receipt.json").read_text())
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
