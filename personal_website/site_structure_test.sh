#!/usr/bin/env bash
set -euo pipefail

readonly dist="${TEST_SRCDIR}/${TEST_WORKSPACE}/personal_website/dist"

fail() {
	printf 'site structure test failed: %s\n' "$1" >&2
	exit 1
}

for slug in free-dsl guess-the-word landing-page mindreadr local-first-gradle-build-scan; do
	[[ -f "${dist}/blog/${slug}/index.html" ]] || fail "missing /blog/${slug}"
done

[[ ! -e "${dist}/projects/index.html" ]] || fail "generated obsolete /projects route"

remaining="$(<"${dist}/blog/index.html")"
for title in \
	"Local-First Gradle Build Scan" \
	"Hello World" \
	"Mindreadr" \
	"Personal Landing Page" \
	"Guess The Word" \
	"Free-DSL"; do
	next="${remaining#*"${title}"}"
	[[ "${next}" != "${remaining}" ]] || fail "missing or misordered blog entry: ${title}"
	remaining="${next}"
done
