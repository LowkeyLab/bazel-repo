"""Check Go report scope and required-output validation through the command."""

import json
import os
import subprocess
from pathlib import Path

import pytest

ROOT = Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"]
MEASURED = "SF:kept.go\nDA:4,7\nLF:1\nLH:1\nend_of_record\n"
EMPTY = "SF:declaration.go\nLF:0\nLH:0\nend_of_record\n"
OTHER = "SF:keep.rs\nDA:9,3\nLF:1\nLH:1\nend_of_record\n"


def invoke(tmp_path: Path, report: str) -> subprocess.CompletedProcess[str]:
    (tmp_path / "inventory.json").write_text(
        json.dumps(
            {
                "excluded": [
                    {"path": "generated.go", "reason": "generated"},
                    {"path": "keep.rs", "reason": "generated"},
                ]
            }
        )
    )
    (tmp_path / "report.lcov").write_text(report)
    return subprocess.run(
        [
            str(ROOT / "tools/coverage/go_report"),
            "--inventory",
            str(tmp_path / "inventory.json"),
            "--report",
            str(tmp_path / "report.lcov"),
        ],
        capture_output=True,
        text=True,
        check=False,
    )


def test_scope_retains_measured_hits_empty_sets_and_other_languages(
    tmp_path: Path,
) -> None:
    excluded = "SF:generated.go\nDA:3,1\nLF:1\nLH:1\nend_of_record\n"
    result = invoke(tmp_path, MEASURED + excluded + EMPTY + OTHER)
    assert result.returncode == 0, result.stdout + result.stderr
    assert (tmp_path / "report.lcov").read_text() == MEASURED + EMPTY + OTHER


@pytest.mark.parametrize(
    "report",
    [
        "",
        "SF:kept.go\nDA:4,7\n",
        "SF:kept.go\nLF:1\nLH:0\nend_of_record\n",
        "SF:kept.go\nDA:4,7\nLF:1\nLH:0\nend_of_record\n",
        "SF:kept.go\nDA:4,7\nDA:4,7\nLF:2\nLH:2\nend_of_record\n",
        "SF:kept.go\nDA:4,-1\nLF:1\nLH:0\nend_of_record\n",
        "SF:kept.go\nDA:0,1\nLF:1\nLH:1\nend_of_record\n",
        "SF:kept.go\nDA:bad\nLF:1\nLH:1\nend_of_record\n",
        "LF:0\nLH:0\nend_of_record\n",
    ],
)
def test_invalid_required_output_fails_without_replacing_report(
    tmp_path: Path, report: str
) -> None:
    result = invoke(tmp_path, report)
    assert result.returncode != 0
    assert "Go coverage report failed:" in result.stderr
    assert (tmp_path / "report.lcov").read_text() == report
