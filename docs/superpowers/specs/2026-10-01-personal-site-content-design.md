# Personal site: one stream of writing

## Intent and approval status

The site should feel like a personal corner of the internet, where professional work, personal projects, technical explanations, and essays naturally become things worth reading. Preserve the existing visual identity and personality, including “Software Engineer & Toil Eliminator”, “I build leverage for people.”, and “Less toil. More leverage.” wherever currently used.

The user approved the content/navigation design and publishing/verification design in chat. They explicitly excluded work periods from both metadata and articles. This document records that design for written-spec review; implementation and an implementation plan are not yet approved.

## Current system

The Astro site has separate blog and work collections. Five project articles already live in blog, alongside an essay and a draft. Three work entries use company, role, and start date metadata. The Work index also contains a skills inventory. The homepage currently contains the introduction only. There is no About page.

Caddy serves the static build and already permanently redirects legacy project URLs. The shared layout has basic title and description metadata, but no canonical, Open Graph, or structured-data implementation; RSS and sitemap generation are also absent. Content-based ETags and existing cache behavior must remain intact.

## Content model and flow

Use only the existing blog collection for substantial content. Retain title, description, `publishDate`, tags, and draft. Add:

- type: required enum of work, project, essay, or note; explicitly classify existing posts.
- featured: optional boolean, default false, for homepage curation.
- context: optional company text, used only as quiet article context.

Do not add work periods, employment dates, roles, or a technology inventory. Do not add speculative repository, website, status, or lesson-component fields; existing project links can remain in article bodies.

Use roughly two to five useful tags per post. Types identify the kind of writing; tags identify its subjects. All public views consume the same published-post selection and chronological ordering. Featured posts are selected from that published set. Preserve the current ability to access an existing draft directly, while excluding drafts from the homepage, blog listing, RSS, and sitemap. Draft pages should be marked noindex.

Retain all existing blog slugs and publication dates. Rewritten work posts receive their actual publication date, explicitly set when published, never inferred from employment history. Publication dates are the only article date concept in this change.

## Navigation and page behavior

Primary navigation becomes Blog, About, GitHub, retaining the theme control. The site brand links home. Preserve existing social/contact links in the introduction.

The homepage retains its headline and visual identity, uses the approved shorter introduction below, and adds:

1. “Things I've worked on”: three curated published posts. Initially select the JVM RPC story, Mindreadr, and the IOI story, reflecting the original brief's professional/personal mix.
2. “Recent writing”: up to three latest remaining published posts, excluding featured posts to avoid duplicate cards. If none remain, omit this section. Place the “All writing →” link below the final recent-writing entry, not beside the section heading.

The Blog page shows all published posts, newest first, using the current list styling. Each entry shows title, description, publication date, a subdued type label, and sparse tags; preserve useful reading-time information. No filtering, search, category routes, or client-side filter state in this iteration.

All articles use the existing blog presentation. Optional company context remains visually secondary. No employer logos, timelines, skill meters, or corporate Work cards. Lessons can appear as ordinary prose; a reusable lesson component is deferred.

Home introduces who Tim is today; About explains the broader personal journey behind those interests. About must not repeat the homepage or focus solely on Bloomberg. Use dense, connected prose rather than a chronology, technology list, or repeated contact section. Keep contact links on Home. Add `More about me →` beneath the homepage introduction. About ends with its final narrative paragraph, without a “Say hello” section or generic blog CTA. Preserve any existing resume if encountered; creating one is out of scope.

Do not use em dashes in new site copy.

<!-- Preserve the accepted first-person Home/About copy. -->
<!-- vale Google.FirstPerson = NO -->

### Approved homepage introduction

I build tools that help people spend less time wrestling with software and more time making things. I’m especially interested in automation, build systems, and making complicated work easier to do.

### Approved about copy

I wasn’t especially interested in software in college. At college, I considered Electrical and Computer Engineering my main degree. Computer Science was something I could tack on and, honestly, a way to one-up my classmates. I went through an entire Computer Science degree without feeling particularly drawn to building software.

Starting my first full-time software job changed that. I felt woefully underprepared, so I started reading software engineering books, listening to podcasts, and watching talks. At first, I was trying to catch up. Over time, I began developing my own judgment and taste from what I read, the authors’ experiences, and my own mistakes.

Around 2023, I realized that despite everything I’d learned, I still didn’t know how to build a website. So I started learning and actually applying the ideas I’d been collecting. This blog, with its many iterations, began then.

Along the way, I developed a fervour for automation and eliminating toil. I’m interested in building the automation that builds the automation: looking beyond an individual task to the system that keeps producing it. That way of thinking connects my professional work, personal projects, and the things I write about here.

<!-- vale Google.FirstPerson = YES -->

## Editorial design and interview evidence

Rewrite the work entries as distinct engineering stories, using natural headings only where useful. Use concrete first-person contributions, distinguish individual work from team outcomes, and retain the user's candid voice. Do not invent metrics, incidents, motives, measured improvements, or confidential details beyond the material supplied. The user confirmed no interview details need omission or generalization.

### Building JVM RPC tooling around Bloomberg's needs

Destination: `/blog/building-jvm-rpc-tooling/`

The JVM community's attempts to adapt entire open source solutions produced incompatibilities and performance issues with Bloomberg's internal technologies. Foundational RPC libraries, including codecs, were unmaintained and had bugs left unfixed for years.

The author developed a Kotlin-based JVM code generator and codec around Bloomberg's requirements, aiming for better type safety, performance, and cross-language compatibility. The old XJC-based tooling could not represent xs:choice as sealed type hierarchies. The old BER codec had subtle behavioral differences from C++ and Python implementations. Present these as concrete motivations; do not claim verified parity or quantified speedups without further evidence.

The new generator supports Jackson and kotlinx.serialization. KotlinPoet handles Kotlin source generation. Jackson supplies a framework for implementing Bloomberg data formats; kotlinx.serialization supplies a serialization framework with custom encoders and decoders. Bloomberg's JSON, XML, and BER behavior did not fit the previous entire-stack reuse approach.

Status: limited beta, moving toward production. Do not describe this as fully deployed or attribute production outcomes to it.

The lesson is deliberate reuse: retain useful frameworks, implement the behavior specific to the environment, and inspect or adapt open source code with appropriate attribution and licensing. Blind reuse and blind rewrites both miss that boundary. The project expanded the author's understanding of code generation, schemaful and schemaless formats, serialization, wire formats, and language/hardware performance optimization. Distinguish this learning from demonstrated performance results.

### Prototyping a team's first JVM pipeline

Destination: /blog/prototyping-a-flink-pipeline/

In 2022, an existing end-of-day C++ pipeline motivated a Flink proof of concept. State 2022 in the article prose; do not confuse the work year with the later article publication date or add a work-period field. The team was unfamiliar with Java, and this was its first JVM service. The deployment also moved from traditional servers to Kubernetes. Describe Kubernetes as this project's deployment choice, not a universal Flink requirement.

The author built a functional end-to-end prototype: app code, builds, container images, CI/CD, Kubernetes deployment, and dummy traffic through Flink. Existing source material supports porting business logic first in Java and then Kotlin. Teaching the team the JVM, Java, Kotlin, and Kubernetes took substantial effort.

The team subsequently improved the prototype and deployed it to production about a year later. Do not imply the author personally completed that later rollout.

This was the author's first contact with the JVM Guild. Its help, alongside the ecosystem friction encountered, motivated community involvement and later work improving the experience for others. This connection is the personal conclusion, rather than an invented technical moral.

### Making a platform buildable and deployable again

Destination: /blog/making-a-platform-buildable-again/

The model-driven architecture platform already existed. Despite expectations of a mature JVM environment, the author found an aging stack, dependencies that could not be upgraded, fragile builds, and complicated environment-specific release branches. Teams struggled to build, test, patch, and ship services, undermining plans to produce many such services.

The author took initiative beyond the original assignment to modernize the stack: remove Apache Camel, upgrade Spring Boot 2 to 3, simplify development interfaces, and share templates, documentation, and reusable libraries across the organization.

A concrete immediate threat was removal of Python 2, which powered model-to-Java code generation. First migrate generation to Python 3. Then centralize Python generation in one repository, build/package generated Java as JARs, and distribute those to consumers. Downstream Java teams no longer need Python or the generation toolchain to build their services.

Replace environment-specific release branches with PR checks and a main-branch release flow. Builds and integration tests run in Docker, with Gradle remote build caching. Merging to main produces a deployable artifact and triggers Bloomberg's automated rollout tooling. Avoid promising reproducibility properties beyond the described controlled build environment.

<!-- The paired alternatives or compound predicates are not three-item lists. -->
<!-- vale Google.OxfordComma = NO -->

The replacement is in production, stable and performant according to the author's experience, with continuing adoption. Do not invent performance measurements. The story's lesson is to remove generation and release complexity from consuming teams, supported by the concrete changes described earlier.
<!-- vale Google.OxfordComma = YES -->

### Existing project articles and essays

Keep all five project URLs, useful technical content, and publication dates. Assign project type and improve README-like framing into building stories where needed. Do not manufacture anecdotes or outcomes to lengthen short articles. Preserve the existing essays' accepted copy and the Rust article's draft status. A project article need not use the same headings as a professional story.

## URL compatibility and discovery

Use explicit Caddy permanent redirects for both trailing-slash variants of each source:

| Retired path                    | Destination                              |
| ------------------------------- | ---------------------------------------- |
| /work                           | /blog/                                   |
| /work/core-java-infrastructure  | `/blog/building-jvm-rpc-tooling/`        |
| /work/ioi-pipeline              | /blog/prototyping-a-flink-pipeline/      |
| /work/model-driven-architecture | /blog/making-a-platform-buildable-again/ |

Match the final canonical URL convention across generated links, redirects, and metadata; accept existing no-slash blog links. Retain every existing legacy project redirect. Do not create a new independent Work page or a broad redirect that hides unrelated missing URLs.

Add RSS at /rss.xml and a sitemap at /sitemap.xml, both generated from the public content set. Sitemap includes Home, About, Blog, and published articles, excluding retired URLs and drafts. RSS includes all published post types and links to their canonical articles. Add feed discovery in the shared head and the sitemap location to robots.txt.

Configure one verified production site origin for absolute URLs; confirm it from deployment configuration before implementation rather than inferring it solely from the robots.txt comment. Add canonical URLs and Open Graph title, description, type, and URL metadata, plus BlogPosting structured data for published articles with headline, description, publication date, author, and canonical URL. Use an existing suitable public raster image for social previews if available; otherwise provide text metadata without inventing an image asset. This change does not include image generation or an asset redesign.

## Boundaries, cleanup, and failure behavior

The content schema validates required metadata at build time. Shared published-post selection prevents listing/feed drift. Shared list/article presentation avoids parallel templates by post type. Caddy owns legacy HTTP redirects; generated content owns canonical article URLs. Keep feed generation small and build-time only.

Delete the work collection, old work content directory after migration, work page templates, skills inventory, and styles/components that become unused. Retain shared components that still have callers. Preserve history with file moves where practical. Update internal links to destinations instead of relying on redirects.

Unknown paths should still return 404. An invalid content entry should fail the build rather than silently disappear. No database, runtime publishing service, new design system, filtering framework, resume, or employment timeline is needed.

## Delivery sequence

Implement in stages within this single refactor: unified metadata; blog presentation; work-story migration; permanent redirects; project framing; homepage/About/navigation; discovery metadata and cleanup. Avoid shipping an intermediate state that removes routes before replacements and redirects exist.

After written-spec approval, create an implementation plan using the writing-plans skill and have the user select its execution method. Do not begin product implementation from conversational design approval alone.

## Verification and acceptance

- All substantial work and project content uses the blog collection and article presentation.
- All three work stories match the interview evidence and deployment statuses described earlier.
- No work-period fields or displays exist; publication dates remain distinct from employment history.
- Home shows three curated posts and chronological nonduplicated recent writing; draft content stays out of public lists and feeds.
- Blog, About, GitHub navigation and existing social/contact links work in both themes and at mobile widths.
- Every retired Work URL and existing legacy Projects URL returns a permanent HTTP redirect to a successful final page, with and without trailing slashes.
- Rendered internal links resolve, unrelated unknown paths retain 404 behavior, and existing cache/ETag checks remain passing.
- RSS and sitemap contain all published migrated content, valid absolute canonical URLs, and no drafts or retired paths.
- Rendered canonical, Open Graph, and structured metadata match article content and dates; inspect output, not merely source strings.
- No unused Work templates or skills inventory remain.
- Run Gazelle immediately after source edits and before formatting; use Bazel/Aspect through the repository's Nix environment.
- Run `aspect format --scope=all`, focused website checks and relevant HTTP tests, `aspect build //...`, `aspect lint`, and `git diff --check` for implementation. Confirm actual test execution and report any warnings or environmental limitations accurately.

This spec-only change requires document review, repository formatting, and whitespace verification. Runtime/build acceptance remains for the implementation stage.
