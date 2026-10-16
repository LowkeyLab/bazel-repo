"""Configure and prepare the restore-only Bazel dependency-cache pilot."""

import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path


def cache_path():
    """The dedicated dependency directory never includes an output base or secrets."""
    return Path.home() / ".cache" / "bazel-repo"


def configure():
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


def prepare():
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
        "publication": "disabled",
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
                f"\nBazel dependency pilot (publication: disabled)\n\n```json\n{message}\n```\n"
            )
    return status if status >= 0 else 128 - status


if __name__ == "__main__":
    if sys.argv[1:] == ["configure"]:
        configure()
    elif sys.argv[1:] == ["prepare"]:
        sys.exit(prepare())
    else:
        sys.exit("usage: bazel_dependencies.py configure|prepare")
