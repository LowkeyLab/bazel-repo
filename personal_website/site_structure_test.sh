#!/usr/bin/env bash
set -euo pipefail

readonly dist="${TEST_SRCDIR}/${TEST_WORKSPACE}/personal_website/dist"

fail() {
	printf 'site structure test failed: %s\n' "$1" >&2
	exit 1
}

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
mindreadr_card="${remaining#*'href="/blog/mindreadr"'}"
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
featured="${home#*'id="featured-heading"'}"
featured="${featured%%'id="recent-heading"'*}"

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
	[[ "${featured}" == *"href=\"/blog/${slug}\""* ]] || fail "missing featured article ${slug}"
done

if [[ "${home}" == *'id="recent-heading"'* ]]; then
	recent="${home#*'id="recent-heading"'}"
	[[ "${recent}" == *'All writing'* ]] || fail "missing All writing link"
	recent_before_all="${recent%%'All writing'*}"
	recent_count="$(count_entry_links "${recent_before_all}")"
	[[ "${recent_count}" -ge 1 && "${recent_count}" -le 3 ]] || fail "recent section must contain one to three entries before All writing"
	for slug in building-jvm-rpc-tooling mindreadr prototyping-a-flink-pipeline; do
		[[ "${recent}" != *"href=\"/blog/${slug}\""* ]] || fail "featured article ${slug} repeated in recent writing"
	done
	[[ "${recent_before_all}" == *'href="/blog/"'* ]] || fail "All writing must link to Blog after recent entries"
	[[ "${recent_before_all}" == *'</section><a href="/blog/"'* ]] || fail "All writing must appear below the recent list"
else
	[[ "${home}" != *'All writing'* ]] || fail "All writing must be omitted when there are no recent entries"
	[[ "${home}" != *'aria-label="Recent writing"'* ]] || fail "empty Recent writing list must be omitted"
fi

readonly caddyfile="${TEST_SRCDIR}/${TEST_WORKSPACE}/personal_website/Caddyfile"

assert_redirect() {
	local source="$1"
	local destination="$2"
	grep -Eq "^[[:space:]]*redir[[:space:]]+${source}[[:space:]]+${destination}[[:space:]]+permanent[[:space:]]*$" "${caddyfile}" ||
		fail "missing permanent redirect from ${source} to ${destination}"
}

assert_redirect "/projects" "/blog/"
assert_redirect "/projects/" "/blog/"
assert_redirect "/projects/guess-the-word" "/blog/guess-the-word/"
assert_redirect "/projects/guess-the-word/" "/blog/guess-the-word/"
assert_redirect "/projects/free-dsl" "/blog/free-dsl/"
assert_redirect "/projects/free-dsl/" "/blog/free-dsl/"
assert_redirect "/projects/landing-page" "/blog/landing-page/"
assert_redirect "/projects/landing-page/" "/blog/landing-page/"
assert_redirect "/projects/mindreadr" "/blog/mindreadr/"
assert_redirect "/projects/mindreadr/" "/blog/mindreadr/"
assert_redirect "/projects/gradle-build-scan-server" "/blog/local-first-gradle-build-scan/"
assert_redirect "/projects/gradle-build-scan-server/" "/blog/local-first-gradle-build-scan/"

project_redirect_count="$(grep -Ec '^[[:space:]]*redir[[:space:]]+/projects(/[^[:space:]]*)?[[:space:]]+' "${caddyfile}")"
[[ "${project_redirect_count}" -eq 12 ]] || fail "expected exactly 12 approved project redirect matchers, found ${project_redirect_count}"

if grep -Eq '^[[:space:]]*redir[[:space:]]+/projects/\*' "${caddyfile}"; then
	fail "wildcard project redirect would hide unknown routes"
fi

[[ -f "${dist}/about/index.html" ]] || fail "missing About page"
for slug in building-jvm-rpc-tooling prototyping-a-flink-pipeline making-a-platform-buildable-again; do
	article="$(<"${dist}/blog/${slug}/index.html")"
	[[ "${article}" == *'entry-type">Work</span>'* ]] || fail "missing Work label for ${slug}"
	[[ "${article}" == *'Bloomberg'* ]] || fail "missing company context for ${slug}"
	[[ "${article}" == *'October 1, 2026'* ]] || fail "wrong publication date for ${slug}"
done
