"""Check real JaCoCo analysis and required-output failures through the report command."""

import json
import os
import shutil
import subprocess
import xml.etree.ElementTree as ET
import zipfile
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


def add_test_result(
    fixture: Path, sources: list[str], jars: list[str] | None = None
) -> None:
    label = "//tools/coverage:jvm_analyzer"
    data = json.loads((fixture / "inventory.json").read_text())
    data["jvm_tests"] = [label]
    (fixture / "inventory.json").write_text(json.dumps(data))
    path = fixture / "test-inputs.json"
    path.write_text(
        json.dumps(
            {
                "rule": label,
                "sources": sources,
                "jars": jars
                if jars is not None
                else [str(ROOT / "tools/coverage/jvm_analyzer.jar")],
            }
        )
    )
    events = [
        {
            "id": {"namedSet": {"id": "test-inputs"}},
            "namedSetOfFiles": {"files": [{"name": str(path)}]},
        },
        {
            "id": {"targetCompleted": {"label": label}},
            "completed": {
                "success": True,
                "outputGroup": [
                    {"name": "jvm_test_inputs", "fileSets": [{"id": "test-inputs"}]}
                ],
            },
        },
        {
            "id": {"testResult": {"label": label}},
            "testResult": {
                "testActionOutput": [
                    {"name": "test.log", "uri": (fixture / "test.log").as_uri()}
                ]
            },
        },
    ]
    with (fixture / "events.jsonl").open("a") as stream:
        stream.write("".join(json.dumps(event) + "\n" for event in events))


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


def test_repeated_report_accepts_read_only_inputs(fixture: Path) -> None:
    (fixture / "baseline.xml").chmod(0o444)
    first = report(fixture)
    assert first.returncode == 0, first.stdout + first.stderr
    original = (fixture / "report.lcov").read_bytes()
    for name in ("baseline-0.xml", "class-0-0.jar"):
        assert (fixture / "evidence" / name).stat().st_mode & 0o200
    second = report(fixture)
    assert second.returncode == 0, second.stdout + second.stderr
    assert (fixture / "report.lcov").read_bytes() == original
    assert (fixture / "evidence/receipt.json").is_file()


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
        add_test_result(fixture, [SOURCE])
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


def test_configured_non_jvm_test_preserves_jvm_baselines(fixture: Path) -> None:
    add_test_result(fixture, [])
    result = report(fixture)
    assert result.returncode == 0, result.stdout + result.stderr
    text = (fixture / "report.lcov").read_text()
    assert text.startswith(OTHER)
    assert "SF:" + SOURCE in text
    assert all(
        line.endswith(",0")
        for line in text.splitlines()
        if line.startswith("DA:") and line != "DA:9,3"
    )
    receipt = json.loads((fixture / "evidence/receipt.json").read_text())
    assert receipt["test_inputs"]["//tools/coverage:jvm_analyzer"]["sources"] == []


@pytest.mark.parametrize(
    "fault", ["event", "missing", "empty", "corrupt", "incomplete", "identity"]
)
def test_required_configured_inputs_failure_preserves_report(
    fixture: Path, fault: str
) -> None:
    add_test_result(fixture, [SOURCE])
    path = fixture / "test-inputs.json"
    if fault == "event":
        events = [
            json.loads(line)
            for line in (fixture / "events.jsonl").read_text().splitlines()
        ]
        events = [
            event
            for event in events
            if event.get("id", {}).get("targetCompleted", {}).get("label")
            != "//tools/coverage:jvm_analyzer"
        ]
        (fixture / "events.jsonl").write_text(
            "".join(json.dumps(event) + "\n" for event in events)
        )
    elif fault == "missing":
        path.unlink()
    elif fault in {"empty", "corrupt"}:
        path.write_text("" if fault == "empty" else "{")
    elif fault == "incomplete":
        path.write_text(json.dumps({"rule": "//tools/coverage:jvm_analyzer"}))
    else:
        path.write_text(json.dumps({"rule": "//wrong:identity", "sources": [SOURCE]}))
    result = report(fixture)
    assert result.returncode != 0
    assert "JVM coverage report failed:" in result.stderr
    assert (fixture / "report.lcov").read_text() == OTHER


def test_baseline_ignores_independent_aspect_events(fixture: Path) -> None:
    path = fixture / "events.jsonl"
    event = {
        "id": {
            "targetCompleted": {
                "label": LABEL,
                "aspect": "//tools/coverage:jvm.bzl%jvm_test_inputs",
            }
        },
        "completed": {"success": True},
    }
    path.write_text(json.dumps(event) + "\n" + path.read_text())
    result = report(fixture)
    assert result.returncode == 0, result.stdout + result.stderr
    assert "SF:" + SOURCE in (fixture / "report.lcov").read_text()


def test_unrelated_valid_profile_preserves_report(fixture: Path) -> None:
    add_test_result(fixture, [SOURCE])
    profile = fixture / "_coverage/required.exec"
    profile.parent.mkdir()
    result = run(
        str(ROOT / "tools/coverage/fixtures/jvm_profile/profile"),
        str(profile),
        cwd=fixture,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    result = report(fixture)
    assert result.returncode != 0
    assert "No required JVM profile matches a configured class" in result.stderr
    assert (fixture / "report.lcov").read_text() == OTHER


def test_inactive_owner_retains_limit_for_an_active_source(fixture: Path) -> None:
    inactive = "//tools/coverage:inactive.jvm_coverage"
    (fixture / "inactive.xml").write_text('<report name="empty"/>')
    (fixture / "inactive.json").write_text(
        json.dumps({"rule": "//tools/coverage:inactive", "sources": [], "jars": []})
    )
    events = [
        {
            "id": {"namedSet": {"id": "inactive"}},
            "namedSetOfFiles": {
                "files": [
                    {"name": str(fixture / name)}
                    for name in ("inactive.xml", "inactive.json")
                ]
            },
        },
        {
            "id": {"targetCompleted": {"label": inactive}},
            "completed": {
                "success": True,
                "outputGroup": [
                    {"name": "jvm_coverage_baselines", "fileSets": [{"id": "inactive"}]}
                ],
            },
        },
    ]
    with (fixture / "events.jsonl").open("a") as stream:
        stream.write("".join(json.dumps(event) + "\n" for event in events))
    data = json.loads((fixture / "inventory.json").read_text())
    data["coverage_roots"].append(inactive)
    data["jvm_source_declarations"] = {LABEL: [SOURCE], inactive: [SOURCE]}
    (fixture / "inventory.json").write_text(json.dumps(data))
    result = report(fixture)
    assert result.returncode == 0, result.stdout + result.stderr
    receipt = json.loads((fixture / "evidence/receipt.json").read_text())
    assert "SF:" + SOURCE in (fixture / "report.lcov").read_text()
    assert any(
        item["source"] == SOURCE
        and item.get("rule") == inactive
        and "configured source selection" in item["missing_guarantee"]
        for item in receipt["limits"]
    )


@pytest.mark.parametrize("kind", ["session", "header", "session-missing-jar"])
def test_profile_without_class_data(fixture: Path, kind: str) -> None:
    add_test_result(
        fixture,
        [SOURCE],
        [str(fixture / "missing.jar")] if kind == "session-missing-jar" else None,
    )
    profile = fixture / "_coverage/required.exec"
    profile.parent.mkdir()
    result = run(
        str(ROOT / "tools/coverage/fixtures/jvm_profile/profile"),
        str(profile),
        "session" if kind == "session-missing-jar" else kind,
        cwd=fixture,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    result = report(fixture)
    if kind == "header":
        assert result.returncode != 0
        assert "Incomplete required JVM profile" in result.stderr
        assert (fixture / "report.lcov").read_text() == OTHER
    else:
        assert result.returncode == 0, result.stdout + result.stderr
        receipt = json.loads((fixture / "evidence/receipt.json").read_text())
        assert list(receipt["profile_info"].values()) == [{"sessions": 1, "classes": 0}]
        assert receipt["matched_classes"] == []
        assert all(
            line.endswith(",0")
            for line in (fixture / "report.lcov").read_text().splitlines()
            if line.startswith("DA:") and line != "DA:9,3"
        )


@pytest.mark.parametrize(
    "fault",
    [
        "none",
        "unmapped",
        "foreign-owner",
        "foreign-version",
        "additional-profile",
        "shared-source",
    ],
)
def test_profile_matches_its_configured_sources(fixture: Path, fault: str) -> None:
    owner = "//tools/coverage/fixtures/jvm_profile:profile.jvm_coverage"
    sources = [
        "tools/coverage/fixtures/jvm_profile/" + name
        for name in ("Profile.java", "Marker.java")
    ]
    for name in sources:
        path = fixture / name
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / name, path)
    jar = (
        ROOT
        / "tools/coverage/fixtures/jvm_profile"
        / ("profile_variant.jar" if fault == "unmapped" else "profile.jar")
    )
    result = run(
        str(ROOT / "tools/coverage/jvm_analyzer"),
        str(fixture / "profile-baseline.xml"),
        "--classfiles",
        str(jar),
        cwd=fixture,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    (fixture / "profile-metadata.json").write_text(
        json.dumps(
            {
                "rule": "//tools/coverage/fixtures/jvm_profile:profile",
                "sources": sources,
                "jars": [str(jar)],
            }
        )
    )
    events = [
        {
            "id": {"namedSet": {"id": "profile-root"}},
            "namedSetOfFiles": {
                "files": [
                    {"name": str(fixture / name)}
                    for name in ("profile-baseline.xml", "profile-metadata.json")
                ]
            },
        },
        {
            "id": {"targetCompleted": {"label": owner}},
            "completed": {
                "success": True,
                "outputGroup": [
                    {
                        "name": "jvm_coverage_baselines",
                        "fileSets": [{"id": "profile-root"}],
                    }
                ],
            },
        },
    ]
    with (fixture / "events.jsonl").open("a") as stream:
        stream.write("".join(json.dumps(event) + "\n" for event in events))
    data = json.loads((fixture / "inventory.json").read_text())
    data["coverage_roots"].append(owner)
    data["eligible"].extend(
        {
            "path": source,
            "language": "jvm",
            "owners": [
                {
                    "label": "//tools/coverage/fixtures/jvm_profile:profile",
                    "kind": "java_binary",
                }
            ],
        }
        for source in sources
    )
    (fixture / "inventory.json").write_text(json.dumps(data))
    add_test_result(
        fixture,
        [SOURCE] if fault == "foreign-owner" else sources,
        [str(ROOT / "tools/coverage/fixtures/jvm_profile/profile_variant.jar")]
        if fault == "foreign-version"
        else [str(ROOT / "tools/coverage/jvm_analyzer.jar")]
        if fault == "foreign-owner"
        else [str(jar)],
    )
    profile = fixture / "_coverage/related.exec"
    profile.parent.mkdir()
    result = run(
        str(
            ROOT
            / "tools/coverage/fixtures/jvm_profile"
            / ("profile_variant" if fault == "unmapped" else "profile")
        ),
        str(profile),
        "marker",
        cwd=fixture,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    if fault == "additional-profile":
        result = run(
            str(
                ROOT
                / "tools/coverage/fixtures/jvm_profile"
                / ("profile_variant" if fault == "unmapped" else "profile")
            ),
            str(profile.parent / "unrelated.exec"),
            cwd=fixture,
        )
        assert result.returncode == 0, result.stdout + result.stderr
    copied: list[str] = []
    if fault == "shared-source":
        copied = ["copied/" + Path(name).name for name in sources]
        for name, original in zip(copied, sources, strict=True):
            destination = fixture / name
            destination.parent.mkdir(exist_ok=True)
            shutil.copyfile(fixture / original, destination)
        copied_owner = "//copied:profile.jvm_coverage"
        data = json.loads((fixture / "inventory.json").read_text())
        data["coverage_roots"].append(copied_owner)
        data["eligible"].extend(
            {
                "path": name,
                "language": "jvm",
                "owners": [{"label": "//copied:profile", "kind": "java_binary"}],
            }
            for name in copied
        )
        (fixture / "inventory.json").write_text(json.dumps(data))
        (fixture / "copied-inputs.json").write_text(
            json.dumps({"sources": copied, "jars": [str(jar)]})
        )
        events = [
            {
                "id": {"namedSet": {"id": "copied"}},
                "namedSetOfFiles": {
                    "files": [
                        {"name": str(fixture / name)}
                        for name in ("copied-inputs.json", "profile-baseline.xml")
                    ]
                },
            },
            {
                "id": {"targetCompleted": {"label": copied_owner}},
                "completed": {
                    "success": True,
                    "outputGroup": [
                        {
                            "name": "jvm_coverage_baselines",
                            "fileSets": [{"id": "copied"}],
                        }
                    ],
                },
            },
        ]
        with (fixture / "events.jsonl").open("a") as stream:
            stream.write("".join(json.dumps(event) + "\n" for event in events))
    result = report(fixture)
    if fault in {"none", "unmapped", "shared-source"}:
        assert result.returncode == 0, result.stdout + result.stderr
        receipt = json.loads((fixture / "evidence/receipt.json").read_text())
        assert any(
            item["class"] == "coverage/fixture/Marker"
            for item in receipt["matched_classes"]
        )
        if fault == "shared-source":
            text = (fixture / "report.lcov").read_text()
            assert all(
                "SF:" + name + "\n" not in text for name in [sources[1], copied[1]]
            )
            assert any(
                "shared by distinct original sources" in item["missing_guarantee"]
                for item in receipt["limits"]
            )
        if fault == "unmapped":
            assert "SF:" + sources[1] not in (fixture / "report.lcov").read_text()
            assert any(
                item["source"] == sources[1]
                and "no unique source record" in item["missing_guarantee"]
                for item in receipt["limits"]
            )
    else:
        assert result.returncode != 0
        assert "No required JVM profile matches a configured class" in result.stderr
        assert (fixture / "report.lcov").read_text() == OTHER


@pytest.mark.parametrize(
    "artifact", ["nested-xml", "original-class-members", "cr-only-source"]
)
def test_standard_analyzer_preserves_xml_and_archive_selection(
    fixture: Path, artifact: str
) -> None:
    if artifact == "cr-only-source":
        source = fixture / SOURCE
        source.write_bytes(source.read_bytes().replace(b"\n", b"\r"))
    elif artifact == "nested-xml":
        tree = ET.parse(fixture / "baseline.xml")
        group = ET.SubElement(tree.getroot(), "group")
        package = ET.SubElement(group, "package", name="ignored")
        ET.SubElement(package, "class", name="ignored/Nested")
        ET.SubElement(package, "sourcefile", name="Nested.java")
        tree.write(fixture / "baseline.xml")
    else:
        original = ROOT / "tools/coverage/jvm_analyzer.jar"
        archive = fixture / "originals.jar"
        with (
            zipfile.ZipFile(original) as source,
            zipfile.ZipFile(archive, "w") as target,
        ):
            for entry in source.infolist():
                name = entry.filename
                content = source.read(entry)
                target.writestr(entry, content)
                if (
                    name.endswith(".class")
                    and name + ".uninstrumented" not in source.namelist()
                ):
                    target.writestr(name + ".uninstrumented", content)
        data = json.loads((fixture / "metadata.json").read_text())
        data["jars"] = [str(archive)]
        (fixture / "metadata.json").write_text(json.dumps(data))
    result = report(fixture)
    assert result.returncode == 0, result.stdout + result.stderr
    receipt = json.loads((fixture / "evidence/receipt.json").read_text())
    assert receipt["mappings"][0]["classes"] == ["coverage/JvmReport"]
    assert "SF:" + SOURCE in (fixture / "report.lcov").read_text()


def test_inactive_owners_preserve_declaration_order(fixture: Path) -> None:
    labels = ["//z:inactive.jvm_coverage", "//a:inactive.jvm_coverage"]
    (fixture / "inactive.xml").write_text('<report name="empty"/>')
    (fixture / "inactive.json").write_text(json.dumps({"sources": [], "jars": []}))
    data = json.loads((fixture / "inventory.json").read_text())
    data["coverage_roots"].extend(labels)
    data["jvm_source_declarations"] = {label: [SOURCE] for label in labels}
    (fixture / "inventory.json").write_text(json.dumps(data))
    events = [
        {
            "id": {"namedSet": {"id": "inactive"}},
            "namedSetOfFiles": {
                "files": [
                    {"name": str(fixture / name)}
                    for name in ("inactive.json", "inactive.xml")
                ]
            },
        }
    ]
    events.extend(
        {
            "id": {"targetCompleted": {"label": label}},
            "completed": {
                "success": True,
                "outputGroup": [
                    {"name": "jvm_coverage_baselines", "fileSets": [{"id": "inactive"}]}
                ],
            },
        }
        for label in labels
    )
    with (fixture / "events.jsonl").open("a") as stream:
        stream.write("".join(json.dumps(event) + "\n" for event in events))
    result = report(fixture)
    assert result.returncode == 0, result.stdout + result.stderr
    receipt = json.loads((fixture / "evidence/receipt.json").read_text())
    assert [
        limit["rule"]
        for limit in receipt["limits"]
        if "configured source selection" in limit["missing_guarantee"]
    ] == labels
