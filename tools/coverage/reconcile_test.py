"""Check final source reconciliation through its executable command."""

import json
import os
import subprocess
from collections.abc import Mapping
from pathlib import Path

import pytest

RUNFILES = Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"]


def test_unexplained_source_fails(tmp_path: Path) -> None:
    inventory = {
        "eligible": [
            {"path": "new.py", "language": "python", "owners": [{"label": "//:lib"}]}
        ],
        "excluded": [],
        "missing_owners": [],
    }
    (tmp_path / "inventory.json").write_text(json.dumps(inventory))
    (tmp_path / "report.lcov").write_text(
        "SF:other.py\nDA:1,1\nLF:1\nLH:1\nend_of_record\n"
    )
    result = subprocess.run(
        [
            str(RUNFILES / "tools/coverage/reconcile"),
            "--inventory",
            "inventory.json",
            "--report",
            "report.lcov",
            "--output",
            "receipt.json",
        ],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode != 0
    assert "new.py" in result.stderr
    assert not (tmp_path / "receipt.json").exists()


def run_command(
    directory: Path,
    inventory: Mapping[str, object],
    report: str,
    **receipts: Mapping[str, object],
) -> subprocess.CompletedProcess[str]:
    (directory / "inventory.json").write_text(json.dumps(inventory))
    (directory / "report.lcov").write_text(report)
    arguments = []
    for family, receipt in receipts.items():
        path = directory / f"{family}.json"
        path.write_text(json.dumps(receipt))
        arguments.extend([f"--{family}", str(path)])
    return subprocess.run(
        [
            str(RUNFILES / "tools/coverage/reconcile"),
            "--inventory",
            "inventory.json",
            "--report",
            "report.lcov",
            "--output",
            "receipt.json",
            *arguments,
        ],
        cwd=directory,
        capture_output=True,
        text=True,
        check=False,
    )


@pytest.mark.parametrize(
    "record",
    [
        "SF:module.py\nend_of_record\n",
        "SF:module.py\nDA:1,-1\nLF:1\nLH:0\nend_of_record\n",
        "SF:module.py\nDA:0,1\nLF:1\nLH:1\nend_of_record\n",
        "SF:module.py\nDA:1,1\nLF:2\nLH:1\nend_of_record\n",
        "SF:module.py\nDA:1,1\nLF:1\nLH:0\nend_of_record\n",
        "SF:module.py\nDA:1,1\nLF:1\nLH:1\n",
        "SF:module.py\nSF:other.py\nDA:1,1\nLF:1\nLH:1\nend_of_record\n",
    ],
)
def test_invalid_executable_record_fails(tmp_path: Path, record: str) -> None:
    inventory = {
        "eligible": [
            {"path": "module.py", "language": "python", "owners": [{"label": "//:lib"}]}
        ],
        "excluded": [],
        "missing_owners": [],
    }
    result = run_command(tmp_path, inventory, record)
    assert result.returncode != 0
    assert not (tmp_path / "receipt.json").exists()


@pytest.mark.parametrize(
    "family,extension",
    [("go", "go"), ("rust", "rs"), ("jvm", "kt"), ("javascript", "ts")],
)
@pytest.mark.parametrize("allowed", [False, True])
def test_only_record_limits_explain_absence(
    tmp_path: Path, family: str, extension: str, allowed: bool
) -> None:
    name = f"hidden/source.{extension}"
    source = {
        "path": name,
        "language": family,
        "owners": [{"label": "//hidden:source"}],
    }
    limit = {
        "source": name,
        "rule": "//hidden:source",
        "evidence": "configured-object.json",
        "missing_guarantee": "This configured collector cannot identify the original source.",
        "record_absence": allowed,
    }
    receipt = {"sources": {}, "limits": [limit]}
    result = run_command(
        tmp_path,
        {"eligible": [source], "excluded": [], "missing_owners": []},
        "SF:other.py\nDA:1,1\nLF:1\nLH:1\nend_of_record\n",
        **{family: receipt},
    )
    assert result.returncode == (0 if allowed else 1), result.stderr
    if allowed:
        output = json.loads((tmp_path / "receipt.json").read_text())
        assert output["sources"][0]["status"] == "collector_limit"
        assert output["sources"][0]["limits"] == [limit]


@pytest.mark.parametrize(
    "family,extension",
    [("go", "go"), ("rust", "rs"), ("jvm", "java"), ("javascript", "js")],
)
def test_collector_lines_and_hits_survive(
    tmp_path: Path, family: str, extension: str
) -> None:
    name = f"ordinary/file.{extension}"
    report = f"SF:{name}\nDA:1,7\nDA:2,0\nLF:2\nLH:1\nend_of_record\n"
    source = {"path": name, "language": family, "owners": [{"label": "//ordinary:lib"}]}
    receipt = {"sources": {name: [1, 2]}, "limits": []}
    result = run_command(
        tmp_path,
        {"eligible": [source], "excluded": [], "missing_owners": []},
        report,
        **{family: receipt},
    )
    assert result.returncode == 0, result.stderr
    assert (tmp_path / "report.lcov").read_text() == report
    output = json.loads((tmp_path / "receipt.json").read_text())
    assert output["sources"][0]["status"] == "measured"
    assert output["sources"][0]["executable_lines"] == 2
    assert output["sources"][0]["hit_lines"] == 1


@pytest.mark.parametrize(
    "case", ["missing_root", "unowned", "lost_line", "empty_record"]
)
def test_python_inventory_requires_executable_evidence(
    tmp_path: Path, case: str
) -> None:
    (tmp_path / "module.py").write_text("VALUE = 1\nOTHER = 2\n")
    source = {
        "path": "module.py",
        "language": "python",
        "owners": [] if case == "unowned" else [{"label": "//:lib"}],
    }
    inventory = {
        "eligible": [source],
        "excluded": [],
        "missing_owners": [],
        "baseline_sources": [] if case == "missing_root" else ["module.py"],
    }
    report = "SF:module.py\nDA:1,0\nDA:2,0\nLF:2\nLH:0\nend_of_record\n"
    if case == "lost_line":
        report = "SF:module.py\nDA:1,0\nLF:1\nLH:0\nend_of_record\n"
    if case == "empty_record":
        report = "SF:module.py\nLF:0\nLH:0\nend_of_record\n"
    result = run_command(tmp_path, inventory, report)
    assert result.returncode != 0
    assert "module.py" in result.stderr


@pytest.mark.parametrize(
    "code,status,record",
    [
        (
            "VALUE = 1\n",
            "zero_hit",
            "SF:module.py\nDA:1,0\nLF:1\nLH:0\nend_of_record\n",
        ),
        (
            '"""Only documentation."""\n',
            "no_executable_lines",
            "SF:other.py\nDA:1,1\nLF:1\nLH:1\nend_of_record\n",
        ),
    ],
)
def test_python_public_analysis_distinguishes_empty_sources(
    tmp_path: Path, code: str, status: str, record: str
) -> None:
    (tmp_path / "module.py").write_text(code)
    inventory = {
        "eligible": [
            {"path": "module.py", "language": "python", "owners": [{"label": "//:lib"}]}
        ],
        "excluded": [],
        "missing_owners": [],
        "baseline_sources": ["module.py"],
    }
    result = run_command(tmp_path, inventory, record)
    assert result.returncode == 0, result.stderr
    output = json.loads((tmp_path / "receipt.json").read_text())
    assert output["sources"][0]["status"] == status


@pytest.mark.parametrize(
    "case",
    [
        "lost_record",
        "lost_line",
        "no_collector_source",
        "empty_unexplained",
        "stale_output",
    ],
)
def test_limits_do_not_suppress_required_output_failures(
    tmp_path: Path, case: str
) -> None:
    name = "module.js"
    limit = {
        "source": name,
        "rule": "//:lib",
        "evidence": "raw.json",
        "missing_guarantee": "Original mapping is limited.",
        "record_absence": True,
    }
    inventory = {
        "eligible": [
            {"path": name, "language": "javascript", "owners": [{"label": "//:lib"}]}
        ],
        "excluded": [],
        "missing_owners": [],
    }
    receipt = {"sources": {name: [1, 2]}, "limits": [limit]}
    report = "SF:module.js\nDA:1,1\nDA:2,0\nLF:2\nLH:1\nend_of_record\n"
    if case == "lost_record":
        report = "SF:other.py\nDA:1,1\nLF:1\nLH:1\nend_of_record\n"
    elif case in ("lost_line", "stale_output"):
        report = "SF:module.js\nDA:1,1\nLF:1\nLH:1\nend_of_record\n"
    elif case == "no_collector_source":
        receipt = {"sources": {}, "limits": []}
    elif case == "empty_unexplained":
        receipt = {"sources": {}, "limits": []}
        report = "SF:module.js\nLF:0\nLH:0\nend_of_record\n"
    if case == "stale_output":
        (tmp_path / "receipt.json").write_text('{"success": true}')
    result = run_command(tmp_path, inventory, report, javascript=receipt)
    assert result.returncode != 0
    assert not (tmp_path / "receipt.json").exists()


def test_scope_filter_preserves_eligible_records(tmp_path: Path) -> None:
    source = {
        "path": "included.js",
        "language": "javascript",
        "owners": [{"label": "//:lib"}],
    }
    inventory = {
        "eligible": [source],
        "excluded": [
            {
                "path": "helper.sh",
                "reason": "excluded language or non-executable content",
            }
        ],
        "missing_owners": [],
    }
    included = "SF:included.js\nDA:1,5\nLF:1\nLH:1\nend_of_record\n"
    excluded = "SF:helper.sh\nDA:1,2\nLF:1\nLH:1\nend_of_record\n"
    result = run_command(
        tmp_path,
        inventory,
        included + excluded,
        javascript={"sources": {"included.js": [1]}, "limits": []},
    )
    assert result.returncode == 0, result.stderr
    assert (tmp_path / "report.lcov").read_text() == included
    output = json.loads((tmp_path / "receipt.json").read_text())
    assert output["excluded"] == inventory["excluded"]


@pytest.mark.parametrize(
    "branches,valid",
    [
        ("BRDA:1,0,0,7\nBRDA:1,0,1,-\nBRF:2\nBRH:1\n", True),
        ("BRDA:1,0,0,-2\nBRF:1\nBRH:0\n", False),
        ("BRDA:1,0,0,7\nBRF:2\nBRH:1\n", False),
        ("BRDA:1,0,0,7\nBRF:1\nBRH:0\n", False),
        ("BRDA:1,0,0,7\nBRDA:1,0,0,7\nBRF:2\nBRH:2\n", False),
    ],
)
def test_branch_records_retain_counts_or_fail(
    tmp_path: Path, branches: str, valid: bool
) -> None:
    inventory = {
        "eligible": [
            {
                "path": "module.js",
                "language": "javascript",
                "owners": [{"label": "//:lib"}],
            }
        ],
        "excluded": [],
        "missing_owners": [],
    }
    report = "SF:module.js\nDA:1,1\nLF:1\nLH:1\n" + branches + "end_of_record\n"
    result = run_command(
        tmp_path,
        inventory,
        report,
        javascript={"sources": {"module.js": [1]}, "limits": []},
    )
    assert result.returncode == (0 if valid else 1), result.stderr
    if valid:
        assert (tmp_path / "report.lcov").read_text() == report


@pytest.mark.parametrize("allowed", [False, True])
def test_rust_summary_gap_needs_an_exact_line_limit(
    tmp_path: Path, allowed: bool
) -> None:
    inventory = {
        "eligible": [
            {"path": "module.rs", "language": "rust", "owners": [{"label": "//:lib"}]}
        ],
        "excluded": [],
        "missing_owners": [],
    }
    limit = {
        "source": "module.rs",
        "rule": "//:lib",
        "evidence": "baseline.lcov",
        "missing_guarantee": "LLVM does not identify all summarized executable lines.",
        "line_summary": True,
    }
    receipt = {"sources": {"module.rs": [1]}, "limits": [limit] if allowed else []}
    result = run_command(
        tmp_path,
        inventory,
        "SF:module.rs\nDA:1,1\nLF:2\nLH:1\nend_of_record\n",
        rust=receipt,
    )
    assert result.returncode == (0 if allowed else 1), result.stderr


@pytest.mark.parametrize("lines", [[True], [1, 1], "1"])
def test_corrupt_collector_line_sets_fail(
    tmp_path: Path, lines: list[bool] | list[int] | str
) -> None:
    inventory = {
        "eligible": [
            {
                "path": "module.js",
                "language": "javascript",
                "owners": [{"label": "//:lib"}],
            }
        ],
        "excluded": [],
        "missing_owners": [],
    }
    result = run_command(
        tmp_path,
        inventory,
        "SF:module.js\nDA:1,1\nLF:1\nLH:1\nend_of_record\n",
        javascript={"sources": {"module.js": lines}, "limits": []},
    )
    assert result.returncode != 0


@pytest.mark.parametrize(
    "found,hit,count,valid",
    [
        (2, 0, 7, False),
        (2, 2, 0, False),
        (1, 0, 7, False),
        (2, 2, 7, True),
        (2, 1, 0, True),
    ],
)
def test_rust_summary_limits_preserve_known_hit_constraints(
    tmp_path: Path, found: int, hit: int, count: int, valid: bool
) -> None:
    inventory = {
        "eligible": [
            {"path": "module.rs", "language": "rust", "owners": [{"label": "//:lib"}]}
        ],
        "excluded": [],
        "missing_owners": [],
    }
    limit = {
        "source": "module.rs",
        "rule": "//:lib",
        "evidence": "baseline.lcov",
        "missing_guarantee": "LLVM does not identify all summarized executable lines.",
        "line_summary": True,
    }
    report = f"SF:module.rs\nDA:1,{count}\nLF:{found}\nLH:{hit}\nend_of_record\n"
    result = run_command(
        tmp_path,
        inventory,
        report,
        rust={"sources": {"module.rs": [1]}, "limits": [limit]},
    )
    assert result.returncode == (0 if valid else 1), result.stderr
    if valid:
        output = json.loads((tmp_path / "receipt.json").read_text())
        assert output["sources"][0]["status"] == (
            "measured" if count else "collector_limit"
        )
        assert (tmp_path / "report.lcov").read_text() == report
