# Projects-to-Blog Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the personal website's projects section with ordinary chronological blog posts while permanently redirecting every former project URL.

**Architecture:** The existing Astro `blog` content collection, index, and detail route become the only publishing path for personal technical writing. A Bazel shell test treats the generated static site and Caddy configuration as public interfaces, while project-only collection, route, layout, and component code is removed.

**Tech Stack:** Astro 5 content collections, Markdown, TypeScript, Caddy, Bazel, rules_shell

**Spec:** `docs/superpowers/specs/2026-09-10-projects-to-blog-design.md`

## Global Constraints

- Use the existing ordinary blog schema; do not add project-specific blog fields.
- Use each migrated project's `startDate` as its blog `publishDate`.
- Normalize migrated tags to lowercase, hyphenated values.
- Keep `local-first-gradle-build-scan` as the only Gradle Build Scan blog entry.
- Preserve every former project URL through an exact permanent redirect; do not add a wildcard `/projects/*` redirect.
- Do not redesign the blog index, blog article page, or work section.
- Use Bazel for all build, test, lint, formatting, and generation operations.
- Run `bazel run //:gazelle` immediately after every source-file edit and before formatting.
- Run `aspect format --scope=all` before each implementation commit and before completion.

## File Structure

- Create `personal_website/site_structure_test.sh`: assert generated blog routes, chronological index order, removed project links/routes, and exact Caddy redirects.
- Create `personal_website/src/content/blog/free-dsl.md`: migrated Free-DSL article.
- Create `personal_website/src/content/blog/guess-the-word.md`: migrated Guess The Word article.
- Create `personal_website/src/content/blog/landing-page.md`: migrated Personal Landing Page article.
- Create `personal_website/src/content/blog/mindreadr.md`: migrated Mindreadr article.
- Modify `personal_website/BUILD.bazel`: expose the generated-site regression test as `//personal_website:site_structure_test`.
- Modify `personal_website/src/content.config.ts`: remove the projects collection.
- Modify `personal_website/src/components/Navbar.astro`: remove Projects navigation and its active-item variant.
- Modify `personal_website/src/components/DevHero.astro`: replace the Projects hero link with Blog.
- Modify `personal_website/src/layouts/Layout.astro`: remove `projects` from the active-item type.
- Modify `personal_website/Caddyfile`: define six exact permanent redirects.
- Delete `personal_website/src/content/projects/*.md`: remove the obsolete project records after migration.
- Delete `personal_website/src/pages/projects/index.astro`: remove the project listing and filter script.
- Delete `personal_website/src/pages/projects/[project].astro`: remove project static-path generation.
- Delete `personal_website/src/layouts/ProjectLayout.astro`: remove project-only article presentation.
- Delete `personal_website/src/components/ProjectCard.astro`: remove the unused project-only card.
- Keep `personal_website/src/content/blog/local-first-gradle-build-scan.md` unchanged.
- Keep `TagBadges.astro` and `CallToAction.astro`; the work pages still consume them.

---

### Task 1: Consolidate Project Content Into Blog

**Files:**

- Create: `personal_website/site_structure_test.sh`
- Create: `personal_website/src/content/blog/free-dsl.md`
- Create: `personal_website/src/content/blog/guess-the-word.md`
- Create: `personal_website/src/content/blog/landing-page.md`
- Create: `personal_website/src/content/blog/mindreadr.md`
- Modify: `personal_website/BUILD.bazel:1-84`
- Modify: `personal_website/src/content.config.ts:1-49`
- Delete: `personal_website/src/content/projects/free-dsl.md`
- Delete: `personal_website/src/content/projects/guess-the-word.md`
- Delete: `personal_website/src/content/projects/landing-page.md`
- Delete: `personal_website/src/content/projects/mindreadr.md`
- Delete: `personal_website/src/content/projects/gradle-build-scan-server.md`
- Delete: `personal_website/src/pages/projects/index.astro`
- Delete: `personal_website/src/pages/projects/[project].astro`
- Delete: `personal_website/src/layouts/ProjectLayout.astro`
- Delete: `personal_website/src/components/ProjectCard.astro`
- Test: `//personal_website:site_structure_test`

**Interfaces:**

- Consumes: Existing `blog` collection fields `title: string`, `description: string`, `publishDate: Date`, `tags: string[]`, and `draft: boolean`.
- Produces: Blog pages at `/blog/free-dsl`, `/blog/guess-the-word`, `/blog/landing-page`, and `/blog/mindreadr`; Bazel target `//personal_website:site_structure_test`.

- [ ] **Step 1: Write the generated-site regression test**

Create `personal_website/site_structure_test.sh` with this content:

```bash
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
```

Mark it executable:

```bash
chmod +x personal_website/site_structure_test.sh
```

- [ ] **Step 2: Regenerate BUILD metadata after adding the shell source**

Run: `bazel run //:gazelle`

Expected: PASS. Inspect the resulting diff before manually changing `personal_website/BUILD.bazel`; preserve all Gazelle changes.

- [ ] **Step 3: Register the regression test**

Add this load to `personal_website/BUILD.bazel`:

```starlark
load("@rules_shell//shell:sh_test.bzl", "sh_test")
```

Add this target after the existing `build` target:

```starlark
sh_test(
    name = "site_structure_test",
    srcs = ["site_structure_test.sh"],
    data = [
        ":build",
        "Caddyfile",
    ],
)
```

- [ ] **Step 4: Run the regression test to verify it fails**

Run: `aspect test //personal_website:site_structure_test`

Expected: FAIL with `site structure test failed: missing /blog/free-dsl`.

- [ ] **Step 5: Move the four unique project articles into the blog directory**

Run:

```bash
git mv personal_website/src/content/projects/free-dsl.md personal_website/src/content/blog/free-dsl.md
git mv personal_website/src/content/projects/guess-the-word.md personal_website/src/content/blog/guess-the-word.md
git mv personal_website/src/content/projects/landing-page.md personal_website/src/content/blog/landing-page.md
git mv personal_website/src/content/projects/mindreadr.md personal_website/src/content/blog/mindreadr.md
```

Do not move `gradle-build-scan-server.md`; the existing `local-first-gradle-build-scan.md` article is its canonical replacement.

- [ ] **Step 6: Convert each moved file to exact ordinary-blog frontmatter**

Use this frontmatter in `free-dsl.md`:

```yaml
---
title: "Free-DSL"
description: "An annotation processor for creating Builders in Kotlin"
publishDate: 2024-08-31
tags: ["kotlin"]
draft: false
---
```

Use this frontmatter in `guess-the-word.md`:

```yaml
---
title: "Guess The Word"
description: "A real-time multiplayer word guessing game"
publishDate: 2025-01-19
tags: ["node-js", "websocket", "svelte", "tailwind", "postgres"]
draft: false
---
```

Use this frontmatter in `landing-page.md`:

```yaml
---
title: "Personal Landing Page"
description: "My personal website"
publishDate: 2025-02-11
tags: ["astro", "tailwind", "deno", "typescript"]
draft: false
---
```

Use this frontmatter in `mindreadr.md`:

```yaml
---
title: "Mindreadr"
description: "A cooperative word-guessing game"
publishDate: 2025-11-19
tags: ["kotlin", "ktor", "angular", "typescript", "websocket", "bazel"]
draft: false
---
```

- [ ] **Step 7: Preserve project links in article content**

Append these exact sections to the corresponding moved files.

`free-dsl.md`:

```markdown
## Links

- [Source code](https://github.com/LowkeyLab/gradle-monorepo/tree/main/free-dsl)
```

`guess-the-word.md`:

```markdown
## Links

- [Source code](https://github.com/LowkeyLab/guess-the-word)
```

`landing-page.md`:

```markdown
## Links

- [Source code](https://github.com/LowkeyLab/deno-monorepo/tree/main/website)
- [Website](https://www.tacascer.com)
```

`mindreadr.md`:

```markdown
## Links

- [Source code](https://github.com/tacascer/bazel-repo/tree/main/mindreadr)
- [Live demo](https://mindreadrfrontend-production.up.railway.app/)
```

Change the final Guess The Word note to:

```markdown
This project has since been rewritten as [Mindreadr](/blog/mindreadr).
```

Change the Mindreadr introduction link to:

```markdown
This is a rewrite of [Guess The Word](/blog/guess-the-word), with a different tech stack and more self-hosted infrastructure.
```

Keep the remainder of both paragraphs unchanged.

- [ ] **Step 8: Remove the project collection and project-only files**

Delete the `projects` definition and the `projects` export entry from `personal_website/src/content.config.ts`, leaving `work` and `blog` unchanged. Delete:

```text
personal_website/src/content/projects/gradle-build-scan-server.md
personal_website/src/pages/projects/index.astro
personal_website/src/pages/projects/[project].astro
personal_website/src/layouts/ProjectLayout.astro
personal_website/src/components/ProjectCard.astro
```

The now-empty `src/content/projects` and `src/pages/projects` directories need not remain.

- [ ] **Step 9: Regenerate BUILD metadata immediately after source changes**

Run: `bazel run //:gazelle`

Expected: PASS. Confirm generated BUILD metadata no longer references deleted source files.

- [ ] **Step 10: Format the repository**

Run: `aspect format --scope=all`

Expected: PASS with all migrated Markdown, TypeScript, shell, and BUILD files formatted.

- [ ] **Step 11: Run the content migration test**

Run: `aspect test //personal_website:site_structure_test`

Expected: PASS, proving all expected blog pages exist, the project index is absent, and entries appear in descending publication-date order.

- [ ] **Step 12: Run Astro validation**

Run: `aspect test //personal_website:check`

Expected: PASS with zero Astro content or type errors.

- [ ] **Step 13: Commit the content migration**

```bash
git add personal_website
git commit -m "refactor(website): move projects into blog"
```

### Task 2: Remove Projects Navigation

**Files:**

- Modify: `personal_website/site_structure_test.sh`
- Modify: `personal_website/src/components/Navbar.astro:4-88`
- Modify: `personal_website/src/components/DevHero.astro:53-67`
- Modify: `personal_website/src/layouts/Layout.astro:6-16`
- Test: `//personal_website:site_structure_test`

**Interfaces:**

- Consumes: Generated static home and blog pages from `//personal_website:build`.
- Produces: Navbar active-item union `"home" | "contact" | "work" | "blog"`; home-page Blog hero link; no generated internal links to `/projects`.

- [ ] **Step 1: Extend the test with navigation assertions**

Append this block to `personal_website/site_structure_test.sh`:

```bash
if grep -R -n -E 'href="/projects(/|"|#)' "${dist}"; then
  fail "generated site still links to /projects"
fi

blog_link_count="$(grep -o 'href="/blog"' "${dist}/index.html" | wc -l)"
[[ "${blog_link_count}" -eq 3 ]] || fail "home page must contain Blog links in both navbar variants and the hero"
```

- [ ] **Step 2: Regenerate BUILD metadata immediately after the test edit**

Run: `bazel run //:gazelle`

Expected: PASS.

- [ ] **Step 3: Run the test to verify it fails**

Run: `aspect test //personal_website:site_structure_test`

Expected: FAIL with generated `/projects` links, or with a Blog link count of `2` rather than `3`.

- [ ] **Step 4: Remove Projects from the navbar**

In `Navbar.astro`, change the props type to:

```ts
interface Props {
  activeItem?: "home" | "work" | "contact" | "blog";
}
```

Delete both `<li>` elements whose anchors point to `/projects`. Keep Home, Work, and Blog in that order in both mobile and desktop navigation.

- [ ] **Step 5: Replace the home hero link**

In `DevHero.astro`, replace the `/projects` anchor with:

```astro
<a
  href="/blog"
  class="font-medium text-lg btn-link text-center sm:text-left py-2 sm:py-0 border-b sm:border-0 border-base-300"
>
  Blog &gt;
</a>
```

Keep the Experience link unchanged.

- [ ] **Step 6: Narrow the shared layout active-item type**

In `Layout.astro`, use:

```ts
interface Props {
  title: string;
  description?: string;
  activeItem?: "home" | "contact" | "work" | "blog";
}
```

- [ ] **Step 7: Regenerate BUILD metadata immediately after source changes**

Run: `bazel run //:gazelle`

Expected: PASS.

- [ ] **Step 8: Format and run the navigation regression test**

Run: `aspect format --scope=all`

Run: `aspect test //personal_website:site_structure_test`

Expected: Both commands PASS; generated output has no `/projects` links and exactly three home-page `/blog` links.

- [ ] **Step 9: Commit the navigation change**

```bash
git add personal_website
git commit -m "refactor(website): remove projects navigation"
```

### Task 3: Permanently Redirect Former Project URLs

**Files:**

- Modify: `personal_website/site_structure_test.sh`
- Modify: `personal_website/Caddyfile:1-5`
- Test: `//personal_website:site_structure_test`

**Interfaces:**

- Consumes: Caddy's `redir <matcher> <target> permanent` directive.
- Produces: Exact permanent redirects from six former project URLs to canonical blog URLs; unknown project URLs remain unmatched.

- [ ] **Step 1: Extend the test with exact redirect assertions**

Append this block to `personal_website/site_structure_test.sh`:

```bash
readonly caddyfile="${TEST_SRCDIR}/${TEST_WORKSPACE}/personal_website/Caddyfile"

assert_redirect() {
  local source="$1"
  local destination="$2"
  grep -Eq "^[[:space:]]*redir[[:space:]]+${source}[[:space:]]+${destination}[[:space:]]+permanent[[:space:]]*$" "${caddyfile}" ||
    fail "missing permanent redirect from ${source} to ${destination}"
}

assert_redirect "/projects" "/blog"
assert_redirect "/projects/guess-the-word" "/blog/guess-the-word"
assert_redirect "/projects/free-dsl" "/blog/free-dsl"
assert_redirect "/projects/landing-page" "/blog/landing-page"
assert_redirect "/projects/mindreadr" "/blog/mindreadr"
assert_redirect "/projects/gradle-build-scan-server" "/blog/local-first-gradle-build-scan"

if grep -Eq '^[[:space:]]*redir[[:space:]]+/projects/\*' "${caddyfile}"; then
  fail "wildcard project redirect would hide unknown routes"
fi
```

- [ ] **Step 2: Regenerate BUILD metadata immediately after the test edit**

Run: `bazel run //:gazelle`

Expected: PASS.

- [ ] **Step 3: Run the test to verify it fails**

Run: `aspect test //personal_website:site_structure_test`

Expected: FAIL with `missing permanent redirect from /projects to /blog`.

- [ ] **Step 4: Add exact Caddy redirect directives**

Replace `personal_website/Caddyfile` with:

```caddyfile
:80 {
	log
	root * /app

	redir /projects /blog permanent
	redir /projects/guess-the-word /blog/guess-the-word permanent
	redir /projects/free-dsl /blog/free-dsl permanent
	redir /projects/landing-page /blog/landing-page permanent
	redir /projects/mindreadr /blog/mindreadr permanent
	redir /projects/gradle-build-scan-server /blog/local-first-gradle-build-scan permanent

	file_server
}
```

- [ ] **Step 5: Format and run the redirect regression test**

Run: `aspect format --scope=all`

Run: `aspect test //personal_website:site_structure_test`

Expected: Both commands PASS. The test confirms all six exact declarations and rejects a wildcard declaration.

- [ ] **Step 6: Build the deployment image**

Run: `aspect build //personal_website:image`

Expected: PASS, proving the updated Caddyfile and generated site package into the production image.

- [ ] **Step 7: Commit the redirect configuration**

```bash
git add personal_website/Caddyfile personal_website/site_structure_test.sh
git commit -m "fix(website): redirect project URLs to blog"
```

### Task 4: Verify The Complete Refactor

**Files:**

- Verify: `personal_website/**`
- Test: `//personal_website:site_structure_test`
- Test: `//personal_website:check`

**Interfaces:**

- Consumes: Completed content migration, navigation cleanup, and redirect configuration.
- Produces: Verification evidence for generated content, responsive navigation, lint, targeted artifacts, and the repository-wide build.

- [ ] **Step 1: Confirm obsolete references are gone**

Run:

```bash
rg -n 'getCollection\("projects"\)|activeItem="projects"|href="/projects|/projects/' personal_website/src
```

Expected: No matches.

- [ ] **Step 2: Run the required generation and formatting workflow**

Run: `bazel run //:gazelle`

Run: `aspect format --scope=all`

Expected: Both commands PASS and leave no formatting diff.

- [ ] **Step 3: Run targeted tests and lint**

Run: `aspect test //personal_website:site_structure_test //personal_website:check`

Run: `aspect lint //personal_website:...`

Expected: All tests and lint checks PASS with zero failures.

- [ ] **Step 4: Run targeted production builds**

Run: `aspect build //personal_website:build //personal_website:image`

Expected: Both static-site and OCI image targets build successfully.

- [ ] **Step 5: Smoke-test desktop navigation**

Start the development server with `ibazel run //personal_website:dev`. Using the `agent-browser-frontend-testing` skill, open `http://localhost:4321/` at a desktop viewport of 1440 by 900 and verify Home, Work, and Blog are visible in the navbar; Projects is absent; the hero contains Blog and Experience links; and the Blog link opens `/blog` with the six entries in the expected chronology.

Expected: All checks pass without browser console errors.

- [ ] **Step 6: Smoke-test mobile navigation**

At a mobile viewport of 390 by 844, open the hamburger menu and verify it contains Home, Work, and Blog but not Projects. Select Blog and open one migrated entry.

Expected: Navigation reaches the blog index and migrated article without overflow or broken layout.

- [ ] **Step 7: Run the repository-wide build**

Run: `aspect build //...`

Expected: PASS with no build failures.

- [ ] **Step 8: Inspect the final worktree**

Run:

```bash
git status --short
git diff --check
git log --oneline -5
```

Expected: The worktree contains no uncommitted implementation changes, `git diff --check` reports no errors, and the three implementation commits are visible.
