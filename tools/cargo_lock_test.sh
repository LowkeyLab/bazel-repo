#!/usr/bin/env bash
set -euo pipefail
# shellcheck disable=SC1091
source "${RUNFILES_DIR:-$0.runfiles}/bazel_tools/tools/bash/runfiles/runfiles.bash"
lock_tool="$(rlocation "$CARGO_LOCK_TOOL")"
workspace="$TEST_TMPDIR/workspace"
mkdir -p "$workspace/src"
cat >"$workspace/Cargo.toml" <<'MANIFEST'
[package]
name = "cargo-lock-regression"
version = "0.1.0"
edition = "2024"

[workspace]
MANIFEST
cat >"$workspace/Cargo.lock" <<'LOCKFILE'
version = 4

[[package]]
name = "cargo-lock-regression"
version = "0.0.0"
LOCKFILE
: >"$workspace/src/lib.rs"
export CARGO_HOME="$TEST_TMPDIR/cargo-home"
export CARGO_NET_OFFLINE=true
export BUILD_WORKSPACE_DIRECTORY="$workspace"
cd "$TEST_SRCDIR/$TEST_WORKSPACE"
"$lock_tool"
grep -Fq 'version = "0.1.0"' "$workspace/Cargo.lock"
