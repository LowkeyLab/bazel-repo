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

if grep -R -n -E 'href="/projects(/|"|#)' "${dist}"; then
	fail "generated site still links to /projects"
fi

blog_link_count="$(grep -o 'href="/blog"' "${dist}/index.html" | wc -l)"
[[ "${blog_link_count}" -eq 3 ]] || fail "home page must contain Blog links in both navbar variants and the hero"
