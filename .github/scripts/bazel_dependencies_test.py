"""Behavioral checks at the dependency preparation process boundary."""

import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Protocol

import pytest


class Preparation(Protocol):
    def __call__(
        self, command: str = "prepare", **environment: str
    ) -> subprocess.CompletedProcess[str]:
        pass


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
    assert "publication: not-attempted" in (root / "summary").read_text()


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


def test_successful_main_tests_allow_publication(runner: Runner) -> None:
    run, _, root = runner
    event = root / "event.json"
    event.write_text("{}")
    result = run(
        command="publication",
        GITHUB_EVENT_PATH=str(event),
        GITHUB_EVENT_NAME="push",
        GITHUB_REF="refs/heads/main",
        GITHUB_REPOSITORY="LowkeyLab/bazel-repo",
        PREPARATION_OUTCOME="success",
        TEST_OUTCOME="success",
        JOB_STATUS="success",
        GITHUB_OUTPUT=str(root / "outputs"),
    )
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["eligible"] is True
    assert (root / "outputs").read_text() == "eligible=true\n"


@pytest.mark.parametrize("author", ["contributor", "lowkey-lab-renovate[bot]"])
def test_successful_same_repository_pr_tests_allow_publication(
    runner: Runner, author: str
) -> None:
    run, _, root = runner
    event = root / "event.json"
    event.write_text(
        json.dumps(
            {
                "pull_request": {
                    "head": {"repo": {"full_name": "LowkeyLab/bazel-repo"}},
                    "user": {"login": author},
                }
            }
        )
    )
    result = run(
        command="publication",
        GITHUB_EVENT_PATH=str(event),
        GITHUB_EVENT_NAME="pull_request",
        GITHUB_REF="refs/pull/42/merge",
        GITHUB_REPOSITORY="LowkeyLab/bazel-repo",
        PREPARATION_OUTCOME="success",
        TEST_OUTCOME="success",
        JOB_STATUS="success",
        GITHUB_OUTPUT=str(root / "outputs"),
    )
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["eligible"] is True


@pytest.mark.parametrize(
    "environment",
    [
        {"PREPARATION_OUTCOME": "failure"},
        {"PREPARATION_OUTCOME": "skipped"},
        {"TEST_OUTCOME": "failure"},
        {"TEST_OUTCOME": "cancelled"},
        {"TEST_OUTCOME": "skipped"},
        {"JOB_STATUS": "failure"},
        {"JOB_STATUS": "cancelled"},
        {"GITHUB_EVENT_NAME": "workflow_dispatch"},
        {"GITHUB_EVENT_NAME": "pull_request_target"},
        {"GITHUB_EVENT_NAME": "schedule"},
        {"GITHUB_REF": "refs/heads/topic"},
        {"GITHUB_EVENT_NAME": "pull_request", "GITHUB_REF": "refs/pull/42/merge"},
    ],
)
def test_failed_cancelled_or_unauthorized_jobs_cannot_publish(
    runner: Runner, environment: dict[str, str]
) -> None:
    run, _, root = runner
    event = root / "event.json"
    event.write_text(
        json.dumps(
            {
                "pull_request": {
                    "head": {"repo": {"full_name": "contributor/bazel-repo"}}
                }
            }
        )
    )
    result = run(
        command="publication",
        **{
            "GITHUB_EVENT_PATH": str(event),
            "GITHUB_EVENT_NAME": "push",
            "GITHUB_REF": "refs/heads/main",
            "GITHUB_REPOSITORY": "LowkeyLab/bazel-repo",
            "PREPARATION_OUTCOME": "success",
            "TEST_OUTCOME": "success",
            "JOB_STATUS": "success",
            "GITHUB_OUTPUT": str(root / "outputs"),
            **environment,
        },
    )
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout)["eligible"] is False
    assert (root / "outputs").read_text() == "eligible=false\n"


@pytest.mark.parametrize("credential", ["BUILDBUDDY_API_KEY", "BAZELISK_GITHUB_TOKEN"])
def test_snapshot_inspection_rejects_credentials_before_publication(
    runner: Runner,
    credential: str,
) -> None:
    run, cache, root = runner
    (cache / "material").write_text("generated file contains ephemeral-test-credential")
    result = run(
        command="inspect",
        RUNNER_TEMP=str(root),
        **{credential: "ephemeral-test-credential"},
    )
    assert result.returncode != 0
    assert "credential" in result.stderr.lower()
    assert "ephemeral-test-credential" not in result.stderr + result.stdout
    assert not (root / "bazel-dependency-inventory.json").exists()


def test_snapshot_inspection_records_contents_separately(runner: Runner) -> None:
    run, cache, root = runner
    (cache / "contents").mkdir()
    (cache / "contents" / "generated").write_bytes(b"public")
    result = run(command="inspect", RUNNER_TEMP=str(root))
    assert result.returncode == 0, result.stderr
    inventory = json.loads((root / "bazel-dependency-inventory.json").read_text())
    assert inventory["archive_bytes"] == len(b"dependency")
    assert inventory["contents_bytes"] == len(b"public")
    assert {entry["path"] for entry in inventory["files"]} == {
        "material",
        "contents/generated",
    }


def test_snapshot_inspection_rejects_links_to_external_material(runner: Runner) -> None:
    run, cache, root = runner
    (root / "private").write_text("outside cache")
    (cache / "external").symlink_to(root / "private")
    result = run(command="inspect", RUNNER_TEMP=str(root))
    assert result.returncode != 0
    assert "external link" in result.stderr
    assert not (root / "bazel-dependency-inventory.json").exists()


def test_snapshot_inspection_finds_credentials_across_read_boundaries(
    runner: Runner,
) -> None:
    run, cache, root = runner
    (cache / "material").write_bytes(
        b"x" * (1024 * 1024 - 4) + b"ephemeral-test-credential"
    )
    result = run(
        command="inspect",
        RUNNER_TEMP=str(root),
        GITHUB_TOKEN="ephemeral-test-credential",
    )
    assert result.returncode != 0
    assert "credential" in result.stderr.lower()
    assert "ephemeral-test-credential" not in result.stderr + result.stdout


def test_snapshot_inspection_preserves_internal_dangling_links(runner: Runner) -> None:
    run, cache, root = runner
    link = cache / "contents" / "repo" / "e2e" / "smoke" / "tools" / "downloader.cfg"
    link.parent.mkdir(parents=True)
    link.symlink_to("../../../tools/downloader.cfg")
    result = run(command="inspect", RUNNER_TEMP=str(root))
    assert result.returncode == 0, result.stderr
    inventory = json.loads((root / "bazel-dependency-inventory.json").read_text())
    assert {
        "path": "contents/repo/e2e/smoke/tools/downloader.cfg",
        "symlink": "../../../tools/downloader.cfg",
    } in inventory["files"]
    assert inventory["contents_bytes"] == 0
    assert link.is_symlink() and not link.exists()


def test_snapshot_inspection_rejects_credentials_in_link_targets(
    runner: Runner,
) -> None:
    run, cache, root = runner
    (cache / "link").symlink_to("ephemeral-test-credential")
    result = run(
        command="inspect",
        RUNNER_TEMP=str(root),
        BAZELISK_GITHUB_TOKEN="ephemeral-test-credential",
    )
    assert result.returncode != 0
    assert "credential" in result.stderr
    assert "ephemeral-test-credential" not in result.stderr + result.stdout


@pytest.mark.parametrize("outcome", ["failure", "success"])
def test_unconfirmed_restore_stops_its_writer_before_cleanup(
    runner: Runner, outcome: str
) -> None:
    run, cache, root = runner
    ready = root / "writer-ready"
    # The transport fixture uses only the standard library and skips site hooks.
    writer = subprocess.Popen(
        [
            sys.executable,
            "-S",
            "-c",
            (
                "import itertools,pathlib,sys\n"
                "cache=pathlib.Path(sys.argv[1]);ready=pathlib.Path(sys.argv[2])\n"
                "for index in itertools.count():\n"
                " try:\n"
                "  path=cache/'contents'/str(index);path.mkdir(parents=True,exist_ok=True)\n"
                "  (path/'material').write_bytes(b'x'*512)\n"
                "  if index==50:ready.touch()\n"
                " except FileNotFoundError:pass\n"
            ),
            str(cache),
            str(ready),
        ],
        env={**os.environ, "BAZEL_DEPENDENCY_RESTORE_WRITER": str(root)},
    )
    try:
        deadline = time.monotonic() + 5
        while not ready.exists():
            assert writer.poll() is None
            assert time.monotonic() < deadline
            time.sleep(0.001)
        result = run(
            RESTORE_OUTCOME=outcome,
            BAZEL_DEPENDENCY_RESTORE_MARKER=str(root),
        )
        assert result.returncode == 0, result.stderr
        writer.wait(timeout=5)
        assert list(cache.iterdir()) == []
        assert json.loads(result.stdout)["stopped_restore_writers"] >= 1
        assert (cache.parent / "unrelated" / "keep").read_text() == "preserved"
    finally:
        if writer.poll() is None:
            writer.terminate()
        writer.wait()


def test_cleanup_preserves_a_process_with_another_restore_marker(
    runner: Runner,
) -> None:
    run, _, root = runner
    unrelated = subprocess.Popen(
        [sys.executable, "-S", "-c", "import time;time.sleep(60)"],
        env={**os.environ, "BAZEL_DEPENDENCY_RESTORE_WRITER": f"other-{root}"},
    )
    try:
        result = run(
            RESTORE_OUTCOME="failure",
            BAZEL_DEPENDENCY_RESTORE_MARKER=str(root),
        )
        assert result.returncode == 0, result.stderr
        assert unrelated.poll() is None
    finally:
        unrelated.terminate()
        unrelated.wait()
