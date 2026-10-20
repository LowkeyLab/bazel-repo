#!/usr/bin/env bash
set -eu
source "$(dirname "$0")/helper.sh"
if [[ "$MODE" == yes ]] &&
	[[ "$SECOND" == yes ]]; then
	printf '%s\n' selected
else
	printf '%s\n' other
fi
bash "$(dirname "$0")/child.sh"
[[ "$helper_value" == helper ]]
printf '%s\n' FUNCTIONAL_PASS
