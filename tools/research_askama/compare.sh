#!/usr/bin/env bash
set -euo pipefail
cd "$TEST_SRCDIR/$TEST_WORKSPACE"
output="$TEST_UNDECLARED_OUTPUTS_DIR"
mode="$1"
shift
index=0
for executable in "$@"; do
	mkdir -p "$output/run-$index"
	TEST_UNDECLARED_OUTPUTS_DIR="$output/run-$index" "$executable" >"$output/run-$index/stdout" 2>"$output/run-$index/stderr"
	index=$((index + 1))
done
if [[ "$mode" == whitespace ]]; then
	old_mismatches=0
	for sample in 0 1 2 3; do
		if ! cmp -s "$output/run-0/render-$sample.html" "$output/run-1/render-$sample.html"; then old_mismatches=$((old_mismatches + 1)); fi
		cmp "$output/run-0/render-$sample.html" "$output/run-2/render-$sample.html"
	done
	test "$old_mismatches" -eq 4
	printf '{"old_mismatches":%s,"fixed_exact_matches":4,"functional":true}\n' "$old_mismatches" >"$output/comparison.json"
else
	for sample in 0 1; do cmp "$output/run-0/render-$sample.html" "$output/run-1/render-$sample.html"; done
	printf '{"native_adapter_exact_matches":2,"functional":true}\n' >"$output/comparison.json"
fi
