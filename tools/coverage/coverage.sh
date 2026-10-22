#!/usr/bin/env bash
# Runs bazel coverage and generates an HTML report from the combined lcov output.
#
# Usage:
#   tools/coverage/coverage.sh [targets...]
#
# Examples:
#   tools/coverage/coverage.sh                    # Cover all targets
#   tools/coverage/coverage.sh //nicknamer/...     # Cover nicknamer only
#   tools/coverage/coverage.sh //predix/...        # Cover predix only
#
# Output:
#   coverage-report/          HTML report directory
#   coverage-report.lcov      Combined lcov file (workspace-relative paths)
#
# Requirements:
#   lcov (provides genhtml) — install via: pacman -S lcov / apt install lcov / brew install lcov

set -euo pipefail

# When invoked via `bazel run`, change to the workspace root
if [[ -n "${BUILD_WORKSPACE_DIRECTORY:-}" ]]; then
	cd "${BUILD_WORKSPACE_DIRECTORY}"
fi

if [ $# -eq 0 ]; then
	ARGS=("//...")
else
	ARGS=("$@")
fi
REPORT_DIR="coverage-report"
COMBINED_LCOV="coverage-report.lcov"
rm -f coverage-reconciliation.json
# Preserve endpoint overrides when querying the report location.
INFO_ARGS=()
for arg in "${ARGS[@]}"; do
	case "${arg}" in
	--remote_cache=* | --remote_executor=* | --bes_backend=*)
		INFO_ARGS+=("${arg}")
		;;
	esac
done
INVENTORY_ARGS=()
for arg in "${INFO_ARGS[@]}"; do
	INVENTORY_ARGS+=("--bazel-option=${arg}")
done
ROOTS_FILE="$(mktemp)"
TEMP_EVENTS_FILE="$(mktemp)"
ANALYZER_SCRIPT="$(mktemp)"
JAVASCRIPT_COLLECTOR="$(mktemp)"
JAVASCRIPT_MERGER="$(mktemp)"
EVENTS_FILE="$TEMP_EVENTS_FILE"
EVENTS_ARGS=("--build_event_json_file=$EVENTS_FILE")
for arg in "${ARGS[@]}"; do
	case "$arg" in
	--build_event_json_file=*)
		EVENTS_ARGS=()
		EVENTS_FILE="${arg#*=}"
		;;
	esac
done
trap 'rm -f "$ROOTS_FILE" "$TEMP_EVENTS_FILE" "$ANALYZER_SCRIPT" "$JAVASCRIPT_COLLECTOR" "$JAVASCRIPT_MERGER"' EXIT
bazel run "${INFO_ARGS[@]}" //tools/coverage:inventory -- \
	--output coverage-inventory.json --roots-output "$ROOTS_FILE" "${INVENTORY_ARGS[@]}"
mapfile -t COVERAGE_ROOTS <"$ROOTS_FILE"
COVERAGE_DAT="$(bazel info "${INFO_ARGS[@]}" output_path)/_coverage/_coverage_report.dat"

echo "==> Running bazel coverage for: ${ARGS[*]}"
bazel coverage "${ARGS[@]}" "${COVERAGE_ROOTS[@]}" "${EVENTS_ARGS[@]}" --nozip_undeclared_test_outputs \
	--aspects=//tools/coverage:jvm.bzl%jvm_test_inputs,//tools/coverage:javascript.bzl%javascript_test_inputs \
	--output_groups=+go_coverage_baselines,rust_coverage_baselines,jvm_coverage_baselines,jvm_test_inputs,javascript_test_inputs \
	--nobuild_event_json_file_path_conversion 2>&1

if [[ ! -f "${COVERAGE_DAT}" ]]; then
	echo "ERROR: Combined coverage report not found at ${COVERAGE_DAT}"
	echo "This can happen if no test targets produced coverage data."
	exit 1
fi

# Copy combined report with workspace-relative paths.
# Make it user-writable so downstream lcov merges (e.g. CI's local-only
# coverage step in .github/workflows/coverage.yml) can overwrite it; the
# Bazel coverage dat in the cache is read-only, so a plain `cp` would
# preserve those mode bits.
cp "${COVERAGE_DAT}" "${COMBINED_LCOV}"
chmod u+w "${COMBINED_LCOV}"
bazel run "${INFO_ARGS[@]}" //tools/coverage:go_report -- \
	--inventory coverage-inventory.json --report "${COMBINED_LCOV}" \
	--events "$EVENTS_FILE" --expected-output coverage-go-baselines.json
bazel run "${INFO_ARGS[@]}" //tools/coverage:go_report -- \
	--language rust --inventory coverage-inventory.json --report "${COMBINED_LCOV}" \
	--events "$EVENTS_FILE" --expected-output coverage-rust-baselines.json
bazel run "${INFO_ARGS[@]}" --script_path="$ANALYZER_SCRIPT" //tools/coverage:jvm_analyzer
bazel run "${INFO_ARGS[@]}" //tools/coverage:jvm_report -- \
	--inventory coverage-inventory.json --report "${COMBINED_LCOV}" \
	--events "$EVENTS_FILE" --evidence coverage-jvm \
	--analyzer "$ANALYZER_SCRIPT"
bazel run "${INFO_ARGS[@]}" --script_path="$JAVASCRIPT_COLLECTOR" //tools/coverage:javascript_baseline
bazel run "${INFO_ARGS[@]}" --script_path="$JAVASCRIPT_MERGER" @bazel_tools//tools/test:lcov_merger
bazel run "${INFO_ARGS[@]}" //tools/coverage:javascript_report -- \
	--inventory coverage-inventory.json --report "${COMBINED_LCOV}" \
	--events "$EVENTS_FILE" --evidence coverage-javascript \
	--collector "$JAVASCRIPT_COLLECTOR" --merger "$JAVASCRIPT_MERGER"
bazel run "${INFO_ARGS[@]}" //tools/coverage:reconcile -- \
	--inventory coverage-inventory.json --report "${COMBINED_LCOV}" \
	--go coverage-go-baselines.json --rust coverage-rust-baselines.json \
	--jvm coverage-jvm/receipt.json --javascript coverage-javascript/receipt.json \
	--output coverage-reconciliation.json
echo "==> Combined lcov written to: ${COMBINED_LCOV}"

# Generate HTML report if genhtml is available
if command -v genhtml &>/dev/null; then
	echo "==> Generating HTML coverage report..."
	# rules_java emits negative line numbers for generated JVM functions.
	# Ignore the resulting format errors and unmatched FNDA records twice
	# so lcov continues without printing a warning for every function.
	genhtml "${COMBINED_LCOV}" \
		--output-directory "${REPORT_DIR}" \
		--title "bazel-repo Coverage" \
		--legend \
		--show-details \
		--ignore-errors category,format,format,mismatch,mismatch \
		--quiet
	echo "==> HTML report: ${REPORT_DIR}/index.html"
	echo "    Open with: xdg-open ${REPORT_DIR}/index.html"
else
	echo "WARN: genhtml not found — skipping HTML report generation."
	echo "      Install lcov for HTML reports: pacman -S lcov / apt install lcov / brew install lcov"
	echo "      The raw lcov data is available at: ${COMBINED_LCOV}"
fi
