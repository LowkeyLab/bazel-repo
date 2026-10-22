"""Configure the Bazel dependency-cache pilot and its publication policy."""

import hashlib
import json
import os
import shutil
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


def prepare() -> int:
    """Discard unconfirmed restores, then propagate the real preload exit status."""
    outcome = os.environ.get("RESTORE_OUTCOME", "")
    matched = os.environ.get("CACHE_MATCHED_KEY", "")
    confirmed = outcome == "success" and bool(matched)
    cache = cache_path()
    if not confirmed:
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
        if not path.resolve().is_relative_to(root):
            raise RuntimeError("Snapshot inspection found an external link")
        if any(value in relative.encode() for value in credentials):
            raise RuntimeError("Snapshot inspection found a credential in a path")
        if path.is_dir():
            continue
        if not path.is_file():
            raise RuntimeError("Snapshot inspection found a special or missing file")
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
    trusted = (
        os.environ.get("GITHUB_EVENT_NAME") == "push"
        and os.environ.get("GITHUB_REF") == "refs/heads/main"
    )
    if os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        head = (event.get("pull_request") or {}).get("head") or {}
        repository = os.environ.get("GITHUB_REPOSITORY", "")
        trusted = bool(repository) and (
            (head.get("repo") or {}).get("full_name") == repository
        )
    eligible = successful and trusted
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
