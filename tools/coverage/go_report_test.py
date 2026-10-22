"""Check Go report scope and required-output validation through the command."""

import hashlib
import json
import os
import subprocess
from pathlib import Path

import pytest

ROOT = Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"]
MEASURED = "SF:kept.go\nDA:4,7\nLF:1\nLH:1\nend_of_record\n"
EMPTY = "SF:declaration.go\nLF:0\nLH:0\nend_of_record\n"
OTHER = "SF:keep.rs\nDA:9,3\nLF:1\nLH:1\nend_of_record\n"


def invoke(
    tmp_path: Path, report: str, fault: str | None = None
) -> subprocess.CompletedProcess[str]:
    (tmp_path / "inventory.json").write_text(
        json.dumps(
            {
                "coverage_roots": ["//fixture:fixture.go_coverage"],
                "excluded": [
                    {"path": "generated.go", "reason": "generated"},
                    {"path": "keep.rs", "reason": "generated"},
                ],
            }
        )
    )
    (tmp_path / "baseline.lcov").write_text(MEASURED)
    artifact = {
        "name": str(tmp_path / "baseline.lcov"),
        "digest": hashlib.sha256(
            b"" if fault == "empty" else MEASURED.encode()
        ).hexdigest(),
        "length": "0" if fault == "empty" else str(len(MEASURED.encode())),
    }
    events = [
        {
            "id": {"namedSet": {"id": "baseline"}},
            "namedSetOfFiles": {"files": [artifact]},
        },
        {
            "id": {"targetCompleted": {"label": "//fixture:fixture.go_coverage"}},
            "completed": {
                "success": True,
                "outputGroup": [
                    {
                        "name": "go_coverage_baselines",
                        "fileSets": [{"id": "baseline"}],
                        "incomplete": fault == "incomplete",
                    }
                ],
            },
        },
    ]
    if fault == "aspect":
        events.insert(
            1,
            {
                "id": {
                    "targetCompleted": {
                        "label": "//fixture:fixture.go_coverage",
                        "aspect": "//tools/coverage:jvm.bzl%jvm_test_inputs",
                    }
                },
                "completed": {"success": True},
            },
        )
    if fault == "root":
        events.pop()
    elif fault == "artifact":
        (tmp_path / "baseline.lcov").unlink()
    elif fault == "empty":
        (tmp_path / "baseline.lcov").write_text("")
    if fault == "truncated":
        (tmp_path / "baseline.lcov").write_text("")
    elif fault == "changed":
        (tmp_path / "baseline.lcov").write_text(MEASURED.replace("kept.go", "lost.go"))
    elif fault == "digest":
        artifact.pop("digest")
    (tmp_path / "events.jsonl").write_text(
        "".join(json.dumps(event) + "\n" for event in events)
    )
    (tmp_path / "report.lcov").write_text(report)
    return subprocess.run(
        [
            str(ROOT / "tools/coverage/go_report"),
            "--events",
            str(tmp_path / "events.jsonl"),
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


@pytest.mark.parametrize(
    "report", [OTHER, "SF:kept.go\nLF:0\nLH:0\nend_of_record\n" + OTHER]
)
def test_missing_native_source_or_line_fails(tmp_path: Path, report: str) -> None:
    result = invoke(tmp_path, report)
    assert result.returncode != 0
    assert "Missing Go baseline" in result.stderr
    assert (tmp_path / "report.lcov").read_text() == report


@pytest.mark.parametrize("fault", ["root", "artifact"])
def test_missing_required_baseline_evidence_fails(tmp_path: Path, fault: str) -> None:
    result = invoke(tmp_path, MEASURED, fault)
    assert result.returncode != 0
    assert "Go coverage report failed:" in result.stderr
    assert (tmp_path / "report.lcov").read_text() == MEASURED


def test_configured_root_without_matching_sources_has_explicit_empty_evidence(
    tmp_path: Path,
) -> None:
    result = invoke(tmp_path, OTHER, "empty")
    assert result.returncode == 0, result.stderr
    assert (tmp_path / "report.lcov").read_text() == OTHER


def test_rust_baseline_validation_preserves_hits_and_records_mapping_limits(
    tmp_path: Path,
) -> None:
    measured = MEASURED.replace("kept.go", "kept.rs")
    baseline = measured.replace("DA:4,7", "DA:4,0").replace("LH:1", "LH:0")
    generated = "SF:generated.rs\nDA:1,9\nLF:1\nLH:1\nend_of_record\n"
    (tmp_path / "baseline.lcov").write_text(baseline + generated)
    (tmp_path / "baseline.json").write_text(
        json.dumps(
            {
                "object": "fixture.rlib",
                "llvm_cov": "llvm-cov",
                "llvm_profdata": "llvm-profdata",
                "generated_sources": ["generated.rs"],
                "children": [["kept.rs", "//fixture:child", "child-object"]],
                "no_mapping": False,
                "diagnostic": "",
            }
        )
    )
    label = "//fixture:fixture.rust_coverage"
    (tmp_path / "events.jsonl").write_text(
        "\n".join(
            json.dumps(event)
            for event in [
                {
                    "id": {"namedSet": {"id": "baseline"}},
                    "namedSetOfFiles": {
                        "files": [
                            {
                                "name": str(tmp_path / "baseline.lcov"),
                                "digest": hashlib.sha256(
                                    (tmp_path / "baseline.lcov").read_bytes()
                                ).hexdigest(),
                                "length": str(
                                    (tmp_path / "baseline.lcov").stat().st_size
                                ),
                            },
                            {
                                "name": str(tmp_path / "baseline.json"),
                                "digest": hashlib.sha256(
                                    (tmp_path / "baseline.json").read_bytes()
                                ).hexdigest(),
                                "length": str(
                                    (tmp_path / "baseline.json").stat().st_size
                                ),
                            },
                        ]
                    },
                },
                {
                    "id": {"targetCompleted": {"label": label}},
                    "completed": {
                        "success": True,
                        "outputGroup": [
                            {
                                "name": "rust_coverage_baselines",
                                "fileSets": [{"id": "baseline"}],
                            }
                        ],
                    },
                },
            ]
        )
    )
    (tmp_path / "inventory.json").write_text(
        json.dumps(
            {
                "coverage_roots": [label],
                "excluded": [],
                "eligible": [
                    {"path": name, "language": "rust", "owners": [{"label": label}]}
                    for name in ["kept.rs", "declaration.rs"]
                ],
            }
        )
    )
    (tmp_path / "report.lcov").write_text(measured + EMPTY + generated)
    arguments = [
        str(ROOT / "tools/coverage/go_report"),
        "--language",
        "rust",
        "--events",
        str(tmp_path / "events.jsonl"),
        "--inventory",
        str(tmp_path / "inventory.json"),
        "--report",
        str(tmp_path / "report.lcov"),
        "--expected-output",
        str(tmp_path / "receipt.json"),
    ]
    result = subprocess.run(arguments, capture_output=True, text=True, check=False)
    assert result.returncode == 0, result.stdout + result.stderr
    assert (tmp_path / "report.lcov").read_text() == measured + EMPTY
    receipt = json.loads((tmp_path / "receipt.json").read_text())
    assert receipt["sources"] == {"kept.rs": [4]}
    assert {limit["source"] for limit in receipt["limits"]} == {
        "declaration.rs",
        "kept.rs",
    }
    assert any(
        limit["rule"] == "//fixture:child"
        and limit["object"] == "child-object"
        and "profile" in limit["missing_guarantee"]
        for limit in receipt["limits"]
    )
    (tmp_path / "report.lcov").write_text(EMPTY)
    result = subprocess.run(arguments, capture_output=True, text=True, check=False)
    assert result.returncode != 0
    assert "Missing Rust baseline" in result.stderr


def test_baseline_ignores_independent_aspect_events(tmp_path: Path) -> None:
    result = invoke(tmp_path, MEASURED + OTHER, "aspect")
    assert result.returncode == 0, result.stdout + result.stderr
    assert (tmp_path / "report.lcov").read_text() == MEASURED + OTHER


@pytest.mark.parametrize("fault", ["truncated", "changed", "digest", "incomplete"])
def test_invalid_baseline_evidence_cannot_become_a_source_limit(
    tmp_path: Path, fault: str
) -> None:
    result = invoke(tmp_path, MEASURED + OTHER, fault)
    assert result.returncode != 0
    assert "Go coverage report failed:" in result.stderr
    assert (tmp_path / "report.lcov").read_text() == MEASURED + OTHER
