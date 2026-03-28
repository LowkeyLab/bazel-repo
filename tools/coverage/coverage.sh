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

TARGETS="${*:-//...}"
REPORT_DIR="coverage-report"
COMBINED_LCOV="coverage-report.lcov"
COVERAGE_DAT="$(bazel info output_path)/_coverage/_coverage_report.dat"

echo "==> Running bazel coverage for: ${TARGETS}"
bazel coverage "${TARGETS}" 2>&1

if [[ ! -f "${COVERAGE_DAT}" ]]; then
	echo "ERROR: Combined coverage report not found at ${COVERAGE_DAT}"
	echo "This can happen if no test targets produced coverage data."
	exit 1
fi

# Copy combined report with workspace-relative paths
cp "${COVERAGE_DAT}" "${COMBINED_LCOV}"
echo "==> Combined lcov written to: ${COMBINED_LCOV}"

# Generate HTML report if genhtml is available
if command -v genhtml &>/dev/null; then
	echo "==> Generating HTML coverage report..."
	genhtml "${COMBINED_LCOV}" \
		--output-directory "${REPORT_DIR}" \
		--title "bazel-repo Coverage" \
		--legend \
		--show-details \
		--quiet
	echo "==> HTML report: ${REPORT_DIR}/index.html"
	echo "    Open with: xdg-open ${REPORT_DIR}/index.html"
else
	echo "WARN: genhtml not found — skipping HTML report generation."
	echo "      Install lcov for HTML reports: pacman -S lcov / apt install lcov / brew install lcov"
	echo "      The raw lcov data is available at: ${COMBINED_LCOV}"
fi
