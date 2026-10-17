#!/usr/bin/env bash
# Disposable experiment: patched AXL builtins with the repository's pinned CLI.
set -euo pipefail
umask 077
prototype_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
export ASPECT_CLI_CACHE="${TY_PROTOTYPE_ASPECT_CACHE:-$(mktemp -d "${TMPDIR:-/tmp}/ty-aspect-prototype.XXXXXXXX")}"
aspect --help >/dev/null
builtins=("$ASPECT_CLI_CACHE"/axl/builtins/*/aspect/lint.axl)
if [[ ${#builtins[@]} != 1 || ! -f ${builtins[0]} ]]; then
	echo 'Expected exactly one extracted Aspect lint module' >&2
	exit 1
fi
if ! grep -q 'Disposable prototype: ty is unconditional' "${builtins[0]}"; then
	patch --silent --strip=1 --directory="$(dirname -- "${builtins[0]}")" <"$prototype_dir/aspect-ty-unconditional.patch"
fi
exec aspect "$@"
