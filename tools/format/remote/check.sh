#!/usr/bin/env bash
# Run from an owned checkout. The input directory is also the per-checkout lock.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
input=tools/format/remote/input
umask 077
mkdir "$input"
trap 'rm -f "$input/workspace.tar"; rmdir "$input"' EXIT
bash tools/format/remote/snapshot.sh "$input/workspace.tar"
startup=()
if [[ -n ${BAZEL_OUTPUT_BASE:-} ]]; then
	startup+=("--output_base=$BAZEL_OUTPUT_BASE")
fi
"${BAZEL_REAL:-bazel}" "${startup[@]}" test "$@" \
	--execution_log_compact_file= --remote_grpc_log= \
	--config=remote-linux --spawn_strategy=remote --strategy_regexp='.*=remote' \
	--noremote_local_fallback --remote_local_fallback_strategy=remote \
	--strategy=CoverageReport=remote --java_runtime_version=remotejdk_21 \
	--tool_java_runtime_version=remotejdk_21 --nocache_test_results \
	--remote_download_toplevel --nozip_undeclared_test_outputs --test_output=errors \
	//tools/format/remote:scope_test //tools/format/remote:format_test
