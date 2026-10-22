"""Configure the Bazel dependency-cache pilot and its publication policy."""

import hashlib
import json
import os
import select
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path


def cache_path() -> Path:
    """Return the dependency directory outside the output base."""
    return Path.home() / ".cache" / "bazel-repo"


def configure() -> None:
    """Export shared workflow identity and configure Bazel after setup-bazel-ci."""
    cache = cache_path()
    prefix = f"bazel-deps-v1-{os.environ['RUNNER_OS']}-{os.environ['RUNNER_ARCH']}-"
    with Path(os.environ["GITHUB_ENV"]).open("a") as output:
        output.write(
            f"BAZEL_DEPENDENCY_CACHE={cache}\nBAZEL_DEPENDENCY_PREFIX={prefix}\n"
        )
    with (Path.home() / ".bazelrc").open("a") as output:
        output.write(
            f"\ncommon --repository_cache={cache}\ncommon --repo_contents_cache={cache}/contents\n"
        )


def stop_restore_writers() -> int:
    """Stop only processes marked by this job's restore step."""
    marker = os.environ.get("BAZEL_DEPENDENCY_RESTORE_MARKER")
    if not marker:
        return 0
    expected = f"BAZEL_DEPENDENCY_RESTORE_WRITER={marker}".encode()
    stopped = set()
    deadline = time.monotonic() + 10
    while True:
        found = False
        for process in Path("/proc").iterdir():
            if not process.name.isdecimal():
                continue
            # Separate handlers keep Python 3.12 syntax after formatting.
            try:
                descriptor = os.pidfd_open(int(process.name))
            except ProcessLookupError:
                continue
            except PermissionError:
                continue
            try:
                if expected not in (process / "environ").read_bytes().split(b"\x00"):
                    continue
                found = True
                # A process handle prevents a reused PID from naming another process.
                signal.pidfd_send_signal(descriptor, signal.SIGKILL)
                stopped.add(process.name)
                poller = select.poll()
                poller.register(descriptor, select.POLLIN)
                poller.poll(1000)
            except FileNotFoundError:
                continue
            except ProcessLookupError:
                continue
            except PermissionError:
                continue
            finally:
                os.close(descriptor)
            if time.monotonic() > deadline:
                raise RuntimeError("Timed-out restore processes did not stop")
        if not found:
            return len(stopped)
        if time.monotonic() > deadline:
            raise RuntimeError("Timed-out restore processes did not stop")


def prepare() -> int:
    """Discard unconfirmed restores, then propagate the real preload exit status."""
    outcome = os.environ.get("RESTORE_OUTCOME", "")
    matched = os.environ.get("CACHE_MATCHED_KEY", "")
    confirmed = outcome == "success" and bool(matched)
    cache = cache_path()
    stopped = 0
    if not confirmed:
        stopped = stop_restore_writers()
        # A transport error may be caught inside the restore action. No matched
        # output also requires cleanup, even when the action reports success.
        if cache.is_symlink():
            cache.unlink()
        elif cache.exists():
            shutil.rmtree(cache)
    cache.mkdir(parents=True, exist_ok=True)
    result = {
        "restore_outcome": outcome,
        "snapshot": matched,
        "restore": ("exact" if os.environ.get("CACHE_HIT") == "true" else "prefix")
        if confirmed
        else "miss-or-transport-error",
        "discarded_unconfirmed_state": not confirmed,
        "stopped_restore_writers": stopped,
        "publication": "not-attempted",
    }
    started = time.monotonic()
    status = subprocess.run(
        ["bazel", "fetch", "--config=ci", "//..."], check=False
    ).returncode
    result.update(
        preparation_seconds=time.monotonic() - started, preparation_exit_status=status
    )
    message = json.dumps(result, sort_keys=True)
    print(message, flush=True)
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary).open("a") as output:
            output.write(
                f"\nBazel dependency pilot (publication: not-attempted)\n\n```json\n{message}\n```\n"
            )
    return status if status >= 0 else 128 - status


def inspect() -> None:
    """Inventory material and reject known job credentials or external links."""
    cache = cache_path()
    if cache.is_symlink() or not cache.is_dir():
        raise RuntimeError("Snapshot inspection requires a real cache directory")
    root = cache.resolve()
    credentials = [
        value.encode()
        for name in (
            "GITHUB_TOKEN",
            "BAZELISK_GITHUB_TOKEN",
            "ASPECT_API_TOKEN",
            "BUILDBUDDY_API_KEY",
            "DOCKER_HUB_TOKEN",
        )
        if (value := os.environ.get(name))
    ]
    overlap = max((len(value) for value in credentials), default=1) - 1
    files = []
    sizes = {"archive_bytes": 0, "contents_bytes": 0}
    for path in sorted(cache.rglob("*")):
        relative = path.relative_to(cache).as_posix()
        link = str(path.readlink()) if path.is_symlink() else None
        if any(
            value in relative.encode() or (link is not None and value in link.encode())
            for value in credentials
        ):
            raise RuntimeError(
                "Snapshot inspection found a credential in a path or link"
            )
        if not path.resolve().is_relative_to(root):
            raise RuntimeError(
                f"Snapshot inspection found an external link: {relative}"
            )
        if link is not None:
            files.append({"path": relative, "symlink": link})
            continue
        if path.is_dir():
            continue
        if not path.is_file():
            raise RuntimeError(
                f"Snapshot inspection found a special or missing file: {relative} (symlink={path.is_symlink()})"
            )
        digest = hashlib.sha256()
        size = 0
        tail = b""
        with path.open("rb") as source:
            while chunk := source.read(1024 * 1024):
                if any(value in tail + chunk for value in credentials):
                    raise RuntimeError("Snapshot inspection found a credential")
                digest.update(chunk)
                size += len(chunk)
                tail = (tail + chunk)[-overlap:] if overlap else b""
        sizes[
            "contents_bytes" if relative.startswith("contents/") else "archive_bytes"
        ] += size
        files.append({"path": relative, "bytes": size, "sha256": digest.hexdigest()})
    result = {
        **sizes,
        "files": files,
        "revision": os.environ.get("GITHUB_SHA", ""),
        "source_ref": os.environ.get("GITHUB_REF", ""),
        "limitation": "Known-credential scan and path inspection do not prove public provenance.",
    }
    target = Path(os.environ["RUNNER_TEMP"]) / "bazel-dependency-inventory.json"
    target.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    print(json.dumps({**sizes, "file_count": len(files)}, sort_keys=True))


def publication() -> None:
    """Permit snapshot publication only after successful trusted test jobs."""
    successful = all(
        os.environ.get(name) == "success"
        for name in ("PREPARATION_OUTCOME", "TEST_OUTCOME", "JOB_STATUS")
    )
    eligible_event = (
        os.environ.get("GITHUB_EVENT_NAME") == "push"
        and os.environ.get("GITHUB_REF") == "refs/heads/main"
    )
    if os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        head = (event.get("pull_request") or {}).get("head") or {}
        repository = os.environ.get("GITHUB_REPOSITORY", "")
        eligible_event = bool(repository) and (
            (head.get("repo") or {}).get("full_name") == repository
        )
    eligible = successful and eligible_event
    result = {
        "eligible": eligible,
        "reason": "eligible" if eligible else "unsuccessful-or-unauthorized",
        "preparation_outcome": os.environ.get("PREPARATION_OUTCOME", ""),
        "test_outcome": os.environ.get("TEST_OUTCOME", ""),
        "job_status": os.environ.get("JOB_STATUS", ""),
    }
    print(json.dumps(result, sort_keys=True), flush=True)
    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
        output.write(f"eligible={str(eligible).lower()}\n")
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with Path(summary).open("a") as output:
            output.write(
                f"\nBazel dependency publication\n\n```json\n{json.dumps(result, sort_keys=True)}\n```\n"
            )


if __name__ == "__main__":
    if sys.argv[1:] == ["configure"]:
        configure()
    elif sys.argv[1:] == ["prepare"]:
        sys.exit(prepare())
    elif sys.argv[1:] == ["inspect"]:
        inspect()
    elif sys.argv[1:] == ["publication"]:
        publication()
    else:
        sys.exit("usage: bazel_dependencies.py configure|prepare|inspect|publication")
