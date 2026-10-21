"""Check source membership through the existing Bazel coverage command."""

import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path
from urllib.parse import unquote, urlparse

SOURCE = """def choose(value):
    if value:
        return 1
    return 0
"""


GO_SOURCE = "predix/internal/domain/user/user.go"

BAZEL_ENDPOINT_OPTIONS: list[str] = []
BAZEL_OPTIONS: list[str] = []
COMMAND_LOG: Path | None = None


def redact_output(output: str) -> str:
    """Remove BuildBuddy credentials from captured command output."""
    return re.sub(
        r"""(?i)(x-buildbuddy-api-key[=:])[^\s"'\\]+""", r"\1<REDACTED>", output
    )


def command(root: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    options = BAZEL_ENDPOINT_OPTIONS + (
        [] if arguments[0] == "query" else BAZEL_OPTIONS
    )
    invocation = ["bazel", arguments[0], *options, *arguments[1:]]
    result = subprocess.run(
        invocation,
        cwd=root,
        env={
            key: value
            for key, value in os.environ.items()
            if key not in {"PYTHONHOME", "PYTHONEXECUTABLE"}
        },
        capture_output=True,
        text=True,
        check=False,
    )
    result.stdout = redact_output(result.stdout)
    result.stderr = redact_output(result.stderr)
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


def _go_line_counts(record: list[str]) -> dict[int, int]:
    entries = [line[3:].split(",") for line in record if line.startswith("DA:")]
    counts = {int(fields[0]): int(fields[1]) for fields in entries}
    assert counts and len(entries) == len(counts), (
        "Go executable records are absent or repeated."
    )
    assert all(line > 0 and count >= 0 for line, count in counts.items())
    assert [line for line in record if line.startswith("LF:")] == [f"LF:{len(counts)}"]
    assert [line for line in record if line.startswith("LH:")] == [
        f"LH:{sum(count > 0 for count in counts.values())}"
    ]
    return counts


def assert_go_coverage(actual: list[str], measured: list[str]) -> None:
    actual_counts = _go_line_counts(actual)
    measured_counts = _go_line_counts(measured)
    for line in (29, 38):
        assert measured_counts.get(line, 0) > 0, (
            f"Native Go body line {line} has no hits."
        )
    for line, count in measured_counts.items():
        assert actual_counts.get(line, -1) >= count, (
            f"Measured Go line {line} lost records or hits: {actual}"
        )


def source_records(report: str) -> dict[str, list[str]]:
    records = {}
    for record in report.split("end_of_record"):
        lines = record.strip().splitlines()
        if not lines:
            continue
        source = next(line[3:] for line in lines if line.startswith("SF:"))
        assert source not in records, f"Repeated source: {source}"
        records[source] = lines
    return records


def check_report(report: str, ordinary: str, hidden: str, native_go: str) -> None:
    records = source_records(report)
    for package in (ordinary, hidden):
        lines = records[f"{package}/untested.py"]
        assert "DA:1,0" in lines
        assert "DA:2,0" in lines
        assert "DA:3,0" in lines
        assert "DA:4,0" in lines
        assert "BRF:2" in lines
        assert "BRH:0" in lines
        dormant = records[f"{package}/dormant_test.py"]
        assert "DA:2,0" in dormant
        assert "DA:3,0" in dormant
        assert "DA:7,0" in dormant
        assert "BRF:2" in dormant
        assert "BRH:0" in dormant
    helper = records[f"{hidden}/helper.py"]
    assert "DA:3,1" in helper
    assert "DA:4,0" in helper
    assert "BRF:4" in helper
    assert "BRH:2" in helper
    observed = records[f"{ordinary}/observed/observed.py"]
    assert "DA:3,1" in observed
    assert "DA:4,0" in observed
    assert "BRF:2" in observed
    assert "BRH:1" in observed
    assert f"{ordinary}/generated.py" not in records
    support = records[f"{ordinary}/test_helpers/support.py"]
    assert "DA:2,0" in support
    assert "DA:3,0" in support
    assert "BRF:2" in support
    assert "BRH:0" in support
    test = records[f"{ordinary}/fixture_test.py"]
    assert "DA:6,1" in test
    for source in records:
        assert not source.startswith(("/", "external/", "bazel-out/")), source
    assert "DA:3,2" in records["python_sample/versions.py"]
    assert "DA:6,2" in records["python_sample/__main__.py"]
    assert "DA:43,1" in records[".github/scripts/bazel_dependencies.py"]
    assert_go_coverage(records[GO_SOURCE], source_records(native_go)[GO_SOURCE])


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
    evidence = Path(arguments.evidence).absolute()
    if arguments.execution == "local":
        BAZEL_ENDPOINT_OPTIONS.extend(
            ["--remote_cache=", "--bes_backend=", "--remote_executor="]
        )
        BAZEL_OPTIONS.append("--spawn_strategy=sandboxed")
    else:
        BAZEL_OPTIONS.append("--config=ci")
    try:
        evidence.mkdir(parents=True, mode=0o700)
    except FileExistsError:
        parser.error("The evidence path already exists. Use a new path.")
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
        for package in (ordinary, hidden):
            generate(
                root,
                package / "dormant_test.py",
                SOURCE
                + '\ndef test_must_not_run():\n    raise AssertionError("Filtered test executed.")\n',
            )
            build = package / "BUILD.bazel"
            build.write_text(
                build.read_text().replace(
                    'name = "dormant_test",',
                    'name = "dormant_test",\n    tags = ["manual", "no-remote", "no-coverage"],  # keep',
                )
            )
        generate(
            root,
            hidden / "helper.py",
            SOURCE + '\nif __name__ == "__main__":\n    print(choose(True))\n',
        )
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
        generate(root, support / "support.py", SOURCE)
        build = support / "BUILD.bazel"
        text = build.read_text()
        position = text.index("\n\n")
        build.write_text(
            text[:position] + "\n\npackage(default_testonly = True)" + text[position:]
        )
        generate(
            root,
            ordinary / "fixture_test.py",
            f"""import os
import subprocess
from pathlib import Path

def test_passes():
    root = Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"]
    result = subprocess.run(
        [str(root / ".github/{hidden.name}/helper_bin")],
        capture_output=True,
        text=True,
        check=True,
    )
    assert result.stdout == "1\\n"
""",
        )
        build = ordinary / "BUILD.bazel"
        text = build.read_text().replace(
            'name = "fixture_test",',
            f'name = "fixture_test",\n    data = [":generated", "//{support.relative_to(root)}:test_helpers", "//{hidden.relative_to(root)}:helper_bin"],',
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
        hidden_build = hidden / "BUILD.bazel"
        text = hidden_build.read_text()
        position = text.index("\n\n")
        text = (
            text[:position] + "\n\npackage(default_testonly = True)" + text[position:]
        )
        hidden_build.write_text(
            'load("//tools/python:defs.bzl", "py_binary")\n'
            + text
            + f'\npy_binary(name = "helper_bin", testonly = True, srcs = ["helper.py"], main = "helper.py", visibility = ["//{ordinary.relative_to(root)}:__pkg__"])\n'
        )
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
        event_text = redact_output(events.read_text())
        events.write_text(event_text)
        for line in event_text.splitlines():
            event = json.loads(line)
            if "testResult" in event:
                executions.append({"id": event["id"], **event["testResult"]})
        (evidence / "test-results.json").write_text(json.dumps(executions, indent=2))
        assert executions, "No test execution metadata"
        assert not any(
            item["id"]["testResult"]["label"].endswith(":dormant_test")
            for item in executions
        ), executions
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
        shutil.copyfile(root / "coverage-report.lcov", evidence / "processed.lcov")
        go_output = next(
            output
            for item in executions
            if item["id"]["testResult"]["label"]
            == "//predix/internal/domain/user:user_test"
            for output in item["testActionOutput"]
            if output["name"] == "test.lcov"
        )
        go_uri = urlparse(go_output["uri"])
        assert go_uri.scheme == "file" and not go_uri.netloc, go_output["uri"]
        go_report = evidence / "go-user-test.lcov"
        shutil.copyfile(Path(unquote(go_uri.path)), go_report)
        native_go = go_report.read_text()
        check_report(
            report.read_text(),
            str(ordinary.relative_to(root)),
            str(hidden.relative_to(root)),
            native_go,
        )
        assert_go_coverage(
            source_records((root / "coverage-report.lcov").read_text())[GO_SOURCE],
            source_records(native_go)[GO_SOURCE],
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
