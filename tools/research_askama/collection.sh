#!/usr/bin/env bash
set -euo pipefail
cd "$TEST_SRCDIR/$TEST_WORKSPACE"
output="$TEST_UNDECLARED_OUTPUTS_DIR"
mkdir -p "$output/render"
TEST_UNDECLARED_OUTPUTS_DIR="$output/render" "$1" >"$output/render/stdout" 2>"$output/render/stderr"
"$2" "$3/baseline.json" "$output/render/events.json" "$output/render/completion.json" "$output/valid" valid
for control in missing-include source-hash unknown-event incomplete profile-hash; do
	mkdir -p "$output/$control"
	set +e
	"$2" "$3/baseline.json" "$output/render/events.json" "$output/render/completion.json" "$output/$control" "$control" >"$output/$control/stdout" 2>"$output/$control/stderr"
	result=$?
	set -e
	printf '%s\n' "$result" >"$output/$control/exit"
	test "$result" -ne 0
done
cmp "$output/valid/combined-zero-first.lcov" "$output/valid/combined-zero-last.lcov"
cmp "$output/valid/combined-zero-first.lcov" "$4"
