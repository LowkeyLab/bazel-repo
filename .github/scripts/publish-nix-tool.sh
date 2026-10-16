#!/usr/bin/env bash
set -euo pipefail
if [[ -z "$CACHIX_AUTH_TOKEN" ]]; then
	echo '::warning::Publication unconfigured: CACHIX_AUTH_TOKEN missing; ordinary consumers may build from source'
	exit 0
fi
if ! timeout --kill-after=5s 15m "$CACHIX_PATH/bin/cachix" push "$CACHIX_CACHE_NAME" "$TOOL_PATH"; then
	echo '::warning::Cachix publication failed; ordinary consumers may build from source'
	exit 0
fi
unset CACHIX_AUTH_TOKEN
if python3 .github/scripts/nix_tool.py verify "$NIX_TOOL" "$TOOL_PATH"; then
	echo "Verified remote runtime closure for $NIX_TOOL ($TOOL_PATH)" >>"$GITHUB_STEP_SUMMARY"
else
	echo '::warning::Publication completed but remote closure is unavailable; degraded source fallback, not verified warm success'
fi
