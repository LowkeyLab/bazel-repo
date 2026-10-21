"""Check real JaCoCo analysis and required-output failures through the report command."""

import json
import os
import shutil
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest

ROOT = Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"]
OTHER = "SF:keep.rs\nDA:9,3\nLF:1\nLH:1\nend_of_record\n"
LABEL = "//tools/coverage:jvm_analyzer.jvm_coverage"
SOURCE = "tools/coverage/JvmReport.java"


def run(*arguments: str, cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        arguments, cwd=cwd, text=True, capture_output=True, check=False
    )


@pytest.fixture
def fixture(tmp_path: Path) -> Path:
    source = tmp_path / SOURCE
    source.parent.mkdir(parents=True)
    shutil.copyfile(ROOT / SOURCE, source)
    classes = ROOT / "tools/coverage/jvm_analyzer.jar"
    result = run(
        str(ROOT / "tools/coverage/jvm_analyzer"),
        str(tmp_path / "baseline.xml"),
        "--classfiles",
        str(classes),
        cwd=tmp_path,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    (tmp_path / "metadata.json").write_text(
        json.dumps(
            {
                "rule": "//tools/coverage:jvm_analyzer",
                "jars": [str(classes)],
                "sources": [SOURCE],
            }
        )
    )
    events = [
        {
            "id": {"namedSet": {"id": "inputs"}},
            "namedSetOfFiles": {
                "files": [
                    {"name": str(tmp_path / name)}
                    for name in ("baseline.xml", "metadata.json")
                ]
            },
        },
        {
            "id": {"targetCompleted": {"label": LABEL}},
            "completed": {
                "success": True,
                "outputGroup": [
                    {"name": "jvm_coverage_baselines", "fileSets": [{"id": "inputs"}]}
                ],
            },
        },
    ]
    (tmp_path / "events.jsonl").write_text(
        "".join(json.dumps(event) + "\n" for event in events)
    )
    (tmp_path / "inventory.json").write_text(
        json.dumps(
            {
                "coverage_roots": [LABEL],
                "eligible": [
                    {
                        "path": SOURCE,
                        "language": "jvm",
                        "owners": [
                            {
                                "label": "//tools/coverage:jvm_analyzer",
                                "kind": "java_binary",
                            }
                        ],
                    }
                ],
                "excluded": [],
            }
        )
    )
    (tmp_path / "report.lcov").write_text(OTHER)
    return tmp_path


def report(root: Path) -> subprocess.CompletedProcess[str]:
    return run(
        str(ROOT / "tools/coverage/jvm_report"),
        "--inventory",
        "inventory.json",
        "--events",
        "events.jsonl",
        "--report",
        "report.lcov",
        "--evidence",
        "evidence",
        "--analyzer",
        str(ROOT / "tools/coverage/jvm_analyzer"),
        cwd=root,
    )


def test_standard_jacoco_baseline_preserves_other_languages(fixture: Path) -> None:
    result = report(fixture)
    assert result.returncode == 0, result.stdout + result.stderr
    text = (fixture / "report.lcov").read_text()
    assert text.startswith(OTHER)
    assert text.count("SF:" + SOURCE + "\n") == 1
    lines = [line for line in text.splitlines() if line.startswith("DA:")][1:]
    assert lines and all(line.endswith(",0") for line in lines)
    receipt = json.loads((fixture / "evidence/receipt.json").read_text())
    assert receipt["roots"] == [LABEL]
    assert receipt["mappings"][0]["classes"] == ["coverage/JvmReport"]
    assert len(receipt["limits"]) == 1
    assert "branch totals" in receipt["limits"][0]["missing_guarantee"]
    assert "BRDA:" not in text
    assert "BRF:" not in text
    assert receipt["branch_totals"][SOURCE]["missed"] > 0
    assert receipt["branch_totals"][SOURCE]["covered"] == 0


@pytest.mark.parametrize(
    "fault",
    [
        "root",
        "metadata",
        "baseline",
        "empty",
        "corrupt",
        "incomplete",
        "classes",
        "owner",
        "data-only",
        "profile",
        "empty-profile",
        "corrupt-profile",
    ],
)
def test_required_output_failure_preserves_existing_report(
    fixture: Path, fault: str
) -> None:
    if fault == "root":
        (fixture / "events.jsonl").write_text("")
    elif fault in {"metadata", "baseline"}:
        (
            fixture / ("metadata.json" if fault == "metadata" else "baseline.xml")
        ).unlink()
    elif fault in {"empty", "corrupt"}:
        (fixture / "baseline.xml").write_text("" if fault == "empty" else "<report>")
    elif fault == "incomplete":
        tree = ET.parse(fixture / "baseline.xml")
        for package in tree.getroot().findall("package"):
            for source in package.findall("sourcefile"):
                package.remove(source)
        tree.write(fixture / "baseline.xml")
    elif fault == "classes":
        data = json.loads((fixture / "metadata.json").read_text())
        data["jars"] = [str(fixture / "missing.jar")]
        (fixture / "metadata.json").write_text(json.dumps(data))
    elif fault in {"owner", "data-only"}:
        data = json.loads((fixture / "inventory.json").read_text())
        data["eligible"].append(
            {
                "path": "missing.java",
                "language": "jvm",
                "owners": [] if fault == "owner" else data["eligible"][0]["owners"],
            }
        )
        (fixture / "inventory.json").write_text(json.dumps(data))
    else:
        data = json.loads((fixture / "inventory.json").read_text())
        data["eligible"][0]["owners"][0]["kind"] = "java_test"
        (fixture / "inventory.json").write_text(json.dumps(data))
        event = {
            "id": {"testResult": {"label": "//tools/coverage:jvm_analyzer"}},
            "testResult": {
                "testActionOutput": [
                    {"name": "test.log", "uri": (fixture / "test.log").as_uri()}
                ]
            },
        }
        with (fixture / "events.jsonl").open("a") as stream:
            stream.write(json.dumps(event) + "\n")
        if fault != "profile":
            directory = fixture / "_coverage"
            directory.mkdir()
            (directory / "required.exec").write_bytes(
                b"" if fault == "empty-profile" else b"corrupt"
            )
    result = report(fixture)
    assert result.returncode != 0, result.stdout + result.stderr
    assert "JVM coverage report failed:" in result.stderr
    assert (fixture / "report.lcov").read_text() == OTHER


def test_inactive_source_requires_declaration_evidence(fixture: Path) -> None:
    data = json.loads((fixture / "inventory.json").read_text())
    data["jvm_source_declarations"] = {LABEL: [SOURCE, "Inactive.java"]}
    data["eligible"].append(
        {
            "path": "Inactive.java",
            "language": "jvm",
            "owners": data["eligible"][0]["owners"],
        }
    )
    (fixture / "inventory.json").write_text(json.dumps(data))
    result = report(fixture)
    assert result.returncode == 0, result.stdout + result.stderr
    assert "SF:Inactive.java" not in (fixture / "report.lcov").read_text()
    receipt = json.loads((fixture / "evidence/receipt.json").read_text())
    limit = next(
        item for item in receipt["limits"] if item["source"] == "Inactive.java"
    )
    assert "configured source selection" in limit["missing_guarantee"]
    assert all(Path(path).is_file() for path in limit["evidence"])
