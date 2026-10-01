# Unified Personal Site Publishing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish work, projects, essays, and notes through one blog while preserving the site's personality and existing URLs.

**Architecture:** Extend the existing Astro blog collection and reuse its article presentation. Shared selection of published posts drives listings, Home, RSS, and sitemap; Caddy handles retired URLs. Home introduces current interests while About tells the approved personal journey.

**Tech Stack:** Astro, TypeScript, Markdown, existing CSS/Tailwind, Caddy, Go HTTP integration tests, Bazel/Aspect through Nix.

**Spec:** `docs/superpowers/specs/2026-10-01-personal-site-content-design.md`. Read the entire spec, including approved copy and interview evidence, before execution.

## Global Constraints

- Do not use em dashes in new site copy.
- Do not add work periods, employment dates, roles, or a technology inventory.
- Preserve all existing blog URLs, publication dates, accepted essay copy, and Rust draft status.
- Keep RPC tooling in limited beta, moving toward production; do not invent performance measurements or verified cross-language parity.
- IOI prototype work occurred in 2022; its publication date is a separate concept.
- Home and About copy must match the approved spec exactly. About uses Electrical and Computer Engineering, Computer Science, and first full-time software job, with no Cornell or Bloomberg names in its prose.
- No filtering, search, new design system, resume creation, or lesson component.
- Use Bazel/Aspect, never direct cargo/npm/pnpm/go commands. Run `nix develop --command bazel run //:gazelle` immediately after each source-edit batch and before formatting or manual BUILD changes.
- Run `nix develop --command aspect format --scope=all` before each commit. Stage only intended files, excluding `.superpowers/` preview artifacts.
- Reuse the current worktree; account for its detached HEAD before implementation commits by creating an appropriate `codex/` branch. Do not create or publish a PR unless requested.

## Review Focus

1. Draft articles must remain directly accessible but stay out of public discovery; test all four discovery surfaces and draft noindex in Task 5.
2. Equal publication dates must produce deterministic ordering, and featured posts must not reappear under Recent writing; test selection edge cases in Task 1 and rendered sections in Task 4.
3. Old URL slash variants must redirect successfully without swallowing unknown paths; exercise actual Caddy responses in Task 3.
4. Titles with ampersands or quotes must remain valid in XML, HTML metadata, and JSON-LD; parse emitted formats using an adversarial fixture in Task 5.
5. Long titles and tags must remain usable on mobile and in both themes; inspect rendered pages at 390px and desktop widths in Task 6.

## File and interface map

- Modify `personal_website/src/content.config.ts`: unified metadata and eventual removal of work collection.
- Extend `personal_website/src/utils/blog.ts`: retain calculateReadingTime; export `publishedPosts(posts: CollectionEntry<"blog">[]): CollectionEntry<"blog">[]` and `homePosts(posts: CollectionEntry<"blog">[]): { featured: CollectionEntry<"blog">[]; recent: CollectionEntry<"blog">[] }`.
- Create `personal_website/src/components/PostList.astro`: `Props { posts: CollectionEntry<"blog">[] }`, reusable semantic list, exact title/description/date/type/tags presentation.
- Modify `personal_website/src/pages/blog/index.astro` and `personal_website/src/pages/blog/[slug].astro`: public listing and unified article rendering.
- Move three work Markdown files into blog under the exact destination slugs below. Update existing project metadata and framing in `personal_website/src/content/blog/*.md`.
- Modify `personal_website/src/pages/index.astro`, `src/components/DevHero.astro`, `src/components/Navbar.astro`, `src/layouts/Layout.astro`, and `src/styles/global.css`; create `src/pages/about.astro`.
- Create `personal_website/src/pages/rss.xml.ts`, `src/pages/sitemap.xml.ts`, and `src/utils/seo.ts`; modify `astro.config.ts` and `public/robots.txt`.
- Modify `personal_website/Caddyfile`, `site_structure_test.sh`, and `cache_headers_test.go`; create `personal_website/publishing_test.go` for parsed rendered-artifact assertions using Go standard library JSON/XML parsing and the existing archive/server harness.
- Update generated BUILD files through Gazelle. Preserve existing `# keep` test data and runtime tool bindings.

## Task 1: Unified metadata and published-post selection

**Interfaces:** Produces the two blog utility functions and collection fields consumed by Tasks 2, 4, and 5. `publishedPosts` filters drafts and sorts descending by publishDate, then ascending by id for ties. `homePosts` takes the first three featured entries in that order, then up to three non-featured entries for recent writing.

- [ ] Add `type: z.enum(["work", "project", "essay", "note"])`, `featured: z.boolean().default(false)`, and `context: z.string().optional()` to blog schema. Keep existing metadata. Explicitly assign project to the five project posts and essay to the two essays; retain all dates and draft flags. Retain the work collection until Task 3.
- [ ] Implement the selection functions in `src/utils/blog.ts`. Have the blog index use `publishedPosts` immediately so rendered behavior exercises the helper.
- [ ] Validate through a temporary content fixture batch: draft marked featured, four published featured items, two same-date entries, and no remaining recent entries. Assert draft exclusion, maximum three featured, stable id tie order, and zero recent when all published posts are featured. Use a Bazel-built rendered page or the repository's existing TypeScript test convention if one exists; do not add a new test framework solely for these helpers. Remove temporary fixtures after the assertions.
- [ ] Run `nix develop --command aspect test //personal_website:check //personal_website:site_structure_test`. Expected: both pass and intended tests execute. Format and commit as `feat(website): unify publishing metadata`.

## Task 2: Shared article and list presentation

**Interfaces:** Consumes blog collection metadata and `publishedPosts`; produces `PostList.astro` for Home and Blog. Article routes continue to include directly accessible drafts.

- [ ] Extend `site_structure_test.sh` to assert a Project label on Mindreadr, title/description/date/tags in the list, and the Rust draft absent from the listing but still directly generated. Run the focused structure target to demonstrate failure for the missing type labels.
- [ ] Implement `PostList.astro` using the existing entry-list/entry-link styles. Show date, subdued type, description, reading time, and tags without company information. Reuse it in the blog index.
- [ ] Update `[slug].astro` to show type and optional context beneath the title with publication date. Preserve the existing prose layout. Avoid a special work template and do not add work periods.
- [ ] Run Gazelle, format, and the focused check/structure targets. Expected: new rendered assertions and existing URL checks pass. Commit as `feat(website): share post presentation across content types`.

## Task 3: Work stories, project framing, and permanent redirects

**Interfaces:** Produces the three published work entries, with `context: Bloomberg`, and removes the work architecture. Caddy maps both slash variants of each retired route to a canonical blog page.

| Source work slug          | Destination blog slug             |
| ------------------------- | --------------------------------- |
| core-java-infrastructure  | building-jvm-rpc-tooling          |
| ioi-pipeline              | prototyping-a-flink-pipeline      |
| model-driven-architecture | making-a-platform-buildable-again |

- [ ] Add `TestLegacyRedirects` in `cache_headers_test.go`, reusing `startCaddy`. Disable redirect following for the first response; assert HTTP 301 and Location for all eight Work source variants. Follow each destination and assert 200. Cover all twelve existing Projects matchers and assert `/work/unknown` and `/projects/unknown` remain 404. Run `nix develop --command aspect test //personal_website:personal_website_test`; expect new Work cases to fail before implementation.
- [ ] Move and rewrite the three work articles from the spec's interview evidence. Explicitly state the IOI prototype occurred in 2022. Use the actual publication date at execution, never the preview's illustrative date or old employment dates. Preserve beta versus production distinctions and team versus individual attribution.
- [ ] Improve project framing where README-like, retaining useful technical detail and URLs. Preserve existing essay bodies. Trim tags to useful topics without generating new taxonomy routes. Set featured on the RPC story, Mindreadr, and IOI story only.
- [ ] Add explicit Caddy redirects from `/work` and `/work/` to `/blog/` and six article variants to their blog URLs. Preserve old Projects rules and cache handlers. Delete work collection, old work pages, and skills inventory. Replace Work navigation with Blog/About/GitHub; add an initial About route using approved copy so navigation has no dead destination.
- [ ] Update `site_structure_test.sh` to expect the new articles, no generated Work pages, no links to retired paths, and no homepage Work link. Run Gazelle, format, both structure/check tests and the HTTP test target. Expected: all pass, including cache/revalidation tests. Commit as `feat(website): migrate work stories and preserve legacy URLs`.

## Task 4: Cohesive Home and About

**Interfaces:** Consumes `homePosts` and `PostList`. Layout/Navbar activeItem supports home, blog, and about; GitHub is external. Approved prose comes verbatim from the spec.

- [ ] Extend rendered structure checks to require three featured links, Recent writing after the curated section, and All writing after the last recent entry. Assert no duplicate article links between the two sections. Run the structure target and confirm missing-section failures.
- [ ] Shorten the homepage hero description to the approved text and add `More about me →` linking `/about/`. Preserve hero headline, existing illustration, portrait, contact action, and social links. Add Things I've worked on and Recent writing using the selection helpers; omit Recent writing when empty. Put `All writing →` below the last recent entry.
- [ ] Finish `about.astro` with the four exact approved paragraphs. No contact section, generic final blog CTA, employer-specific chronology, or em dashes. Retain the 2023 website origin sentence: `This blog, with its many iterations, began then.`
- [ ] Adjust existing styles only for spacing, section headings, and small type/context labels. Remove skills-specific CSS and components only after checking remaining callers.
- [ ] Run Gazelle, format, and focused website checks. Compare normalized rendered Home/About text to the approved paragraphs and check navigation destinations. Commit as `feat(website): connect a concise home to a personal About page`.

## Task 5: Feeds and page metadata

**Interfaces:** `src/utils/seo.ts` exports `canonicalUrl(path: string, site: URL): string` and `escapeXml(value: string): string`. Page routes use Astro's configured site origin. Layout gains optional `canonicalPath?: string`, `noindex?: boolean`, and `article?: { title: string; description: string; publishDate: Date }` props. Default canonical path is Astro.url.pathname, without query or fragment.

- [ ] Verify the production origin from deployment configuration or the live domain's redirect behavior before setting `site` in `astro.config.ts`. The current robots comment alone is insufficient. If deployment evidence is unavailable, ask for the canonical domain while continuing other tasks. Use one verified HTTPS origin consistently and trailing slashes for HTML routes; preserve `.xml` endpoint paths.
- [ ] Add parsed-artifact tests in `publishing_test.go`: `TestPublishedDiscovery` checks RSS/sitemap membership equals published routes and excludes the draft and retired Work URLs; `TestPageMetadata` checks canonical URL, OG title/description/type/url and parsed BlogPosting fields; `TestDraftNoindex` checks direct draft access and noindex. Reuse archive extraction from the existing HTTP harness, factoring a helper only if needed. Run the HTTP test target and confirm failures for absent feeds/metadata.
- [ ] Implement static `GET: APIRoute` endpoints for `/rss.xml` and `/sitemap.xml` using `publishedPosts`. RSS includes title, description, canonical link, stable GUID, and UTC publication date. Sitemap includes Home, Blog, About, and published articles. Escape XML text/attributes, set correct XML content types, and add no runtime service or new feed dependency.
- [ ] Implement shared canonical/OG tags and published-article BlogPosting JSON-LD in Layout. Serialize JSON safely so article text cannot close the script element. Set draft noindex and omit published BlogPosting markup for drafts. Add feed discovery and robots sitemap declaration. Reuse a suitable public raster image only if present; otherwise omit OG image instead of inventing a new asset.
- [ ] Temporarily add an adversarial published fixture with ampersands, quotes, and a closing-script sequence in metadata. Build through Bazel and assert XML/JSON parsers recover the original strings and rendered HTML contains no injected executable element. Remove the fixture and rebuild the actual site.
- [ ] Run Gazelle, format, focused check/structure/HTTP targets. Confirm feeds carry all three migrated stories, timestamps represent publication dates, and cache tests remain passing for new files. Commit as `feat(website): publish feeds and canonical article metadata`.

## Task 6: Final rendered verification and cleanup

**Interfaces:** Validates the complete production build, not the standalone mockup.

- [ ] Build through Bazel and inspect actual output or `bazel-bin/personal_website/frontend_layer.tar`. Check internal root-relative links resolve to generated files or explicit redirects. Add a rendered-artifact check to `publishing_test.go` if existing checks do not cover link resolution; do not require outbound-network checks of every external link.
- [ ] Preview Home, Blog, all three migrated articles, and About at 390px and desktop widths in light/dark themes. Verify no horizontal overflow, readable long titles/tags, visible keyboard focus, correct navigation, no work periods, and no About contact section. Check rendering with font fallback as well as Caveat.
- [ ] Audit copied text against the spec, verify new prose contains no em dashes, and check exact old URL mappings and draft behavior. Remove dead Work imports/styles and confirm shared components retain callers.
- [ ] Run `nix develop --command aspect format --scope=all`, `nix develop --command aspect test //personal_website:check //personal_website:site_structure_test //personal_website:personal_website_test`, `nix develop --command aspect build //...`, `nix develop --command aspect lint`, and `git diff --check`. Expected: successful exits and actual focused test execution; report warnings and infrastructure limits precisely.
- [ ] Commit any final corrections after rerunning affected checks. Obtain the execution method's required review and address verified findings. Report concrete validation and remaining limitations without claiming deployment or publication.

## Self-review and handoff

Coverage checked: metadata, story evidence, project preservation, legacy routes, Home/About copy and cohesion, placement of All writing, no work periods/em dashes, draft handling, feeds, metadata, visual identity, cleanup, and repository checks each map to a task above. Shared signatures and field names are consistent across tasks. Production origin is an explicit verification prerequisite, not an invented constant.

Recommended execution: Native, because these six tasks share a small collection and rendering surface and are easiest to validate together. Review this plan and choose Native or Subagent-driven before implementation begins.
