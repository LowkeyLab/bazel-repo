"""Exercise the reference project's observable command output."""

import os
import subprocess
from pathlib import Path

import pytest

from cli import main


def test_command_prints_one_version_per_line(
    capsys: pytest.CaptureFixture[str],
) -> None:
    main(["10.0", "2.0", "1.0rc1", "1.0"])
    assert capsys.readouterr().out == "1.0rc1\n1.0\n2.0\n10.0\n"


def test_generated_executable_starts_with_isolated_imports() -> None:
    binary = (
        Path(os.environ["TEST_SRCDIR"])
        / os.environ["TEST_WORKSPACE"]
        / "python_sample/python_sample_bin"
    )
    result = subprocess.run(
        [str(binary), "10.0", "2.0", "1.0rc1", "1.0"],
        check=True,
        capture_output=True,
        text=True,
    )
    assert result.stdout == "1.0rc1\n1.0\n2.0\n10.0\n"
