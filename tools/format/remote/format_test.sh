#!/usr/bin/env bash
set -euo pipefail
runroot="$TEST_SRCDIR/$TEST_WORKSPACE"
formatter="$runroot/$1"
export RUNFILES_DIR="$TEST_SRCDIR"
export BUILD_WORKSPACE_DIRECTORY="$TEST_TMPDIR/workspace"
export BUILD_WORKING_DIRECTORY="$BUILD_WORKSPACE_DIRECTORY"
mkdir "$BUILD_WORKSPACE_DIRECTORY"
cd "$BUILD_WORKSPACE_DIRECTORY"
tar -xf "$runroot/tools/format/remote/input/workspace.tar"
# The no-argument multirun uses Git for language and attribute selection.
git init -q
git add -f -- .
status=0
"$formatter" || status=$?
printf '%d\n' "$status" >"$TEST_UNDECLARED_OUTPUTS_DIR/formatter.exit"
git diff --no-ext-diff --binary >"$TEST_UNDECLARED_OUTPUTS_DIR/format.patch"
if ((status != 0)); then
	exit "$status"
fi
if [[ -s $TEST_UNDECLARED_OUTPUTS_DIR/format.patch ]]; then
	git diff --stat
	exit 1
fi
