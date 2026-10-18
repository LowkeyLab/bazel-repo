"""Check coverage reports and failures through real Bazel-built test commands."""

import os
import subprocess
from pathlib import Path

import pytest


ROOT = Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"]


def run_test(
    tmp_path: Path,
    target: str,
    sources: list[str],
    *,
    fault: str = "",
    coverage: bool = True,
) -> tuple[subprocess.CompletedProcess[str], Path]:
    directory = tmp_path / "coverage"
    directory.mkdir()
    manifest = tmp_path / "manifest"
    manifest.write_text("\n".join(sources) + "\n")
    report = directory / "coverage.dat"
    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("COVERAGE_", "BAZEL_PYTHON_COVERAGE"))
        and key != "TESTBRIDGE_TEST_ONLY"
    }
    environment.update(
        XML_OUTPUT_FILE=str(tmp_path / "results.xml"),
        COVERAGE_TEST_FAULT=fault,
    )
    if coverage:
        environment.update(
            COVERAGE_MANIFEST=str(manifest),
            COVERAGE_DIR=str(directory),
            COVERAGE_OUTPUT_FILE=str(report),
        )
    result = subprocess.run(
        [str(ROOT / target), "-o", f"python_files={Path(target).name}.py"],
        cwd=ROOT,
        env=environment,
        text=True,
        capture_output=True,
        check=False,
        timeout=30,
    )
    return result, report


def test_generated_binary_and_imports_share_source_identities(tmp_path: Path) -> None:
    result, report = run_test(
        tmp_path,
        "python_sample/cli_test",
        [
            "python_sample/__main__.py",
            "python_sample/cli.py",
            "python_sample/versions.py",
        ],
    )
    assert result.returncode == 0, result.stdout + result.stderr
    records = report.read_text().split("end_of_record")
    entrypoint = next(r for r in records if "SF:python_sample/__main__.py\n" in r)
    assert "DA:6,1\n" in entrypoint
    assert "BRH:1\n" in entrypoint
    assert "BRF:2\n" in entrypoint
    assert sum("SF:python_sample/cli.py\n" in r for r in records) == 1
    assert not any("SF:/" in r for r in records)


def test_current_interpreter_records_script_lines_and_branches(tmp_path: Path) -> None:
    result, report = run_test(
        tmp_path,
        ".github/scripts/bazel_dependencies_test",
        [".github/scripts/bazel_dependencies.py"],
    )
    assert result.returncode == 0, result.stdout + result.stderr
    text = report.read_text()
    assert text.count("SF:.github/scripts/bazel_dependencies.py\n") == 1
    assert "DA:41,0\n" in text
    assert "DA:43,1\n" in text
    assert "BRF:14\n" in text
    assert "BRH:9\n" in text


@pytest.mark.parametrize("fault", ["collection", "combination"])
def test_child_coverage_failure_fails_after_passing_tests(
    tmp_path: Path, fault: str
) -> None:
    result, report = run_test(
        tmp_path,
        "tools/python/coverage_fixture_test",
        ["tools/python/coverage_fixture_test.py"],
        fault=fault,
    )
    assert "1 passed" in result.stdout, result.stdout + result.stderr
    assert result.returncode != 0
    assert not report.exists()
    if fault == "collection":
        assert "Python child coverage did not finish" in result.stderr
    else:
        assert "Couldn't use data file" in result.stderr


@pytest.mark.parametrize("fault", ["", "collection"])
def test_ordinary_process_tests_pass_without_coverage(
    tmp_path: Path, fault: str
) -> None:
    result, report = run_test(
        tmp_path,
        "tools/python/coverage_fixture_test",
        [],
        fault=fault,
        coverage=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert not report.exists()
