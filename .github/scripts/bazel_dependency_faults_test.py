"""Check save fault evidence at the acceptance verification boundary."""

import json
import time
from pathlib import Path

import pytest
from bazel_dependency_faults import verify_save


@pytest.fixture
def save_evidence(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    evidence = tmp_path / "bazel-dependency-faults"
    evidence.mkdir()
    (evidence / "save-started").write_text(str(time.time() - 130))
    (evidence / "requests.jsonl").write_text(
        json.dumps(
            {
                "method": "POST",
                "path": "/twirp/github.actions.results.api.v1.CacheService/CreateCacheEntry",
                "time": time.time() - 120,
            }
        )
        + "\n"
    )
    (evidence / "preparation.log").write_text(
        json.dumps({"preparation_exit_status": 0}) + "\n"
    )
    monkeypatch.setenv("RUNNER_TEMP", str(tmp_path))
    monkeypatch.setenv("SAVE_OUTCOME", "failure")
    return evidence


def test_save_failure_requires_a_real_endpoint_request(save_evidence: Path) -> None:
    (save_evidence / "requests.jsonl").write_text("")
    with pytest.raises(RuntimeError, match="Save never contacted"):
        verify_save("error")


def test_save_stall_retains_observed_deadline_overhead(save_evidence: Path) -> None:
    verify_save("stall")
    result = json.loads((save_evidence / "save-result.json").read_text())
    assert result["save_elapsed_seconds"] >= 130
    assert result["deadline_overhead_seconds"] >= 10
    assert result["scope"] == "isolated-fixture"


def test_stall_without_step_failure_is_not_deadline_evidence(
    save_evidence: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("SAVE_OUTCOME", "success")
    with pytest.raises(RuntimeError, match="two-minute deadline"):
        verify_save("stall")
    assert not (save_evidence / "save-result.json").exists()


def test_save_error_requires_successful_preparation(save_evidence: Path) -> None:
    (save_evidence / "preparation.log").write_text(
        json.dumps({"preparation_exit_status": 42}) + "\n"
    )
    with pytest.raises(RuntimeError, match="successful preparation"):
        verify_save("error")


def test_internal_save_warning_can_preserve_step_success(
    save_evidence: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("SAVE_OUTCOME", "success")
    verify_save("error")
    result = json.loads((save_evidence / "save-result.json").read_text())
    assert result["save_step_outcome"] == "success"
