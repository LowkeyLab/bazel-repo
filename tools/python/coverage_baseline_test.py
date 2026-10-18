"""Check baseline actions with real source files and the real collector."""

import os
import subprocess
from pathlib import Path

import pytest

ROOT = Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"]


@pytest.mark.parametrize("symlink", [False, True])
def test_unexecuted_source_has_zero_lines_and_branches(
    tmp_path: Path, symlink: bool
) -> None:
    working = tmp_path / "workspace"
    working.mkdir()
    source = working / "subject.py"
    if symlink:
        source.symlink_to(tmp_path / "original.py")
    source.write_text(
        "def choose(value):\n    if value:\n        return 1\n    return 0\n"
    )
    report = tmp_path / "baseline.lcov"
    result = subprocess.run(
        [str(ROOT / "tools/python/coverage_baseline"), str(report), "subject.py"],
        cwd=working,
        capture_output=True,
        text=True,
        check=False,
        timeout=30,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    text = report.read_text()
    assert "SF:subject.py\n" in text
    assert "DA:1,0\nDA:2,0\nDA:3,0\nDA:4,0\n" in text
    assert "BRF:2\nBRH:0\n" in text
    assert "FN:1,choose\n" in text
    assert "FNDA:0,choose\n" in text


@pytest.mark.parametrize("source", [None, "def broken(\n"])
def test_missing_or_invalid_source_fails_baseline(
    tmp_path: Path, source: str | None
) -> None:
    if source is not None:
        (tmp_path / "subject.py").write_text(source)
    report = tmp_path / "baseline.lcov"
    result = subprocess.run(
        [str(ROOT / "tools/python/coverage_baseline"), str(report), "subject.py"],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        check=False,
        timeout=30,
    )
    assert result.returncode != 0
    assert not report.exists()
