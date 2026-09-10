# Projects-to-Blog Refactor Design

## Goal

Remove the personal website's separate projects section and publish each project as an ordinary, chronologically ordered blog entry. The blog becomes the sole collection and route family for personal technical writing while existing project URLs continue to resolve through permanent redirects.

## Current State

The Astro site has separate `projects` and `blog` content collections, index pages, detail routes, and article layouts. Five project records and two blog posts exist. Gradle Build Scan Server is represented in both collections, with the blog version already serving as the longer canonical article.

Project navigation also appears in the desktop and mobile navbar and in the home-page hero. Two project articles link to other `/projects/...` pages.

## Chosen Approach

Use one ordinary blog collection and one blog presentation. Migrate the four projects without existing blog equivalents into `src/content/blog`, retain the existing Gradle Build Scan blog post as its canonical entry, and delete project-only content infrastructure.

This is preferred over combining two collections at render time because it removes the obsolete content model rather than hiding it. It is preferred over retaining redirect-only Astro pages because redirects belong in deployment configuration and should not preserve unused page implementations.

## Content Migration

Each migrated project becomes an ordinary blog post using the existing blog schema:

- `title` and `description` retain their current meanings.
- `publishDate` is copied from the project's `startDate`.
- `draft` is set to `false`.
- Tags are normalized to lowercase, hyphenated values.
- Project-specific `startDate`, `endDate`, `featured`, and `links` fields are removed.
- Repository, demo, and website links are represented in a short Markdown `Links` section when applicable.
- The existing Markdown body is retained, with internal project links updated to blog routes.

The migration mapping is:

| Former project           | Blog slug                       | Publish date | Tags                                                            |
| ------------------------ | ------------------------------- | ------------ | --------------------------------------------------------------- |
| Free-DSL                 | `free-dsl`                      | 2024-08-31   | `kotlin`                                                        |
| Guess The Word           | `guess-the-word`                | 2025-01-19   | `node-js`, `websocket`, `svelte`, `tailwind`, `postgres`        |
| Personal Landing Page    | `landing-page`                  | 2025-02-11   | `astro`, `tailwind`, `deno`, `typescript`                       |
| Mindreadr                | `mindreadr`                     | 2025-11-19   | `kotlin`, `ktor`, `angular`, `typescript`, `websocket`, `bazel` |
| Gradle Build Scan Server | `local-first-gradle-build-scan` | 2026-05-27   | Keep the existing blog post and tags unchanged                  |

The blog index continues sorting all non-draft posts by descending `publishDate`. No project category, project card variant, date range, or project-specific filtering is introduced.

## Routes And Redirects

The canonical destinations are `/blog` and `/blog/<slug>`. Production Caddy configuration issues permanent redirects before static file handling:

| Former URL                           | Destination                           |
| ------------------------------------ | ------------------------------------- |
| `/projects`                          | `/blog`                               |
| `/projects/guess-the-word`           | `/blog/guess-the-word`                |
| `/projects/free-dsl`                 | `/blog/free-dsl`                      |
| `/projects/landing-page`             | `/blog/landing-page`                  |
| `/projects/mindreadr`                | `/blog/mindreadr`                     |
| `/projects/gradle-build-scan-server` | `/blog/local-first-gradle-build-scan` |

Redirects are exact rather than wildcard mappings. Unknown `/projects/*` paths retain normal not-found behavior. Migrated Markdown cross-links point directly to canonical blog URLs rather than relying on redirects.

## Navigation And Presentation

Remove Projects from both navbar variants and from the navbar active-item type. Change the home hero's Projects link to Blog while retaining the Experience link. Migrated entries use the existing blog index cards and blog article page without project-specific visual treatment. This keeps one consistent reading experience and avoids extending the blog schema for metadata that can live in article content.

The existing Blog navigation item is active on all migrated article pages. Work pages and their shared components remain unchanged.

## Code Removal

Remove:

- The `projects` collection definition and export.
- `src/content/projects/` after migration.
- The `/projects` index and detail page implementations.
- `ProjectLayout.astro`.
- `ProjectCard.astro`, which has no current consumer outside the obsolete project feature.

Keep shared components such as `TagBadges` and `CallToAction` because work pages still use them.

## Error Handling

This remains a static site with no new runtime data flow. Astro content schema validation fails the build for invalid migrated frontmatter. Static path generation fails for duplicate or invalid blog entries. Exact redirect rules avoid accidentally redirecting unknown paths.

## Verification

Verification must establish that:

- Astro accepts every migrated post under the ordinary blog schema.
- `/blog` contains all four migrated projects plus the existing posts in descending `publishDate` order.
- Each expected `/blog/<slug>` page is generated.
- Caddy has an exact permanent redirect for the project index and every former project detail URL.
- No internal navigation or Markdown link still targets `/projects`.
- The desktop and mobile navbar and home hero expose Blog rather than Projects.
- Work pages still build after project-only code is removed.

After source edits, run the repository-required workflow in order: `bazel run //:gazelle`, `aspect format --scope=all`, targeted personal website checks, and `aspect build //...`. Use `aspect lint //personal_website:...` for the targeted lint check and `aspect build //personal_website:build` for the targeted build.

## Out Of Scope

- Blog tag filtering or category pages.
- A redesigned blog index or article layout.
- New project-specific fields in the blog schema.
- Changes to the separate work/experience section.
- Rewriting the substance of migrated project articles beyond frontmatter, links, and minimal wording needed to remove references to the old projects section.
