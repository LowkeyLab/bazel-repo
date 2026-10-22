"""Check report validation through the command with the maintained native collector."""

import hashlib
import json
import os
import subprocess
import zipfile
from pathlib import Path

import pytest

RUNFILES = Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"]
REPORTER = RUNFILES / "tools/coverage/javascript_report"
COLLECTOR = RUNFILES / os.environ["JAVASCRIPT_COLLECTOR"]
MERGER = RUNFILES / os.environ["JAVASCRIPT_LCOV_MERGER"]
LCOV = "SF:module.js\nDA:1,4\nLF:1\nLH:1\nend_of_record\n"


@pytest.fixture
def workspace(tmp_path: Path) -> Path:
    (tmp_path / "module.js").write_text(
        "export function choose(flag) {\n  return flag ? 7 : 3;\n}\n"
    )
    (tmp_path / "inventory.json").write_text(
        json.dumps(
            {
                "eligible": [
                    {
                        "path": "module.js",
                        "language": "javascript",
                        "owners": [
                            {
                                "label": "//:library",
                                "kind": "js_library",
                                "tags": [],
                                "testonly": False,
                            }
                        ],
                    }
                ],
                "excluded": [],
                "javascript_tests": ["//:test"],
            }
        )
    )
    (tmp_path / "report.lcov").write_text(LCOV)
    test = tmp_path / "test"
    test.mkdir()
    (test / "coverage.dat").write_text(LCOV)
    profiles = test / "_coverage"
    profiles.mkdir()
    (profiles / "coverage-1.json").write_text(
        json.dumps({"result": [{"url": "file:///module.js", "functions": []}]})
    )
    (tmp_path / "fixture.javascript-test-inputs.json").write_text(
        json.dumps({"rule": "//:test", "instrumented": True, "sources": ["module.js"]})
    )
    events = [
        {
            "id": {"namedSet": {"id": "inputs"}},
            "namedSetOfFiles": {
                "files": [
                    {"uri": (tmp_path / "fixture.javascript-test-inputs.json").as_uri()}
                ]
            },
        },
        {
            "id": {"targetCompleted": {"label": "//:test"}},
            "completed": {
                "outputGroup": [
                    {"name": "javascript_test_inputs", "fileSets": [{"id": "inputs"}]}
                ]
            },
        },
        {
            "id": {"testResult": {"label": "//:test"}},
            "testResult": {
                "testActionOutput": [
                    {"name": "test.lcov", "uri": (test / "coverage.dat").as_uri()}
                ],
                "executionInfo": {"strategy": "protocol-fixture"},
            },
        },
    ]
    (tmp_path / "events.json").write_text(
        "".join(json.dumps(event) + "\n" for event in events)
    )
    return tmp_path


def run(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            str(REPORTER),
            "--inventory",
            str(root / "inventory.json"),
            "--report",
            str(root / "report.lcov"),
            "--events",
            str(root / "events.json"),
            "--evidence",
            str(root / "evidence"),
            "--collector",
            str(COLLECTOR),
            "--merger",
            str(MERGER),
        ],
        cwd=root,
        env={**os.environ, "BUILD_WORKSPACE_DIRECTORY": str(root)},
        capture_output=True,
        text=True,
        check=False,
    )


def test_merging_retains_known_hits_and_native_baseline(workspace: Path) -> None:
    result = run(workspace)
    assert result.returncode == 0, result.stdout + result.stderr
    report = (workspace / "report.lcov").read_text()
    assert report.count("SF:module.js\n") == 1
    assert "DA:1,4\n" in report
    assert "DA:2,0\n" in report
    receipt = json.loads((workspace / "evidence/receipt.json").read_text())
    assert receipt["native_sources"] == ["module.js"]
    assert receipt["executions"][0]["execution"]["strategy"] == "protocol-fixture"


@pytest.mark.parametrize(
    "fault",
    [
        "missing-report",
        "empty-report",
        "corrupt-line",
        "partial-report",
        "missing-profile",
        "corrupt-profile",
        "empty-profile",
        "missing-inputs",
        "outside-filter",
        "missing-baseline-source",
        "incomplete-summary",
        "corrupt-branch",
        "incomplete-branches",
        "corrupt-function",
        "corrupt-range",
        "corrupt-function-count",
        "corrupt-map",
    ],
)
def test_required_output_failures_preserve_report(workspace: Path, fault: str) -> None:
    report = workspace / "test/coverage.dat"
    profile = workspace / "test/_coverage/coverage-1.json"
    if fault == "missing-report":
        report.unlink()
    elif fault == "empty-report":
        report.write_text("")
    elif fault == "corrupt-line":
        report.write_text(LCOV.replace("DA:1,4", "DA:1,broken"))
    elif fault == "partial-report":
        report.write_text(LCOV.replace("module.js", "other.js"))
    elif fault == "missing-profile":
        profile.unlink()
    elif fault == "corrupt-profile":
        profile.write_text("{")
    elif fault == "empty-profile":
        profile.write_text('{"result": []}')
    elif fault == "missing-inputs":
        (workspace / "events.json").write_text(
            (workspace / "events.json").read_text().splitlines()[-1] + "\n"
        )
    elif fault == "outside-filter":
        (workspace / "fixture.javascript-test-inputs.json").write_text(
            '{"rule": "//:test", "instrumented": false, "sources": ["module.js"]}'
        )
    elif fault == "missing-baseline-source":
        (workspace / "module.js").unlink()
    elif fault == "incomplete-summary":
        report.write_text(LCOV.replace("LF:1", "LF:2"))
    elif fault == "corrupt-branch":
        report.write_text(
            LCOV.replace("end_of_record", "BRDA:1,0,0,broken\nend_of_record")
        )
    elif fault == "incomplete-branches":
        report.write_text(
            LCOV.replace("end_of_record", "BRDA:1,0,0,0\nBRF:2\nBRH:0\nend_of_record")
        )
    elif fault == "corrupt-function":
        profile.write_text(
            json.dumps(
                {
                    "result": [
                        {
                            "url": "file:///module.js",
                            "functions": [{"functionName": "choose"}],
                        }
                    ]
                }
            )
        )
    elif fault == "corrupt-range":
        profile.write_text(
            json.dumps(
                {
                    "result": [
                        {
                            "url": "file:///module.js",
                            "functions": [
                                {
                                    "functionName": "choose",
                                    "ranges": [
                                        {"startOffset": 0, "endOffset": 1, "count": -1}
                                    ],
                                }
                            ],
                        }
                    ]
                }
            )
        )
    elif fault == "corrupt-function-count":
        report.write_text(
            LCOV.replace("end_of_record", "FNDA:broken,choice\nend_of_record")
        )
    elif fault == "corrupt-map":
        (workspace / "module.js.map").write_text(
            '{"version":3,"sources":{},"mappings":""}'
        )
        path = workspace / "fixture.javascript-test-inputs.json"
        configured = json.loads(path.read_text())
        configured["artifacts"] = [{"path": "module.js.map", "source": "module.js.map"}]
        path.write_text(json.dumps(configured))
    result = run(workspace)
    assert result.returncode != 0, fault
    expected = {
        "missing-report": "FileNotFoundError",
        "empty-report": "Missing or incomplete JavaScript LCOV",
        "corrupt-line": "Invalid JavaScript line record",
        "partial-report": "Missing required native JavaScript source",
        "missing-profile": "Missing required native Node profile",
        "corrupt-profile": "JSONDecodeError",
        "empty-profile": "Invalid native Node profile",
        "missing-inputs": "Missing required configured JavaScript instrumentation",
        "outside-filter": "Missing required configured JavaScript instrumentation",
        "missing-baseline-source": "Missing or incomplete JavaScript LCOV",
        "incomplete-summary": "Incomplete JavaScript executable records",
        "corrupt-branch": "Invalid JavaScript branch record",
        "incomplete-branches": "Incomplete JavaScript branch coverage",
        "corrupt-function": "Invalid native Node function profile",
        "corrupt-range": "Invalid native Node range profile",
        "corrupt-function-count": "Invalid JavaScript function record",
        "corrupt-map": "Invalid required JavaScript source map",
    }
    assert expected[fault] in result.stderr, result.stdout + result.stderr
    assert (workspace / "report.lcov").read_text() == LCOV


@pytest.mark.parametrize("aggregate_only", [False, True])
def test_native_original_manifest_limits_do_not_publish_false_zeros(
    workspace: Path,
    aggregate_only: bool,
) -> None:
    source = workspace / "library.ts"
    source.write_text("export const value: number = 7;\n")
    inventory = json.loads((workspace / "inventory.json").read_text())
    inventory["eligible"].append(
        {
            "path": "library.ts",
            "language": "javascript",
            "owners": [
                {
                    "label": "//:library",
                    "kind": "ts_project",
                    "tags": [],
                    "testonly": False,
                }
            ],
        }
    )
    (workspace / "inventory.json").write_text(json.dumps(inventory))
    configured = json.loads(
        (workspace / "fixture.javascript-test-inputs.json").read_text()
    )
    if not aggregate_only:
        configured["sources"].append("library.ts")
    (workspace / "fixture.javascript-test-inputs.json").write_text(
        json.dumps(configured)
    )
    unsupported = "SF:library.ts\nFN:1,(empty-report)\nFNF:1\nFNH:0\nFNDA:0,(empty-report)\nDA:1,0\nLF:1\nLH:0\nend_of_record\n"
    other = "SF:keep.rs\nDA:9,3\nLF:1\nLH:1\nend_of_record\n"
    if aggregate_only:
        unsupported = "SF:library.ts\nFNF:0\nFNH:0\nLF:0\nLH:0\nend_of_record\n"
    else:
        (workspace / "test/coverage.dat").write_text(LCOV + unsupported)
    (workspace / "report.lcov").write_text(LCOV + unsupported + other)
    result = run(workspace)
    assert result.returncode == 0, result.stdout + result.stderr
    report = (workspace / "report.lcov").read_text()
    assert "SF:library.ts\n" not in report
    assert "SF:keep.rs\n" in report
    assert "DA:9,3\n" in report
    receipt = json.loads((workspace / "evidence/receipt.json").read_text())
    assert any(
        limit["source"] == "library.ts"
        and "original-manifest" in limit["missing_guarantee"]
        for limit in receipt["limits"]
    )


def test_missing_combined_source_causes_failure(workspace: Path) -> None:
    other = "SF:keep.rs\nDA:9,3\nLF:1\nLH:1\nend_of_record\n"
    (workspace / "report.lcov").write_text(other)
    result = run(workspace)
    assert result.returncode != 0
    assert "Missing required combined JavaScript source" in result.stderr
    assert (workspace / "report.lcov").read_text() == other


def test_package_directory_is_not_a_script(workspace: Path) -> None:
    (workspace / "package.js").mkdir()
    path = workspace / "fixture.javascript-test-inputs.json"
    configured = json.loads(path.read_text())
    configured["artifacts"] = [{"path": "package.js", "source": "package.js"}]
    path.write_text(json.dumps(configured))
    result = run(workspace)
    assert result.returncode == 0, result.stdout + result.stderr


def test_native_branch_hits_survive_fallback_baselines(workspace: Path) -> None:
    (workspace / "library.ts").write_text("export const value: number = 7;\n")
    inventory = json.loads((workspace / "inventory.json").read_text())
    inventory["eligible"].append(
        {
            "path": "library.ts",
            "language": "javascript",
            "owners": [
                {
                    "label": "//:library",
                    "kind": "ts_project",
                    "tags": [],
                    "testonly": False,
                }
            ],
        }
    )
    (workspace / "inventory.json").write_text(json.dumps(inventory))
    configured = json.loads(
        (workspace / "fixture.javascript-test-inputs.json").read_text()
    )
    configured["sources"].append("library.ts")
    (workspace / "fixture.javascript-test-inputs.json").write_text(
        json.dumps(configured)
    )
    branch_hit = "SF:library.ts\nFN:1,(empty-report)\nFNF:1\nFNH:0\nFNDA:0,(empty-report)\nDA:1,0\nLF:1\nLH:0\nBRDA:1,0,0,1\nBRF:1\nBRH:1\nend_of_record\n"
    (workspace / "test/coverage.dat").write_text(LCOV + branch_hit)
    (workspace / "report.lcov").write_text(LCOV + branch_hit)
    result = run(workspace)
    assert result.returncode == 0, result.stdout + result.stderr
    assert "SF:library.ts\n" in (workspace / "report.lcov").read_text()
    assert "BRDA:1,0,0,1\n" in (workspace / "report.lcov").read_text()


def test_selective_child_profile_loss_records_exact_limit(workspace: Path) -> None:
    child = workspace / "test/_coverage/coverage-child.json"
    child.write_text(
        json.dumps({"result": [{"url": "file:///module.js", "functions": []}]})
    )
    child.unlink()
    result = run(workspace)
    assert result.returncode == 0, result.stdout + result.stderr
    receipt = json.loads((workspace / "evidence/receipt.json").read_text())
    limits = [
        item
        for item in receipt["limits"]
        if "child profile" in item["missing_guarantee"]
    ]
    assert any(
        item["source"] == "module.js" and item["rule"] == "//:test" for item in limits
    )
    assert len(receipt["executions"][0]["retained_profiles"]) == 1


def test_required_browser_output_cannot_disappear(workspace: Path) -> None:
    metadata = workspace / "fixture.javascript-test-inputs.json"
    data = json.loads(metadata.read_text())
    data["browser"] = True
    metadata.write_text(json.dumps(data))
    original = (workspace / "report.lcov").read_bytes()
    result = run(workspace)
    assert result.returncode != 0, result.stdout + result.stderr
    assert "Missing required browser coverage" in result.stderr
    assert (workspace / "report.lcov").read_bytes() == original


@pytest.mark.parametrize(
    "fault",
    [
        None,
        "missing-profile",
        "corrupt-profile",
        "missing-map",
        "corrupt-map",
        "missing-report",
        "incomplete-report",
        "invalid-range",
        "invalid-function",
        "invalid-receipt",
        "incomplete-scripts",
        "invalid-map-fields",
        "missing-receipts",
        "missing-page",
        "duplicate-page",
        "invalid-page",
        "unsafe-path",
        "unmapped-source",
        "ambiguous-source",
        "duplicate-source",
        "empty-page",
        "archived-page",
    ],
)
def test_browser_output_validation_and_hit_preservation(
    workspace: Path, fault: str | None
) -> None:
    metadata = workspace / "fixture.javascript-test-inputs.json"
    data = json.loads(metadata.read_text())
    data["browser"] = True
    metadata.write_text(json.dumps(data))
    inventory_path = workspace / "inventory.json"
    inventory = json.loads(inventory_path.read_text())
    inventory["eligible"].append(
        {**inventory["eligible"][0], "path": "page.astro", "language": "browser"}
    )
    inventory_path.write_text(json.dumps(inventory))
    source = (workspace / "module.js").read_text()
    raw = [
        {
            "url": "http://127.0.0.1:1234/site.js",
            "source": source,
            "functions": [
                {
                    "functionName": "choose",
                    "isBlockCoverage": True,
                    "ranges": [
                        {"startOffset": 0, "endOffset": len(source), "count": 7}
                    ],
                }
            ],
        }
    ]
    mapping = {
        "version": 3,
        "sources": ["/build/module.js"],
        "sourcesContent": [source],
        "mappings": "AAAA",
    }
    receipt = {
        "schema_version": 1,
        "scripts": [
            {
                "script": "/build/site.js",
                "sources": mapping["sources"],
                "sourcesContent": [source],
            }
        ],
        "sources": ["/build/module.js"],
    }
    files = {
        "receipt.json": json.dumps(receipt),
        "profile.json": json.dumps(raw),
        "0.js": source,
        "0.map": json.dumps(mapping),
        "report.lcov": "SF:/build/module.js\nDA:1,7\nLF:1\nLH:1\nend_of_record\n",
    }
    receipt["profile_sha256"] = hashlib.sha256(
        files["profile.json"].encode()
    ).hexdigest()
    receipt["report_sha256"] = hashlib.sha256(files["report.lcov"].encode()).hexdigest()
    receipt["scripts"][0]["map_sha256"] = hashlib.sha256(
        files["0.map"].encode()
    ).hexdigest()
    files["receipt.json"] = json.dumps(receipt)
    if fault in {"missing-profile", "missing-map", "missing-report"}:
        del files[
            {
                "missing-profile": "profile.json",
                "missing-map": "0.map",
                "missing-report": "report.lcov",
            }[fault]
        ]
    elif fault == "corrupt-profile":
        files["profile.json"] = "!"
    elif fault == "corrupt-map":
        files["0.map"] = "!"
    elif fault == "incomplete-report":
        files["report.lcov"] = "SF:/build/module.js\nLF:1\nLH:1\nend_of_record\n"
    elif fault in {"invalid-range", "invalid-function"}:
        if fault == "invalid-range":
            raw[0]["functions"][0]["ranges"][0]["count"] = -1
        else:
            raw[0]["functions"][0]["ranges"] = []
        files["profile.json"] = json.dumps(raw)
    elif fault == "invalid-receipt":
        receipt["schema_version"] = 2
    elif fault == "incomplete-scripts":
        receipt["scripts"] = []
    elif fault == "invalid-map-fields":
        mapping["version"] = 2
        files["0.map"] = json.dumps(mapping)
    elif fault == "unmapped-source":
        mapping["sourcesContent"] = ["different original source"]
        receipt["scripts"][0]["sourcesContent"] = mapping["sourcesContent"]
        files["0.map"] = json.dumps(mapping)
    elif fault == "ambiguous-source":
        path = workspace / "inventory.json"
        inventory = json.loads(path.read_text())
        extra = {**inventory["eligible"][0], "path": "build/module.js"}
        inventory["eligible"].append(extra)
        path.write_text(json.dumps(inventory))
        (workspace / "build").mkdir()
        (workspace / "build/module.js").write_text(source)
    elif fault == "duplicate-source":
        files["report.lcov"] += files["report.lcov"].replace(
            "/build/module.js", "/other/module.js"
        )
        receipt["sources"].append("/other/module.js")
    elif fault == "empty-page":
        files["profile.json"] = "[]"
        files["report.lcov"] = ""
        receipt["scripts"] = []
        receipt["sources"] = []
    # Structural faults use valid hashes, so the command must reach their validation.
    if fault not in {"corrupt-profile", "corrupt-map"}:
        for filename, key in (
            ("profile.json", "profile_sha256"),
            ("report.lcov", "report_sha256"),
        ):
            if filename in files:
                receipt[key] = hashlib.sha256(files[filename].encode()).hexdigest()
        if receipt["scripts"] and "0.map" in files:
            receipt["scripts"][0]["map_sha256"] = hashlib.sha256(
                files["0.map"].encode()
            ).hexdigest()
        files["receipt.json"] = json.dumps(receipt)
    if fault == "missing-receipts":
        del files["receipt.json"]
    pages = {
        "missing-page": ["page", "lost"],
        "duplicate-page": ["page", "page"],
        "invalid-page": ["../page"],
    }.get(fault, ["page"])
    directory = workspace / "browser-outputs/browser-coverage"
    directory.mkdir(parents=True)
    (directory / "pages.json").write_text(json.dumps(pages))
    for name, content in files.items():
        path = directory / "page" / name
        path.parent.mkdir(exist_ok=True)
        path.write_text(content)
    events_path = workspace / "events.json"
    events = [json.loads(line) for line in events_path.read_text().splitlines()]
    if fault == "archived-page":
        archive_path = workspace / "browser.zip"
        with zipfile.ZipFile(archive_path, "w") as archive:
            for path in directory.rglob("*"):
                if path.is_file():
                    archive.write(path, path.relative_to(directory.parent).as_posix())
        events[-1]["testResult"]["testActionOutput"].append(
            {"name": "test.outputs__outputs.zip", "uri": archive_path.as_uri()}
        )
    else:
        events[-1]["testResult"]["testActionOutput"].extend(
            {
                "name": "test.outputs/" + path.relative_to(directory.parent).as_posix(),
                "uri": path.as_uri(),
            }
            for path in directory.rglob("*")
            if path.is_file()
        )
    if fault == "unsafe-path":
        events[-1]["testResult"]["testActionOutput"].append(
            {
                "name": "browser-coverage/../escape",
                "uri": (directory / "pages.json").as_uri(),
            }
        )
    events_path.write_text("".join(json.dumps(event) + "\n" for event in events))
    original = (workspace / "report.lcov").read_bytes()
    result = run(workspace)
    if fault not in {None, "unmapped-source", "empty-page", "archived-page"}:
        assert result.returncode != 0, result.stdout + result.stderr
        assert (workspace / "report.lcov").read_bytes() == original
        expected = {
            "invalid-range": "Invalid browser coverage range",
            "invalid-function": "Invalid browser function ranges",
            "invalid-receipt": "Invalid browser coverage receipt",
            "incomplete-scripts": "Incomplete browser coverage inputs",
            "invalid-map-fields": "Invalid required browser source map",
            "missing-receipts": "Missing required browser coverage receipts",
            "missing-page": "Incomplete required browser page outputs",
            "duplicate-page": "Incomplete required browser page outputs",
            "invalid-page": "Incomplete required browser page outputs",
            "unsafe-path": "Invalid browser artifact path",
            "ambiguous-source": "Ambiguous browser source",
            "duplicate-source": "Duplicate canonical browser source",
            "incomplete-report": "Incomplete JavaScript executable records",
        }
        if fault in expected:
            assert expected[fault] in result.stderr, result.stderr
    else:
        assert result.returncode == 0, result.stdout + result.stderr
        report = (workspace / "report.lcov").read_text()
        assert report.count("SF:module.js\n") == 1
        assert (
            "DA:1,11\n" if fault in {None, "archived-page"} else "DA:1,4\n"
        ) in report
        result_receipt = json.loads((workspace / "evidence/receipt.json").read_text())
        assert any(
            limit["source"] == "page.astro"
            and limit.get("owners")
            and "Astro" in limit["missing_guarantee"]
            for limit in result_receipt["limits"]
        )
