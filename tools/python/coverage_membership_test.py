"""Check complete coverage through Bazel with temporary production packages."""

import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

SOURCE = """def choose(value):
    if value:
        return 1
    return 0
"""


def command(root: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bazel", *arguments],
        cwd=root,
        capture_output=True,
        text=True,
        check=False,
    )


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


def main() -> None:
    root = Path(os.environ["BUILD_WORKSPACE_DIRECTORY"])
    evidence = Path(sys.argv[1]).absolute()
    evidence.mkdir(parents=True, exist_ok=True)
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
        targets = [
            "//python_sample/...",
            "//.github/...",
            "//tools/python/...",
            f"//{ordinary.relative_to(root)}/...",
        ]
        result = command(root, "coverage", *targets, "--config=ci")
        (evidence / "coverage.log").write_text(result.stdout + result.stderr)
        assert result.returncode == 0, result.stdout + result.stderr
        output = command(root, "info", "output_path")
        assert output.returncode == 0, output.stdout + output.stderr
        report = Path(output.stdout.strip()) / "_coverage/_coverage_report.dat"
        shutil.copyfile(report, evidence / "combined.lcov")
        check_report(
            report.read_text(),
            str(ordinary.relative_to(root)),
            str(hidden.relative_to(root)),
        )
        # The new packages must not become dependencies of any test.
        query = command(
            root,
            "query",
            f'kind("py_test rule", rdeps(//..., //{ordinary.relative_to(root)}:untested.py))',
        )
        assert query.returncode == 0, query.stdout + query.stderr
        assert not query.stdout.strip(), query.stdout
        (evidence / "outside-test-dependencies.log").write_text(
            query.stdout + query.stderr
        )
        source = ordinary / "untested.py"
        source.write_text("def broken(\n")
        # Gazelle must run immediately, including after a deliberate source fault.
        generation = command(root, "run", "//:gazelle")
        (evidence / "invalid-source-gazelle.log").write_text(
            generation.stdout + generation.stderr
        )
        result = command(root, "coverage", *targets, "--config=ci", "--keep_going")
        text = result.stdout + result.stderr
        (evidence / "invalid-source.log").write_text(text)
        assert result.returncode != 0, text
        assert "PythonCoverageBaseline" in text and "Couldn't parse" in text, text
    print(f"Coverage membership checks passed. Evidence: {evidence}")


if __name__ == "__main__":
    main()
