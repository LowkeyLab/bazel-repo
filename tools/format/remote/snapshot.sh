#!/usr/bin/env bash
# Package working-tree bytes, not HEAD or the staged blobs.
set -euo pipefail
[[ -z $(git rev-parse --show-prefix) ]]
git ls-files -z -t --cached --others --exclude-standard | {
	while IFS= read -r -d '' record; do
		[[ ${record:0:2} != 'S ' ]] || continue
		path=${record:2}
		# Inspect each prefix from the root, never stat through a link.
		prefix=
		rest=$path
		while [[ $rest == */* ]]; do
			prefix+=${rest%%/*}/
			[[ ! -L ${prefix%/} ]] || continue 2
			rest=${rest#*/}
		done
		[[ ! -L $path ]] || continue
		if [[ ${record:0:2} == '? ' ]]; then
			printf 'Unapproved untracked input (review and stage or exclude): %q\n' "$path" >&2
			exit 1
		fi
		[[ -e $path ]] || continue
		if [[ ! -f $path ]]; then
			printf 'Unsupported formatter input: %q\n' "$path" >&2
			exit 1
		fi
		printf '%s\0' "$path"
	done
} | LC_ALL=C sort -zu | tar --create --file="$1" --no-recursion --hard-dereference --null --files-from=-
