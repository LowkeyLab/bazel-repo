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


@pytest.mark.parametrize(
    "fault",
    [
        None,
        "no-mapping",
        "summary-gap",
        "bad-metadata",
        "missing-metadata",
        "empty-mapping",
        "conflict",
        "bad-diagnostic",
        "generated",
        "large-integer",
        "missing-line",
    ],
)
def test_rust_mapping_evidence_and_summary_limits(
    tmp_path: Path, fault: str | None
) -> None:
    label = "//fixture:fixture.rust_coverage"
    measured = "SF:kept.rs\nDA:4,7\nDA:8,0\nLF:2\nLH:1\nend_of_record\n"
    baseline = "SF:kept.rs\nDA:4,0\nLF:1\nLH:0\nend_of_record\n"
    metadata = {
        "object": "fixture.rlib",
        "llvm_cov": "llvm-cov",
        "llvm_profdata": "llvm-profdata",
        "generated_sources": [],
        "children": [],
        "no_mapping": False,
        "diagnostic": "",
    }
    if fault in {"no-mapping", "bad-diagnostic"}:
        metadata.update(
            no_mapping=True,
            diagnostic="no coverage data found"
            if fault == "no-mapping"
            else "unexpected",
        )
        baseline = ""
    elif fault == "summary-gap":
        baseline = baseline.replace("LF:1", "LF:2")
    elif fault == "bad-metadata":
        metadata["no_mapping"] = "false"
    elif fault == "empty-mapping":
        baseline = ""
    elif fault == "conflict":
        metadata.update(no_mapping=True, diagnostic="no coverage data found")
    elif fault == "generated":
        metadata["generated_sources"] = ["generated.rs"]
        measured += "SF:generated.rs\nDA:1,1\nLF:1\nLH:1\nend_of_record\n"
    elif fault == "large-integer":
        baseline = baseline.replace("DA:4,0", "DA:184467440737095516160,0")
        measured = measured.replace(
            "DA:4,7", "DA:184467440737095516160,7000000000000000000000000"
        )
    elif fault == "missing-line":
        measured = (
            measured.replace("DA:4,7\n", "")
            .replace("LF:2", "LF:1")
            .replace("LH:1", "LH:0")
        )
    (tmp_path / "baseline.lcov").write_text(baseline)
    (tmp_path / "baseline.json").write_text(json.dumps(metadata))
    names = ["baseline.lcov"] + (
        [] if fault == "missing-metadata" else ["baseline.json"]
    )
    artifacts = [
        {
            "name": name,
            "digest": hashlib.sha256((tmp_path / name).read_bytes()).hexdigest(),
            "length": str((tmp_path / name).stat().st_size),
        }
        for name in names
    ]
    events = [
        {
            "id": {"namedSet": {"id": "baseline"}},
            "namedSetOfFiles": {"files": artifacts},
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
    (tmp_path / "events.jsonl").write_text(
        "\n".join(json.dumps(event) for event in events)
    )
    (tmp_path / "inventory.json").write_text(
        json.dumps(
            {
                "coverage_roots": [label],
                "excluded": [],
                "eligible": [
                    {
                        "path": "kept.rs",
                        "language": "rust",
                        "owners": [{"label": label}],
                    }
                ],
            }
        )
    )
    (tmp_path / "report.lcov").write_text(measured)
    arguments = [
        str(ROOT / "tools/coverage/go_report"),
        "--language",
        "rust",
        "--events",
        "events.jsonl",
        "--inventory",
        "inventory.json",
        "--report",
        "report.lcov",
        "--expected-output",
        "receipt.json",
    ]
    result = subprocess.run(
        arguments,
        capture_output=True,
        text=True,
        check=False,
        cwd=tmp_path,
        env={
            key: value
            for key, value in os.environ.items()
            if key != "BUILD_WORKSPACE_DIRECTORY"
        },
    )
    accepted = fault in {
        None,
        "no-mapping",
        "summary-gap",
        "generated",
        "large-integer",
    }
    assert (result.returncode == 0) == accepted, result.stdout + result.stderr
    if not accepted:
        assert (tmp_path / "report.lcov").read_text() == measured
        assert not (tmp_path / "receipt.json").exists()
    else:
        receipt = json.loads((tmp_path / "receipt.json").read_text())
        assert (tmp_path / "report.lcov").read_text() == measured.split(
            "SF:generated.rs"
        )[0]
        if fault == "no-mapping":
            assert receipt["sources"] == {}
            assert receipt["limits"][0]["record_absence"]
        elif fault == "summary-gap":
            assert receipt["limits"][0]["line_summary"]
        elif fault == "large-integer":
            assert receipt["sources"] == {"kept.rs": [184467440737095516160]}
        else:
            assert receipt["sources"] == {"kept.rs": [4]}


def test_command_preserves_abbreviated_and_repeated_options(tmp_path: Path) -> None:
    result = invoke(tmp_path, MEASURED)
    assert result.returncode == 0, result.stderr
    result = subprocess.run(
        [
            str(ROOT / "tools/coverage/go_report"),
            "--events",
            str(tmp_path / "unused.jsonl"),
            "--events",
            str(tmp_path / "events.jsonl"),
            "--inv",
            str(tmp_path / "inventory.json"),
            "--report",
            str(tmp_path / "report.lcov"),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    assert (tmp_path / "report.lcov").read_text() == MEASURED


@pytest.mark.parametrize(
    "form",
    [
        "number",
        "fraction",
        "underscores",
        "null",
        "object",
        "array",
        "boolean",
        "empty-false",
        "empty-negative-zero",
        "empty-fraction",
        "empty-missing",
    ],
)
def test_baseline_length_retains_legacy_integer_conversion(
    tmp_path: Path, form: str
) -> None:
    empty = form.startswith("empty-")
    report = OTHER if empty else MEASURED
    setup = invoke(tmp_path, report, "empty" if empty else None)
    assert setup.returncode == 0, setup.stderr
    events = [
        json.loads(line)
        for line in (tmp_path / "events.jsonl").read_text().splitlines()
    ]
    artifact = events[0]["namedSetOfFiles"]["files"][0]
    length = len(MEASURED.encode())
    values = {
        "number": length,
        "fraction": length + 0.75,
        "underscores": "_".join(str(length)),
        "null": None,
        "object": {},
        "array": [],
        "boolean": True,
        "empty-false": False,
        "empty-negative-zero": "-0",
        "empty-fraction": 0.75,
    }
    if form == "empty-missing":
        artifact.pop("length")
    else:
        artifact["length"] = values[form]
    (tmp_path / "events.jsonl").write_text(
        "\n".join(json.dumps(event) for event in events)
    )
    result = subprocess.run(
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
    assert (result.returncode == 0) == (
        form in {"number", "fraction", "underscores"} or empty
    ), result.stderr
    assert (tmp_path / "report.lcov").read_text() == report
