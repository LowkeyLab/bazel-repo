"""Check the baseline CLI with real LLVM objects and invalid inputs."""

import json
import os
import runpy
import subprocess
from pathlib import Path

import pytest

ROOT = Path(os.environ["TEST_SRCDIR"]) / os.environ["TEST_WORKSPACE"]


def tool(name: str) -> str:
    return str(ROOT / os.environ[name])


def invoke(tmp_path: Path, object_path: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            str(ROOT / "tools/coverage/rust_baseline"),
            "--llvm-cov",
            tool("LLVM_COV"),
            "--llvm-ar",
            tool("LLVM_AR"),
            "--llvm-profdata",
            tool("LLVM_PROFDATA"),
            "--object",
            str(object_path),
            "--output",
            str(tmp_path / "baseline.lcov"),
            "--metadata",
            str(tmp_path / "baseline.json"),
            "--generated-source",
            "generated.rs",
            "--generated-source",
            "generated.rs",
        ],
        capture_output=True,
        text=True,
        check=False,
    )


@pytest.mark.parametrize("object_kind", ["archive", "object"])
def test_real_llvm_baselines_retain_zero_hit_executable_lines(
    tmp_path: Path, object_kind: str
) -> None:
    object_path = ROOT / os.environ["BASELINE_FIXTURE"]
    if object_kind == "object":
        members = subprocess.check_output(
            [tool("LLVM_AR"), "t", str(object_path)], text=True
        ).splitlines()
        member = next(name for name in members if name.endswith(".o"))
        object_path = tmp_path / "fixture.o"
        object_path.write_bytes(
            subprocess.check_output(
                [tool("LLVM_AR"), "p", tool("BASELINE_FIXTURE"), member]
            )
        )
    result = invoke(tmp_path, object_path)
    assert result.returncode == 0, result.stdout + result.stderr
    lines = (tmp_path / "baseline.lcov").read_text().splitlines()
    sources = [line[3:] for line in lines if line.startswith("SF:")]
    assert sources == ["tools/coverage/fixtures/rust_baseline/lib.rs"]
    counts = [int(line.split(",")[1]) for line in lines if line.startswith("DA:")]
    assert counts and not any(counts)
    metadata = json.loads((tmp_path / "baseline.json").read_text())
    assert metadata["object"] == str(object_path)
    assert metadata["generated_sources"] == ["generated.rs"]
    assert metadata["no_mapping"] is False
    assert metadata["diagnostic"] == ""


@pytest.mark.parametrize("fault", ["missing", "empty-archive", "corrupt-object"])
def test_invalid_required_objects_fail_without_writing_a_baseline(
    tmp_path: Path, fault: str
) -> None:
    object_path = tmp_path / "invalid.o"
    if fault == "empty-archive":
        object_path = tmp_path / "empty.rlib"
        subprocess.run([tool("LLVM_AR"), "rcs", str(object_path)], check=True)
    elif fault == "corrupt-object":
        object_path.write_bytes(b"not an object")
    result = invoke(tmp_path, object_path)
    assert result.returncode != 0
    assert result.stderr
    assert not (tmp_path / "baseline.lcov").exists()
    assert not (tmp_path / "baseline.json").exists()


@pytest.mark.parametrize("aspect_first", [False, True])
def test_membership_metadata_ignores_aspect_events(
    tmp_path: Path, aspect_first: bool
) -> None:
    metadata = {"object": "fixture.o"}
    (tmp_path / "metadata.json").write_text(json.dumps(metadata))
    label = "//fixture:test.rust_coverage"
    target = {
        "id": {"targetCompleted": {"label": label}},
        "completed": {
            "outputGroup": [
                {"name": "rust_coverage_baselines", "fileSets": [{"id": "metadata"}]}
            ]
        },
    }
    aspect = {
        "id": {
            "targetCompleted": {
                "label": label,
                "aspect": "//tools/coverage:jvm.bzl%jvm_test_inputs",
            }
        },
        "completed": {"success": True},
    }
    events = [
        {
            "id": {"namedSet": {"id": "metadata"}},
            "namedSetOfFiles": {"files": [{"name": "metadata.json"}]},
        },
        *([aspect, target] if aspect_first else [target, aspect]),
    ]
    events_file = tmp_path / "events.jsonl"
    events_file.write_text("".join(json.dumps(event) + "\n" for event in events))
    module = runpy.run_path(str(ROOT / "tools/coverage/rust_membership_test.py"))
    assert module["object_metadata"](events_file, label, tmp_path) == metadata
