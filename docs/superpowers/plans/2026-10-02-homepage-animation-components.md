# Homepage Animation Components Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give each homepage section independently testable animation ownership and reveal all of its details only after its heading sequence finishes.

**Architecture:** Three Astro components own their markup and root-scoped controllers. A shared lifecycle helper coordinates font readiness, completion, visibility, cancellation, and fallback; it does not discover or sequence sibling sections. Browser fixtures exercise the real components and animation library through separate Bazel targets.

**Tech Stack:** Astro, TypeScript, Anime.js 4.5.0, Playwright, Chromium, Bazel/Aspect, Nix.

**Spec:** [Independent homepage animation components](../specs/2026-10-02-homepage-animation-components-design.md).

## Global Constraints

- “Load in” means visually reveal. Content remains server-rendered in the initial HTML; this change does not introduce deferred fetching.
- Preserve copy, links, heading levels, content order, artwork, and responsive layout. Keep blog-page `PostList` behavior unchanged.
- Reveal complete article entries, including title, date, description, type, reading time, and tags, together after the section heading.
- Hero details include the role label, description, actions, socials, illustration note, and portrait caption. They follow headline writing and underline completion.
- Each section runs once per mounted visit. Cancellation restores readable content and prevents late callbacks from restarting motion.
- No JavaScript or reduced motion means immediate static content. A failed animation-module load must restore early pending content within two seconds.
- Fixtures must not enter production routes, sitemap, or published image. Browser tests use real animation and rendering code.
- Use the existing worktree. Prior approved animation changes are still uncommitted; preserve and include them in the implementation baseline, never discard them.
- Run Gazelle immediately after every source-edit batch and before manually adjusting BUILD files. Run repository-wide formatting before commits.
- Use `nix develop --command bazel/aspect ...`; no direct package-manager build/test commands. Use conventional commits and stage explicit paths.

## Review Focus

1. A component removed while fonts are loading must never resume or hide content elsewhere: Task 1 lifecycle and Task 4 navigation tests.
2. One of two identical components may complete or disconnect first without affecting its sibling: Task 2 isolation tests.
3. Unsupported glyphs, blocked fonts, and blocked animation modules must leave readable, focusable static content: Tasks 1 and 4 fallback tests.
4. Long titles, empty lists, dark mode, and resizing during writing must preserve usable layout: Tasks 2, 3, and 4 browser cases.
5. A different handwriting duration must change the detail-reveal time automatically: Task 1 completion contract and Task 2 real-animation variant.

## File and interface map

Production paths below are relative to `personal_website/`.

- Modify `src/components/DevHero.astro`; create `FeaturedWork.astro` and `RecentWriting.astro` beside it. Each owns an autonomous custom-element wrapper and its component script. Use distinct tags `home-hero-animation`, `featured-work-animation`, and `recent-writing-animation`, styled as block containers.
- Create `src/utils/section-animation.ts`: common lifecycle, without document-wide component selectors.
- Create `src/components/hero-animation.ts`, `featured-work-animation.ts`, and `recent-writing-animation.ts`: each exports `mount(root: HTMLElement): () => void` for its component. Controllers obtain all targets from that root. Custom-element disconnect calls the returned disposer; registration uses `customElements.get` to avoid duplicate definitions after Astro navigation.
- Extend `src/utils/handwriting.ts` with a completion/cancellation adapter while retaining the existing mask implementation.
- Create `src/components/AnimationBootstrap.astro`: early per-instance pending marker and bounded fallback, shared by the three wrappers.
- Modify `src/pages/index.astro` to compose components only. Modify `src/styles/global.css` for pending-detail layout and component wrapper display.
- Create `tests/fixtures.config.ts`, `tests/fixtures/{hero,featured-work,recent-writing,independence,lifecycle}.astro`, and `tests/fixtures/posts.ts`. Inject these routes through the separate test config, importing real production components. Do not put fixtures under production `src/pages`.
- Create `tests/browser/{harness,hero,featured-work,recent-writing,homepage}.mjs` and `tests/browser/README.md`.
- Modify `personal_website/BUILD.bazel`, generated source BUILD files, root `package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`, `MODULE.bazel`, and `flake.nix` only as needed for declared browser tooling and fixture inputs. Create `personal_website/tests/BUILD.bazel` and `personal_website/tests/browser/BUILD.bazel`.

Shared production contracts:

```ts
export interface AnimationRun {
  finished: Promise<"completed" | "cancelled">;
  cancel(): void; // Idempotently restore any temporary styles/markup.
}
export interface SectionSequence {
  heading: HTMLElement;
  details: readonly HTMLElement[];
  trigger: "immediate" | "visible";
  write(): AnimationRun | undefined;
  afterWrite?(): AnimationRun | undefined;
  reveal(): AnimationRun;
}
export function mountSection(
  root: HTMLElement,
  sequence: SectionSequence,
): () => void;
export function writeHeading(
  heading: HTMLElement,
  duration: number,
): AnimationRun | undefined;
```

`writeHeading` returns undefined before mutation for unsupported text or unavailable drawing support. In that case `mountSection` restores the entire static section immediately. Successful writing calls `afterWrite` when supplied, then `reveal`, using the preceding run's actual completion. Cancellation never advances the sequence.

## Task 1: Independently testable hero and shared lifecycle

**Files:** Hero, bootstrap, shared lifecycle, handwriting adapter, styles, hero fixture/test, browser harness/config, and toolchain/build files from the map.

**Interfaces:** Produces the contracts above; retains existing `DevHero` props. Produces `//personal_website/tests/browser:hero_test` and `//personal_website/tests:fixtures_build`.

- [ ] Record the baseline with `nix develop --command aspect test //personal_website/...` and `git diff --check`. Commit the prior approved animation files explicitly as `feat(website): add sketchbook homepage animations`; keep the approved spec commit and any unrelated user changes separate.
- [ ] Add `playwright` pinned to `1.63.0` through the root catalog and Bazel-managed pnpm. Resolve and record the immutable digest of official `mcr.microsoft.com/playwright:v1.63.0-noble` for the browser-test rule's remote `container-image` execution property. The npm package and image version must match. Do not download browser binaries during tests.
- [ ] Add `CHROMIUM_EXECUTABLE = "${pkgs.chromium}/bin/chromium"` to the Nix shell environment for local execution, using the existing locked nixpkgs. The harness uses that explicit path when supplied, otherwise the image's `/ms-playwright` browser installation. Do not use absolute developer cache/store paths in checked-in files. Keep the local override out of remote test invocations.
- [ ] Add a SHA-256-pinned Caveat font download via Bazel `http_file`, with license attribution under `tests/browser/README.md`. Intercept the Google font requests in browser tests and fulfill them from this declared fixture asset. Production continues using its existing CDN font configuration; tests make no external HTTP requests.
- [ ] Build test-only routes with a separate Astro config importing the production config and injecting fixture pages, with a distinct output directory. Add a `js_test` harness that serves declared built fixtures on an ephemeral loopback port, launches Chromium, uses a fresh browser context per case, saves failure screenshots/traces under `TEST_UNDECLARED_OUTPUTS_DIR`, and always closes the browser/server. Fail on page errors or unavailable browser prerequisites; never silently skip.
- [ ] Expose `withPage(options, run)` from `harness.mjs`, where options contain `{ route, viewport?, reducedMotion?, javaScriptEnabled?, fontMode?, blockAnimationModule? }`, `run` receives the Playwright page, and the returned Promise includes teardown. Fixtures render original components and representative props. The lifecycle fixture renders two plain heading/detail roots using the real shared helper, with handwriting durations 1200 ms and 2600 ms; it is test-only and introduces no production debug switch. Use computed opacity and visibility plus keyboard traversal rather than relying on Playwright visibility checks alone, which do not detect opacity-zero text. Do not assert generated mask-node counts.
- [ ] Write `hero.test` cases before changing production sequencing. Capture the heading/detail bounding boxes, hold the browser clock while font readiness settles, then advance animation frames. Assert details are invisible and untabbable during writing and underline drawing; afterwards all details are visible, keyboard reachable, and occupy the reserved boxes. Assert no initial visible-then-hidden flash.
- [ ] Run `nix develop --command aspect test //personal_website/tests/browser:hero_test`. Expect a behavior failure because the current hero reveals its description and actions before the headline finishes; fix harness/configuration errors before treating that as the red result.
- [ ] Implement `writeHeading` and `mountSection`. Guard the asynchronous font wait and every completion continuation with per-instance disposal state. Track all active runs and return one idempotent disposer. Reduced motion, resize, missing fonts, unsupported glyphs, or exceptions restore all detail visibility and focusability, and remove pending state. Reuse Anime.js completion callbacks rather than reusing duration constants as timers.
- [ ] Implement `AnimationBootstrap.astro` inside each root. Before detail markup paints, activate pending presentation only when motion is allowed, and arrange a two-second static fallback if the component module never takes ownership. Hand off and clear that watchdog only after the controller is ready; never leave a pending marker with no owner. Pending details preserve layout and are not focusable. No-JavaScript and reduced-motion paths never mark details pending.
- [ ] Move hero sequencing into `hero-animation.ts` and mount it from `DevHero.astro`'s custom-element lifecycle, after its children are available. Start heading writing immediately, then its underline, then its detailed text. Keep decorative illustration motion component-owned and cancel it with the same lifecycle. Remove hero orchestration from the page script without changing post sections yet.
- [ ] Add focused cases for startup/mid-animation reduced motion, removal during delayed font loading, resize, blocked fonts, and cancellation before completion. Test the completion contract with real handwriting at 1200 ms and 2600 ms using an isolated harness fixture that calls the production adapter; there must be no alternate production test mode or duplicated detail delay.
- [ ] Run the hero target and existing website tests; expect all cases to execute and pass. Run format and `git diff --check`, then commit the task as `refactor(website): isolate hero animation lifecycle`.

## Task 2: Featured work owns complete entry reveals

**Files:** Create `FeaturedWork.astro`, `featured-work-animation.ts`, featured-work and independence fixtures/tests; modify homepage composition and relevant generated BUILD files.

**Interfaces:** Consumes `mountSection` and `writeHeading` from Task 1. Produces `FeaturedWork` with `Props { posts: CollectionEntry<"blog">[]; headingId?: string }` (default `featured-heading`), `mount(root: HTMLElement): () => void`, and `//personal_website/tests/browser:featured_work_test`.

- [ ] Write `featured_details_follow_heading`: render three representative posts; while the heading writes, assert every complete entry is hidden, including its date, description, reading time, and tags. After writing, assert entries reveal in order and each entry's child content reveals together.
- [ ] Write `two_featured_instances_are_independent`: mount two roots, activate one while the other stays outside the viewport, and remove the active root during writing. Assert the sibling remains intact and runs when subsequently scrolled into view. Give fixtures unique heading IDs through instance-owned ID generation, without changing the production featured-heading anchor.
- [ ] Run the new target and establish a red result before extraction. A missing-component build failure is initial scaffolding evidence; after the markup-only extraction, confirm the reveal-order assertion fails against the old title-only behavior.
- [ ] Implement `FeaturedWork` with the existing heading text and `PostList` markup. Use `mountSection` with the heading as the viewport trigger and `.entry-link` elements as complete detail groups. The heading writes once per mount; its actual completion releases the entry stagger. Scope all observers and selectors to the component root.
- [ ] Remove only the featured markup and featured animation handling from `index.astro`; pass its existing `featured` result into the component. Preserve the `featured-heading` anchor and accessible label. Support an optional `headingId` prop defaulting to `featured-heading` so independent fixture instances have valid unique IDs.
- [ ] Add cases for empty-list text following the heading, unsupported heading text in the direct lifecycle fixture, altered handwriting duration, and very long titles/tags at 390 px in both themes. Assert horizontal overflow is absent and later sections do not move when details appear.
- [ ] Run `nix develop --command aspect test //personal_website/tests/browser:featured_work_test //personal_website/tests/browser:hero_test`; expect all cases to execute and pass. Format, check whitespace, and commit as `refactor(website): isolate featured work animations`.

## Task 3: Recent writing owns entries and its final link

**Files:** Create `RecentWriting.astro`, `recent-writing-animation.ts`, recent fixture/test; finish simplifying `index.astro`; update generated BUILD files.

**Interfaces:** Consumes Task 1 helpers and existing `PostList`. Produces `RecentWriting` with `Props { posts: CollectionEntry<"blog">[]; headingId?: string }`, default heading ID `recent-heading`, its controller's `mount`, and `//personal_website/tests/browser:recent_writing_test`.

- [ ] Write `recent_entries_and_link_follow_heading`: assert complete entries remain hidden until heading completion, entries reveal in sequence, and All writing is revealed after the entry sequence. Assert the link retains `/blog/` and becomes keyboard reachable after reveal.
- [ ] Write `empty_recent_section_is_omitted`: render with an empty posts array and assert no recent heading, empty wrapper, observer-owned content, or All writing link appears.
- [ ] Establish a behavior failure with the extracted markup and existing title-only animation, then implement the component-owned sequence using `mountSection`. Use the same lifecycle semantics as FeaturedWork without merging the two controllers into a configurable homepage orchestrator.
- [ ] Make `index.astro` obtain `homePosts`, render the hero and two post components, and contain no animation script. Each component owns its mounting and teardown; remove the old page-wide listeners and selectors.
- [ ] Run `nix develop --command aspect test //personal_website/tests/browser:recent_writing_test //personal_website/tests/browser:featured_work_test //personal_website/tests/browser:hero_test` and existing website tests. Expect all cases to execute and pass. Format, check whitespace, and commit as `refactor(website): isolate recent writing animations`.

## Task 4: Production composition, failure paths, and delivery

**Files:** `tests/browser/homepage.mjs`, browser BUILD/README, `site_structure_test.sh`; narrowly fix owning production modules if these tests expose defects.

**Interfaces:** Produces `//personal_website/tests/browser:homepage_test` and `//personal_website/tests/browser:tests` (suite of all four browser targets).

- [ ] Add `homepage_sections_run_independently` against the normal production build: scroll to recent writing while another sequence remains active and assert each section's detail visibility depends only on its own heading completion. Verify the blog listing remains static.
- [ ] Add `navigation_disposes_pending_work`: leave during handwriting, return through Astro navigation, and assert a fresh sequence with one reveal; delay font loading before leaving and assert late resolution cannot mutate the new page. Exercise two visits and check page errors.
- [ ] Add `fallback_preserves_all_content`: block the animation module and assert readable, focusable details by 2000 ms; separately disable JavaScript, enable reduced motion at startup, block font responses, and resize during writing. No path may leave content hidden.
- [ ] Add `mobile_preserves_layout`: test 390 px and 430 px widths, including orientation/viewport change during writing; compare reserved bounding boxes before and after reveal. Include desktop 1440 px and both themes.
- [ ] Extend `site_structure_test.sh` to assert injected fixture paths and fixture-only font assets are absent from production output and sitemap. Confirm they are also absent from `frontend_layer.tar`.
- [ ] Run the focused browser suite and verify every target reports executed tests. Run a local smoke invocation through the Nix browser path: `nix develop --command aspect test //personal_website/tests/browser:tests --strategy=TestRunner=local --test_env=CHROMIUM_EXECUTABLE`. The normal command uses the declared remote browser image. Document both and verify neither needs an already running preview server.
- [ ] Run `nix develop --command aspect format --scope=all`, `nix develop --command aspect test //personal_website/...`, `nix develop --command aspect build //...`, `nix develop --command aspect lint`, and `git diff --check`. Resolve actual failures and distinguish non-blocking toolchain warnings from errors.
- [ ] Review the rendered mobile sequence and record a refreshed short video using the existing preview workflow. Keep video artifacts outside source directories. Record validation results in the final report, then commit as `test(website): verify independent section reveal behavior`.

## Tooling references and boundaries

The official [Playwright Docker guide](https://playwright.dev/docs/docker) documents the browser/system-dependency image and matching package/image versions. Browser packaging is test infrastructure; neither Playwright nor fixture fonts belong in the shipped website bundle. Resolve and record external artifact digests during implementation, then run tests without external network access.

Implementation must not be declared complete on the strength of type checks, skipped browser tests, or temporary local scripts. The four Bazel browser targets are the durable acceptance checks for this refactor.
