#!/usr/bin/env bash
# Check the runner protocol without starting Bazel or the report tools.
set -euo pipefail
wrapper=$(realpath "${1:-$(dirname "$0")/coverage.sh}")
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
cd "$scratch"
unset BUILD_WORKSPACE_DIRECTORY

# These functions model the command boundary, not collector behavior.
git() { [[ $* == 'rev-parse HEAD' ]] && printf 'fixture-revision\n'; }
bazel() {
	[[ $1 == --digest_function=SHA256 ]] || return 81
	local command=$2 arg
	shift 2
	for arg in --config=remote-linux --spawn_strategy=remote '--strategy_regexp=.*=remote' \
		--noremote_local_fallback --remote_local_fallback_strategy=remote --strategy=CoverageReport=remote; do
		[[ " $* " == *" $arg "* ]] || return 82
	done
	case "$command" in
	info) printf '%s\n' "$PWD" ;;
	coverage)
		[[ " $* " == *' //... '* && " $* " == *' //fixture:baseline '* ]] || return 83
		mkdir -p _coverage && touch _coverage/_coverage_report.dat
		;;
	run)
		while (($#)); do
			case "$1" in
			//tools/coverage:reconcile) [[ $FAIL_RECONCILE == no ]] || return 29 ;;
			--roots-output)
				shift
				printf '//fixture:baseline\n' >"$1"
				;;
			--output | --expected-output)
				shift
				touch "$1"
				;;
			--evidence)
				shift
				mkdir -p "$1" && touch "$1/receipt.json"
				;;
			esac
			shift
		done
		;;
	*) return 84 ;;
	esac
}
export -f git bazel
export BUILDBUDDY_ARTIFACTS_DIRECTORY="$scratch/artifacts"
export FAIL_RECONCILE=yes
mkdir "$BUILDBUDDY_ARTIFACTS_DIRECTORY"
status=0
"$wrapper" --buildbuddy-runner || status=$?
[[ $status == 29 ]]
output="$BUILDBUDDY_ARTIFACTS_DIRECTORY/coverage"
[[ ! -e $output/reports.tar.gz && ! -e coverage-reconciliation.json ]]
[[ $(tail -n 1 "$output/status.txt") == *' run 29' ]]

export FAIL_RECONCILE=no
export BUILDBUDDY_ARTIFACTS_DIRECTORY="$scratch/success"
mkdir "$BUILDBUDDY_ARTIFACTS_DIRECTORY"
"$wrapper" --buildbuddy-runner
tar -tzf "$BUILDBUDDY_ARTIFACTS_DIRECTORY/coverage/reports.tar.gz" \
	coverage-inventory.json coverage-report.lcov coverage-{go,rust}-baselines.json \
	coverage-{jvm,javascript}/receipt.json coverage-reconciliation.json
