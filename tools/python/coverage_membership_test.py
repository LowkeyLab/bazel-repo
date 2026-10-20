"""Check complete coverage through Bazel with temporary production packages."""

import argparse
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

SOURCE = """def choose(value):
    if value:
        return 1
    return 0
"""


BAZEL_ENDPOINT_OPTIONS: list[str] = []
BAZEL_OPTIONS: list[str] = []
COMMAND_LOG: Path | None = None


def command(root: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    options = BAZEL_ENDPOINT_OPTIONS + (
        [] if arguments[0] == "query" else BAZEL_OPTIONS
    )
    invocation = ["bazel", arguments[0], *options, *arguments[1:]]
    result = subprocess.run(
        invocation,
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )
    if COMMAND_LOG is not None:
        with COMMAND_LOG.open("a") as output:
            output.write(
                json.dumps({"command": invocation, "exit": result.returncode}) + "\n"
            )
    return result


def generate(root: Path, path: Path, text: str) -> None:
    path.write_text(text)
    result = command(root, "run", "//:gazelle")
    assert result.returncode == 0, result.stdout + result.stderr


def check_report(report: str, ordinary: str, hidden: str) -> None:
    records = {}
    for record in report.split("end_of_record"):
        lines = record.strip().splitlines()
        if not lines:
            continue
        source = next(line[3:] for line in lines if line.startswith("SF:"))
        assert source not in records, f"Repeated source: {source}"
        records[source] = lines
    for package in (ordinary, hidden):
        lines = records[f"{package}/untested.py"]
        assert "DA:1,0" in lines
        assert "DA:2,0" in lines
        assert "DA:3,0" in lines
        assert "DA:4,0" in lines
        assert "BRF:2" in lines
        assert "BRH:0" in lines
    observed = records[f"{ordinary}/observed/observed.py"]
    assert "DA:3,1" in observed
    assert "DA:4,0" in observed
    assert "BRF:2" in observed
    assert "BRH:1" in observed
    assert f"{ordinary}/generated.py" not in records
    assert f"{ordinary}/test_helpers/support.py" not in records
    for source in records:
        assert not source.startswith(("/", "external/", "bazel-out/")), source
        assert not source.endswith("_test.py"), source
    assert "DA:3,2" in records["python_sample/versions.py"]
    assert "DA:6,2" in records["python_sample/__main__.py"]
    assert "DA:43,1" in records[".github/scripts/bazel_dependencies.py"]
    go = records["predix/internal/domain/user/user.go"]
    assert "LH:11" in go
    assert "LF:13" in go


def check_failures(root: Path, evidence: Path, package: Path) -> None:
    build = package / "BUILD.bazel"
    text = (
        'load("@aspect_rules_py//py:defs.bzl", raw_py_test = "py_test")\n'
        + build.read_text()
    )
    text += '\nraw_py_test(name = "missing_collector_test", srcs = ["fault_test.py"], pytest_main = True, deps = ["@pypi//pytest"], args = ["-o", "python_files=fault_test.py"])\n'
    build.write_text(text)
    target = f"//{package.relative_to(root)}:fault_test"
    cases = [
        (
            "collection",
            target,
            ["--test_env=COVERAGE_TEST_FAULT=collection"],
            "Python child coverage did not finish",
        ),
        (
            "combination",
            target,
            ["--test_env=COVERAGE_TEST_FAULT=combination"],
            "Couldn't use data file",
        ),
        (
            "report",
            target,
            [
                f"--test_env=COVERAGE_RCFILE={package.relative_to(root)}/report.coveragerc"
            ],
            "No data to report",
        ),
        (
            "dependency",
            f"//{package.relative_to(root)}:missing_collector_test",
            [],
            "No module named 'coverage'",
        ),
    ]
    for name, selected, flags, message in cases:
        ordinary = command(
            root, "test", selected, "--nocache_test_results", "--test_output=all"
        )
        (evidence / f"{name}-ordinary.log").write_text(
            ordinary.stdout + ordinary.stderr
        )
        assert ordinary.returncode == 0, ordinary.stdout + ordinary.stderr
        result = command(
            root,
            "coverage",
            selected,
            "--nocache_test_results",
            "--test_output=all",
            *flags,
        )
        text = result.stdout + result.stderr
        (evidence / f"{name}.log").write_text(text)
        (evidence / f"{name}.exit").write_text(f"{result.returncode}\n")
        assert result.returncode != 0, text
        assert message in text, text
        if name != "dependency":
            assert "1 passed" in text, text


def main() -> None:
    global COMMAND_LOG

    root = Path(os.environ["BUILD_WORKSPACE_DIRECTORY"])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("--execution", choices=("local", "remote"), default="local")
    arguments = parser.parse_args()
    evidence = arguments.evidence.absolute()
    if arguments.execution == "local":
        BAZEL_ENDPOINT_OPTIONS.extend(
            ["--remote_cache=", "--bes_backend=", "--remote_executor="]
        )
        BAZEL_OPTIONS.append("--spawn_strategy=sandboxed")
    else:
        BAZEL_OPTIONS.append("--config=ci")
    evidence.mkdir(parents=True, exist_ok=True)
    COMMAND_LOG = evidence / "commands.jsonl"
    revision = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=root,
        text=True,
        capture_output=True,
        check=True,
    )
    (evidence / "revision.txt").write_text(revision.stdout)
    diff = subprocess.run(
        ["git", "diff", "HEAD"], cwd=root, text=True, capture_output=True, check=True
    )
    (evidence / "working-tree.patch").write_text(diff.stdout)
    with (
        tempfile.TemporaryDirectory(
            prefix="coverage_fixture_", dir=root
        ) as ordinary_directory,
        tempfile.TemporaryDirectory(
            prefix="coverage_fixture_", dir=root / ".github"
        ) as hidden_directory,
    ):
        ordinary = Path(ordinary_directory)
        hidden = Path(hidden_directory)
        generate(root, ordinary / "untested.py", SOURCE)
        generate(root, hidden / "untested.py", SOURCE)
        observed = ordinary / "observed"
        observed.mkdir()
        generate(root, observed / "observed.py", SOURCE)
        generate(
            root,
            observed / "observed_test.py",
            f"from {ordinary.name}.observed.observed import choose\n\ndef test_choose():\n    assert choose(True) == 1\n",
        )
        support = ordinary / "test_helpers"
        support.mkdir()
        generate(root, support / "support.py", "TEST_SUPPORT = True\n")
        build = support / "BUILD.bazel"
        text = build.read_text()
        position = text.index("\n\n")
        build.write_text(
            text[:position] + "\n\npackage(default_testonly = True)" + text[position:]
        )
        generate(
            root, ordinary / "fixture_test.py", "def test_passes():\n    assert True\n"
        )
        build = ordinary / "BUILD.bazel"
        text = build.read_text().replace(
            'name = "fixture_test",',
            f'name = "fixture_test",\n    data = [":generated", "//{support.relative_to(root)}:test_helpers"],',
        )
        build.write_text(text)
        with (ordinary / "BUILD.bazel").open("a") as build:
            build.write("""
genrule(
    name = "generate",
    outs = ["generated.py"],
    cmd = "echo 'GENERATED = True' > $@",
)
py_library(name = "generated", srcs = [":generated.py"])
""")
        fault_package = ordinary / "faults"
        fault_package.mkdir()
        generate(
            root,
            fault_package / "fault_test.py",
            (root / "tools/python/coverage_fixture_test.py").read_text(),
        )
        (fault_package / "report.coveragerc").write_text("[report]\nomit = *\n")
        fault_build = fault_package / "BUILD.bazel"
        text = fault_build.read_text()
        text = text.replace(
            'name = "fault_test",',
            'name = "fault_test",\n    data = ["report.coveragerc"],',
        )
        fault_build.write_text(text)
        targets = [
            "//python_sample/...",
            "//.github/...",
            "//tools/python/...",
            "//predix/internal/domain/user:user_test",
            f"//{ordinary.relative_to(root)}/...",
        ]
        flags = ["--nocache_test_results", "--remote_accept_cached=false"]
        events = evidence / "coverage.bep.json"
        flags.append(f"--build_event_json_file={events}")
        result = command(
            root,
            "run",
            "//tools/coverage",
            "--",
            *targets,
            *BAZEL_ENDPOINT_OPTIONS,
            *BAZEL_OPTIONS,
            *flags,
        )
        (evidence / "coverage.log").write_text(result.stdout + result.stderr)
        (evidence / "coverage.exit").write_text(f"{result.returncode}\n")
        assert result.returncode == 0, result.stdout + result.stderr
        executions = []
        for line in events.read_text().splitlines():
            event = json.loads(line)
            if "testResult" in event:
                executions.append({"id": event["id"], **event["testResult"]})
        (evidence / "test-results.json").write_text(json.dumps(executions, indent=2))
        assert executions, "No test execution metadata"
        assert all(not item.get("cachedLocally", False) for item in executions)
        if arguments.execution == "remote":
            assert all(
                item["executionInfo"]["strategy"] == "remote" for item in executions
            ), executions
            assert all(
                not item["executionInfo"].get("cachedRemotely", False)
                for item in executions
            )
        else:
            assert all(
                item["executionInfo"]["strategy"] == "linux-sandbox"
                for item in executions
            ), executions
        output = command(root, "info", "output_path")
        assert output.returncode == 0, output.stdout + output.stderr
        report = Path(output.stdout.strip()) / "_coverage/_coverage_report.dat"
        shutil.copyfile(report, evidence / "combined.lcov")
        check_report(
            report.read_text(),
            str(ordinary.relative_to(root)),
            str(hidden.relative_to(root)),
        )
        for package in (ordinary, hidden):
            query = command(
                root,
                "query",
                f'kind("py_test rule", rdeps(//..., //{package.relative_to(root)}:untested.py))',
            )
            assert query.returncode == 0, query.stdout + query.stderr
            assert not query.stdout.strip(), query.stdout
            (evidence / f"{package.name}-outside-test-dependencies.log").write_text(
                query.stdout + query.stderr
            )
        check_failures(root, evidence, fault_package)
        source = ordinary / "untested.py"
        source.write_text("def broken(\n")
        # Gazelle must run immediately, including after a deliberate source fault.
        generation = command(root, "run", "//:gazelle")
        (evidence / "invalid-source-gazelle.log").write_text(
            generation.stdout + generation.stderr
        )
        result = command(root, "coverage", *targets, "--keep_going")
        text = result.stdout + result.stderr
        (evidence / "invalid-source.log").write_text(text)
        assert result.returncode != 0, text
        assert "PythonCoverageBaseline" in text and "Couldn't parse" in text, text
    print(f"Coverage membership checks passed. Evidence: {evidence}")


if __name__ == "__main__":
    main()
