"""Behavioral checks at the dependency preparation process boundary."""

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Protocol

import pytest


class Preparation(Protocol):
    def __call__(
        self, command: str = "prepare", **environment: str
    ) -> subprocess.CompletedProcess[str]: ...


type Runner = tuple[Preparation, Path, Path]


SCRIPT = Path(__file__).with_name("bazel_dependencies.py")


@pytest.fixture
def runner(tmp_path: Path) -> Runner:
    home = tmp_path / "home"
    cache = home / ".cache" / "bazel-repo"
    cache.mkdir(parents=True)
    (cache / "material").write_text("dependency")
    unrelated = home / ".cache" / "unrelated"
    unrelated.mkdir()
    (unrelated / "keep").write_text("preserved")
    bazel = tmp_path / "bazel"
    bazel.write_text('#!/bin/sh\nexit "${PREPARATION_STATUS:-0}"\n')
    bazel.chmod(0o755)

    def run(
        command: str = "prepare", **environment: str
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), command],
            env={
                **os.environ,
                "HOME": str(home),
                "PATH": f"{tmp_path}:{os.environ['PATH']}",
                "GITHUB_STEP_SUMMARY": str(tmp_path / "summary"),
                "GITHUB_ENV": str(tmp_path / "environment"),
                "RUNNER_OS": "Linux",
                "RUNNER_ARCH": "X64",
                **environment,
            },
            capture_output=True,
            text=True,
            check=False,
        )

    return run, cache, tmp_path


@pytest.mark.parametrize(
    ("outcome", "matched"),
    [("failure", "snapshot"), ("success", ""), ("failure", ""), ("skipped", "")],
)
def test_unconfirmed_restore_is_discarded_before_preparation(
    runner: Runner, outcome: str, matched: str
) -> None:
    run, cache, root = runner
    bazel = root / "bazel"
    bazel.write_text('#!/bin/sh\ntest ! -e "$HOME/.cache/bazel-repo/material"\n')
    result = run(RESTORE_OUTCOME=outcome, CACHE_MATCHED_KEY=matched)
    assert result.returncode == 0, result.stderr
    assert not (cache / "material").exists()
    assert (cache.parent / "unrelated" / "keep").read_text() == "preserved"
    assert "publication: disabled" in (root / "summary").read_text()


@pytest.mark.parametrize(
    ("hit", "classification"), [("true", "exact"), ("false", "prefix")]
)
def test_confirmed_restore_is_preserved_and_identified(
    runner: Runner, hit: str, classification: str
) -> None:
    run, cache, _ = runner
    result = run(RESTORE_OUTCOME="success", CACHE_MATCHED_KEY="snapshot", CACHE_HIT=hit)
    assert result.returncode == 0, result.stderr
    assert (cache / "material").read_text() == "dependency"
    evidence = json.loads(result.stdout)
    assert evidence["snapshot"] == "snapshot"
    assert evidence["restore"] == classification


@pytest.mark.parametrize("outcome", ["success", "failure"])
def test_preparation_failure_propagates_even_after_transport_failure(
    runner: Runner, outcome: str
) -> None:
    run, _, _ = runner
    result = run(
        RESTORE_OUTCOME=outcome, CACHE_MATCHED_KEY="snapshot", PREPARATION_STATUS="42"
    )
    assert result.returncode == 42
    assert json.loads(result.stdout)["preparation_exit_status"] == 42


def test_configuration_preserves_existing_flags_and_uses_shared_identity(
    runner: Runner,
) -> None:
    run, cache, root = runner
    bazelrc = root / "home" / ".bazelrc"
    bazelrc.write_text("common --keep_going\n")
    result = run(command="configure")
    assert result.returncode == 0, result.stderr
    assert bazelrc.read_text() == (
        f"common --keep_going\n\ncommon --repository_cache={cache}\n"
        f"common --repo_contents_cache={cache}/contents\n"
    )
    assert (root / "environment").read_text() == (
        f"BAZEL_DEPENDENCY_CACHE={cache}\nBAZEL_DEPENDENCY_PREFIX=bazel-deps-v1-Linux-X64-\n"
    )
