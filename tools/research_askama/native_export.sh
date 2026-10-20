#!/usr/bin/env bash
set -euo pipefail
cd "$TEST_SRCDIR/$TEST_WORKSPACE"
native=$1
profdata=$2
cov=$3
"$native"
"$profdata" --version >"$TEST_UNDECLARED_OUTPUTS_DIR/profdata-version.txt"
"$cov" --version >"$TEST_UNDECLARED_OUTPUTS_DIR/cov-version.txt"
"$profdata" merge -sparse "$TEST_UNDECLARED_OUTPUTS_DIR/native.profraw" -o "$TEST_UNDECLARED_OUTPUTS_DIR/native.profdata"
"$cov" export "$native" --instr-profile="$TEST_UNDECLARED_OUTPUTS_DIR/native.profdata" >"$TEST_UNDECLARED_OUTPUTS_DIR/native.json"
"$cov" export "$native" --instr-profile="$TEST_UNDECLARED_OUTPUTS_DIR/native.profdata" --format=lcov >"$TEST_UNDECLARED_OUTPUTS_DIR/native.lcov"
"$cov" export "$native" --empty-profile --format=lcov >"$TEST_UNDECLARED_OUTPUTS_DIR/native-zero.lcov"
