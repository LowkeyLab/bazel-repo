#!/usr/bin/env bash
# Wrapper script to run ktlint with --format flag

set -euo pipefail

# Use Bazel runfiles to locate ktlint
if [ -n "${RUNFILES_DIR:-}" ]; then
    RUNFILES="$RUNFILES_DIR"
else
    # Default to relative path from script
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    RUNFILES="${SCRIPT_DIR}.runfiles"
fi

# Try to find ktlint in runfiles - use +_repo_rules+ prefix
KTLINT_BIN="${RUNFILES}/+_repo_rules+ktlint/ktlint"

if [ ! -x "$KTLINT_BIN" ]; then
    # Try _main workspace prefix
    KTLINT_BIN="${RUNFILES}/_main/external/+_repo_rules+ktlint/ktlint"
fi

if [ ! -x "$KTLINT_BIN" ]; then
    echo "Error: ktlint binary not found" >&2
    echo "Tried: ${RUNFILES}/+_repo_rules+ktlint/ktlint" >&2
    echo "Tried: ${RUNFILES}/_main/external/+_repo_rules+ktlint/ktlint" >&2
    exit 1
fi

# Run ktlint with --format flag and pass through all arguments
exec "$KTLINT_BIN" --format "$@"
