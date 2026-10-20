#!/usr/bin/env bash
printf '%s\n' disconnected
if [[ "$UNUSED" == yes ]]; then
	printf '%s\n' never
fi
