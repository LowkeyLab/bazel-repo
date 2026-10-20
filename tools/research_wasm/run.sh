#!/usr/bin/env bash
set -euo pipefail
cd "$TEST_SRCDIR/$TEST_WORKSPACE"
export LD_LIBRARY_PATH="${NIX_LD_LIBRARY_PATH}${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$@"
