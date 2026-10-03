#!/usr/bin/env bash
set -euo pipefail

readonly dist="${TEST_SRCDIR}/${TEST_WORKSPACE}/personal_website/dist"
readonly site_archive="${TEST_SRCDIR}/${TEST_WORKSPACE}/personal_website/frontend_layer.tar"

fail() {
	printf 'site structure test failed: %s\n' "$1" >&2
	exit 1
}

# Rendering must not depend on third-party font services.
if grep -R -l -E 'fonts\.googleapis\.com|fonts\.gstatic\.com' "${dist}" --include='*.html' --include='*.css'; then
	fail "published pages still require Google Fonts"
fi
grep -R -q 'data:font/woff2;base64,' "${dist}/_astro" --include='*.css' || fail "missing embedded production font"

# Browser routes must stay out of the published site.
[[ ! -e "${dist}/tests" ]] || fail "browser fixture route entered production output"
[[ -f "${dist}/sitemap.xml" ]] || fail "missing sitemap"
if grep -q '/tests/' "${dist}/sitemap.xml"; then
	fail "browser fixture route entered sitemap"
fi
[[ -f "${dist}/caveat-OFL.txt" ]] || fail "missing bundled font license"
archive_entries="$(tar -tf "${site_archive}")"
if grep -Eq '(^|/)tests/' <<<"${archive_entries}"; then
	fail "browser fixture entered frontend layer"
fi

for slug in free-dsl guess-the-word landing-page mindreadr local-first-gradle-build-scan building-jvm-rpc-tooling prototyping-a-flink-pipeline making-a-platform-buildable-again; do
	[[ -f "${dist}/blog/${slug}/index.html" ]] || fail "missing /blog/${slug}"
done

for route in \
	work \
	work/core-java-infrastructure \
	work/ioi-pipeline \
	work/model-driven-architecture \
	projects \
	projects/guess-the-word \
	projects/free-dsl \
	projects/landing-page \
	projects/mindreadr \
	projects/gradle-build-scan-server; do
	[[ ! -e "${dist}/${route}/index.html" ]] || fail "generated obsolete /${route} route"
done

remaining="$(<"${dist}/blog/index.html")"
[[ -f "${dist}/blog/rust-is-a-mind-virus/index.html" ]] || fail "missing directly accessible Rust draft"
[[ "${remaining}" != *"Rust Is a Mind Virus."* ]] || fail "Rust draft appeared in blog listing"
mindreadr_card="${remaining#*'href="/blog/mindreadr/"'}"
[[ "${mindreadr_card}" != "${remaining}" ]] || fail "missing Mindreadr card"
mindreadr_card="${mindreadr_card%%'</a>'*}"
for detail in \
	"A cooperative word-guessing game" \
	"November 19, 2025" \
	"kotlin" \
	"min read"; do
	[[ "${mindreadr_card}" == *"${detail}"* ]] || fail "missing Mindreadr card detail: ${detail}"
done
[[ "${mindreadr_card}" == *'entry-type">Project</span>'* ]] || fail "missing Project label in Mindreadr card"
mindreadr="$(<"${dist}/blog/mindreadr/index.html")"
[[ "${mindreadr}" == *'entry-type">Project</span>'* ]] || fail "missing Project label on Mindreadr article"
for title in \
	"Local-First Gradle Build Scan" \
	"Mindreadr" \
	"Personal Landing Page" \
	"Guess The Word" \
	"Free-DSL"; do
	next="${remaining#*"${title}"}"
	[[ "${next}" != "${remaining}" ]] || fail "missing or misordered blog entry: ${title}"
	remaining="${next}"
done

if grep -R -n -E 'href="/(projects|work)(/|"|#)' "${dist}"; then
	fail "generated site still links to retired paths"
fi

for destination in /blog/ /about/; do
	grep -q "href=\"${destination}\"" "${dist}/index.html" || fail "home page must link to ${destination}"
done

home="$(<"${dist}/index.html")"
[[ "${home}" == *'id="featured-heading"'* ]] || fail "missing Things I've worked on section"
[[ "${home}" == *'id="recent-heading"'* ]] || fail "missing Recent writing section for current content fixture"
featured="${home#*'id="featured-heading"'}"
featured="${featured%%'id="recent-heading"'*}"
recent="${home#*'id="recent-heading"'}"
[[ "${recent}" == *'All writing'* ]] || fail "missing All writing link"
recent_before_all="${recent%%'All writing'*}"

count_entry_links() {
	local content="$1"
	local count=0
	while [[ "${content}" == *'class="entry-link"'* ]]; do
		content="${content#*'class="entry-link"'}"
		((count += 1))
	done
	printf '%s' "${count}"
}

[[ "$(count_entry_links "${featured}")" -eq 3 ]] || fail "featured section must contain three article links"
for slug in building-jvm-rpc-tooling mindreadr prototyping-a-flink-pipeline; do
	[[ "${featured}" == *"href=\"/blog/${slug}/\""* ]] || fail "missing featured article ${slug}"
done

recent_count="$(count_entry_links "${recent_before_all}")"
[[ "${recent_count}" -ge 1 && "${recent_count}" -le 3 ]] || fail "recent section must contain one to three entries before All writing"
for slug in building-jvm-rpc-tooling mindreadr prototyping-a-flink-pipeline; do
	[[ "${recent}" != *"href=\"/blog/${slug}/\""* ]] || fail "featured article ${slug} repeated in recent writing"
done
[[ -f "${dist}/about/index.html" ]] || fail "missing About page"
for slug in building-jvm-rpc-tooling prototyping-a-flink-pipeline making-a-platform-buildable-again; do
	article="$(<"${dist}/blog/${slug}/index.html")"
	[[ "${article}" == *'entry-type">Work</span>'* ]] || fail "missing Work label for ${slug}"
	[[ "${article}" == *'Bloomberg'* ]] || fail "missing company context for ${slug}"
	[[ "${article}" == *'October 1, 2026'* ]] || fail "wrong publication date for ${slug}"
done
