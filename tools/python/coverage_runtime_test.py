"""Check coverage reports and failures through real Bazel-built test commands."""

import os
import subprocess
from pathlib import Path

import coverage
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


@pytest.mark.parametrize(
    ("children", "fault"),
    [
        (False, ""),
        (True, ""),
        (True, "completion"),
        (True, "missing"),
        (True, "corrupt"),
    ],
)
def test_nested_collection_reports_real_execution(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, children: bool, fault: str
) -> None:
    import coverage

    from tools.python import coverage_runtime

    outer = coverage.Coverage.current()
    subject = ROOT / "python_sample/__main__.py"
    directory = tmp_path / "nested"
    directory.mkdir()
    with monkeypatch.context() as environment:
        environment.setenv("COVERAGE_DIR", str(directory))
        for name in (
            "BAZEL_PYTHON_COVERAGE",
            "BAZEL_PYTHON_COVERAGE_CONFIG",
            "BAZEL_PYTHON_COVERAGE_HOOK",
        ):
            environment.setenv(name, os.environ.get(name, ""))
        collector = coverage_runtime.start(
            [str(subject), str(Path(__file__)), str(Path(coverage_runtime.__file__))]
        )
        _exercise_nested_collection(collector, outer, tmp_path, children, fault)


def _exercise_nested_collection(
    collector: coverage.Coverage,
    outer: coverage.Coverage | None,
    tmp_path: Path,
    children: bool,
    fault: str,
) -> None:
    from tools.python import coverage_runtime

    directory = tmp_path / "nested"
    subject = ROOT / "python_sample/__main__.py"
    if children:
        result = subprocess.run(
            [str(ROOT / "python_sample/python_sample_bin"), "2.0", "1.0"],
            capture_output=True,
            text=True,
            check=True,
            timeout=30,
        )
        assert result.stdout == "1.0\n2.0\n"
    if fault:
        root = directory / "python-processes"
        if fault == "completion":
            for receipt in root.glob("*.saved"):
                receipt.unlink()
        else:
            for data_file in root.glob(".coverage.*"):
                if fault == "missing":
                    data_file.unlink()
                else:
                    data_file.write_bytes(b"invalid coverage data")
        with pytest.raises((RuntimeError, coverage.exceptions.DataError)):
            coverage_runtime.finish(collector)
        assert coverage.Coverage.current() is outer
        return
    coverage_runtime.finish(collector)
    _check_nested_report(collector, outer, subject, tmp_path / "nested.lcov", children)


def _check_nested_report(
    collector: coverage.Coverage,
    outer: coverage.Coverage | None,
    subject: Path,
    report: Path,
    children: bool,
) -> None:
    assert coverage.Coverage.current() is outer
    collector.lcov_report(outfile=str(report))
    if children:
        assert "DA:6,1\n" in report.read_text()
        assert "BRH:1\n" in report.read_text()
    else:
        assert str(subject) not in report.read_text()
    if outer is not None:
        outer.get_data().update(collector.get_data())


@pytest.mark.parametrize("fault", ["", "startup", "receipt"])
def test_child_bootstrap_completes_or_reports_collection_failure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, fault: str
) -> None:
    import coverage

    from tools.python import coverage_runtime

    outer = coverage.Coverage.current()
    directory = tmp_path / "bootstrap"
    directory.mkdir()
    observed = tmp_path / "observed.coverage"
    runtime = str(Path(coverage_runtime.__file__).resolve())
    with monkeypatch.context() as environment:
        environment.setenv("COVERAGE_DIR", str(directory))
        for name in (
            "BAZEL_PYTHON_COVERAGE",
            "BAZEL_PYTHON_COVERAGE_CONFIG",
            "BAZEL_PYTHON_COVERAGE_HOOK",
        ):
            environment.setenv(name, os.environ.get(name, ""))
        collector = coverage_runtime.start([runtime, str(Path(__file__))])
        _exercise_child_bootstrap(collector, outer, runtime, observed, fault)


def _exercise_child_bootstrap(
    collector: coverage.Coverage,
    outer: coverage.Coverage | None,
    runtime: str,
    observed: Path,
    fault: str,
) -> None:
    import sys

    from tools.python import coverage_runtime

    child_environment = dict(os.environ)
    child_environment.pop("BAZEL_PYTHON_COVERAGE")
    # An independent real collector measures the complete startup path.
    program = """
import atexit
import os
import runpy
import sys
from pathlib import Path
import coverage

runtime, observed, fault = sys.argv[1:]
observer = coverage.Coverage(config_file=False, data_file=observed, include=[runtime], branch=True)
observer.set_option("run:patch", ["_exit"])
observer.start()
def finish_observer():
    observer.stop()
    observer.save()
atexit.register(finish_observer)
if fault == "startup":
    os.environ["COVERAGE_CORE"] = "invalid"
runpy.run_path(runtime, run_name="__bazel_coverage_child__")
if fault == "receipt":
    root = Path(os.environ["BAZEL_PYTHON_COVERAGE_CONFIG"]).parent
    for receipt in root.glob(f"{os.getpid()}-*.started"):
        receipt.with_suffix(".saved").mkdir()
print("application completed", flush=True)
"""
    result = subprocess.run(
        [sys.executable, "-c", program, runtime, str(observed), fault],
        env=child_environment,
        text=True,
        capture_output=True,
        check=False,
        timeout=30,
    )
    if fault:
        assert result.returncode != 0, result.stdout + result.stderr
        with pytest.raises(RuntimeError, match="Python child coverage did not finish"):
            coverage_runtime.finish(collector)
    else:
        assert result.returncode == 0, result.stdout + result.stderr
        coverage_runtime.finish(collector)
    _check_bootstrap_report(collector, outer, result, fault, observed, runtime)


def _check_bootstrap_report(
    collector: coverage.Coverage,
    outer: coverage.Coverage | None,
    result: subprocess.CompletedProcess[str],
    fault: str,
    observed: Path,
    runtime: str,
) -> None:
    if fault != "startup":
        assert "application completed" in result.stdout
    assert coverage.Coverage.current() is outer
    measured = coverage.CoverageData(basename=str(observed))
    measured.read()
    assert runtime in measured.measured_files()
    if outer is not None:
        outer.get_data().update(measured)
        outer.get_data().update(collector.get_data())
