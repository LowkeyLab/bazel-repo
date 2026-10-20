#!/usr/bin/env bash
set -euo pipefail
cd "$TEST_SRCDIR/$TEST_WORKSPACE"
tool="$1"
shift
output="$TEST_UNDECLARED_OUTPUTS_DIR"
index=0
for source in "$@"; do
	mkdir -p "$output/case-$index"
	set +e
	"$tool" "$output/case-$index/output" "$source" >"$output/case-$index/stdout" 2>"$output/case-$index/stderr"
	result=$?
	set -e
	printf '%s\t%s\t%s\n' "$index" "$source" "$result" >>"$output/results.tsv"
	printf '%s\n' "$result" >"$output/case-$index/exit"
	index=$((index + 1))
done
printf 'Inspected %s source templates. Unsupported inputs retain their errors.\n' "$index"
