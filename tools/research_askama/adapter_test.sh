#!/usr/bin/env bash
set -euo pipefail
cd "$TEST_SRCDIR/$TEST_WORKSPACE"
exporter=$1
native=$2
profdata=$3
cov=$4
adapter=$5
collector=$6
instrumented=$7
cp "$8" "$TEST_UNDECLARED_OUTPUTS_DIR/compiler.txt"
sha256sum "$native" "$adapter" >"$TEST_UNDECLARED_OUTPUTS_DIR/binary-sha256.txt"
sha256sum tools/research_askama/*.rs tools/research_askama/*.sh tools/research_askama/instrument.bzl tools/research_askama/templates/*.html >"$TEST_UNDECLARED_OUTPUTS_DIR/input-sha256.txt"
"$exporter" "$native" "$profdata" "$cov"
export ASKAMA_BASELINE="$instrumented/baseline.json"
"$adapter"
cp "$ASKAMA_BASELINE" "$TEST_UNDECLARED_OUTPUTS_DIR/ast-baseline.json"
cp -R "$instrumented/templates" "$TEST_UNDECLARED_OUTPUTS_DIR/instrumented-templates"
profile="$TEST_UNDECLARED_OUTPUTS_DIR/template-profile.json"
receipt="$TEST_UNDECLARED_OUTPUTS_DIR/template-completion.json"
collected="$TEST_UNDECLARED_OUTPUTS_DIR/collected"
"$collector" "$ASKAMA_BASELINE" "$profile" "$receipt" "$TEST_UNDECLARED_OUTPUTS_DIR/native.lcov" "$collected"
negative() {
	local name=$1 baseline=$2
	if "$collector" "$baseline" "$profile" "$receipt" "$TEST_UNDECLARED_OUTPUTS_DIR/native.lcov" "$TEST_UNDECLARED_OUTPUTS_DIR/$name" >"$TEST_UNDECLARED_OUTPUTS_DIR/$name.stdout" 2>"$TEST_UNDECLARED_OUTPUTS_DIR/$name.stderr"; then
		echo "Unexpected collector success: $name" >&2
		exit 1
	else
		local status=$?
		test "$status" -eq 1
		test -s "$TEST_UNDECLARED_OUTPUTS_DIR/template-functional.txt"
		printf '%s exit=%s functional_assertions=passed\n' "$name" "$status" >>"$TEST_UNDECLARED_OUTPUTS_DIR/controls.txt"
	fi
}
mv "$profile" "$profile.saved"
negative missing-profile "$ASKAMA_BASELINE"
mv "$profile.saved" "$profile"
cp "$profile" "$profile.saved"
printf '{corrupt' >"$profile"
negative corrupt-profile "$ASKAMA_BASELINE"
mv "$profile.saved" "$profile"
mv "$receipt" "$receipt.saved"
negative missing-receipt "$ASKAMA_BASELINE"
mv "$receipt.saved" "$receipt"
negative missing-baseline "$TEST_UNDECLARED_OUTPUTS_DIR/absent-baseline.json"
