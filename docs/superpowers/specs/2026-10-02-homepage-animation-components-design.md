# Independent homepage animation components

## Intent and approval

The user wants the homepage sections to be separate components so each animation
can be maintained and tested independently. Each section's detailed content must
appear only after its heading has finished writing.

The user approved the conversational design on October 2, 2026. This document
records that design for review; implementation planning requires approval of this
written spec.

“Load in” means visually reveal. Content remains server-rendered in the initial
HTML; this change does not introduce deferred fetching.

## Starting point

The current working tree already contains the approved Anime.js animations and
handwriting utility. These changes are not yet committed. This refactor builds on
that working implementation, rather than the older implementation at HEAD.

`src/pages/index.astro` currently owns the hero timeline, section observers, font
readiness, reduced-motion handling, and cleanup for all sections. Its article
animation targets titles only, leaving descriptions, dates, and tags visible
while the heading is being written. `DevHero.astro` already owns hero markup;
`PostList.astro` renders article entries for both the homepage and blog.

## Component ownership

| Component             | Responsibility                                                           | Inputs                                         |
| --------------------- | ------------------------------------------------------------------------ | ---------------------------------------------- |
| `DevHero.astro`       | Hero markup, heading writing, underline, illustration, and detail reveal | Existing hero props                            |
| `FeaturedWork.astro`  | Featured section markup and its complete reveal sequence                 | Featured posts                                 |
| `RecentWriting.astro` | Recent section markup, reveal sequence, and All writing link             | Recent posts                                   |
| `PostList.astro`      | Shared article markup, without owning homepage animation behavior        | Existing posts, label, and heading-level props |
| `index.astro`         | Obtain and partition content, then compose the three sections            | Astro content collection                       |

Recent writing remains omitted when there are no recent posts. Existing copy,
links, heading levels, content order, artwork, and responsive layout are preserved.
Blog-page uses of `PostList` retain their current behavior.

Each section owns an animation controller scoped to its own root element. It can
mount, complete, and dispose without querying or mutating sibling sections. Two
instances of the same component must also work independently.

Shared helpers retain the handwriting implementation and common lifecycle and
fallback behavior. A helper receives a component root and explicit collaborators;
it must not discover all homepage sections through document-wide selectors.
Section-specific sequencing stays with the owning component. No new UI framework
or general-purpose animation configuration system is required.

## Reveal behavior

### Hero

1. Write the headline in its existing reading order.
2. Draw its underline after the handwriting completes.
3. Reveal the description, actions, social links, and supporting text after the
   underline completes. Supporting text includes the role label, illustration
   note, and portrait caption.

The illustration keeps its existing arrival, flourish, and single plant-sway
behavior. Those decorative animations do not release the detail reveal early.

### Featured work and recent writing

Each heading starts writing once it enters the viewport. After its handwriting
completes, reveal complete article entries with a short stagger. Each entry's
title, date, description, type, reading time, and tags appear together. The recent
section's All writing link appears after its entries. Empty-state text, when
present, follows the same heading-first rule.

A section does not wait for another section. Each runs once per mounted page
visit. Returning through Astro navigation creates a new visit and a fresh sequence.

### Completion contract

Detail reveals are driven by the actual completion of the preceding animation,
not by a second copy of its duration. Changing handwriting duration must not
require changing a separate detail delay.

The component lifecycle distinguishes waiting, writing, revealing, complete, and
disposed states. Disposal cancels pending observers and asynchronous work; late
font readiness or animation completion must not restart a disposed component.
Disposal is safe to repeat. This is a small lifecycle contract, not a requirement
for a state-machine dependency.

## Accessibility, fallback, and layout

- Keep original semantic headings and content in server-rendered HTML. Decorative
  SVG copies remain hidden from assistive technology.
- Without JavaScript or with reduced motion enabled at startup, show all content
  immediately, without waiting for handwriting or fonts.
- If reduced motion is enabled during a sequence, stop motion and restore the
  complete heading and details immediately.
- Preserve space for pending details so revealing them does not move later
  sections. Hidden controls must not remain keyboard-focusable.
- Normal animation activation must not flash details visibly before hiding them.
  If enhancement cannot initialize safely, use the static presentation.
- Missing or failed fonts, unsupported glyphs, or animation initialization failure
  must fall back to readable content instead of leaving pending details hidden.
  Any early pending marker needs a bounded fallback if the animation module fails
  to load; content must be restored within two seconds in that case.
- Resize cancels measurements and restores static content for that mounted
  instance. It does not replay the animation automatically.
- Astro navigation and page teardown release listeners, observers, timelines,
  temporary masks, and pending work. Other sections remain unaffected when one
  component is disposed independently.

## Independent verification

Provide test-only browser fixtures that render the real Astro components in
isolation. Fixtures use representative post data and are excluded from the
production site's routes, sitemap, and published image.

Use real Anime.js, handwriting, SVG layout, and component initialization in these
checks. Browser tests should observe rendered content, visibility, focusability,
and ordering; assertions should not depend on private helper calls or the exact
number of generated SVG nodes. Browser animation-time controls may accelerate
execution without replacing the production sequence with a fake implementation.

| Check                                        | Regression it must detect                                                               |
| -------------------------------------------- | --------------------------------------------------------------------------------------- |
| Hero in isolation                            | Details appearing before headline and underline finish                                  |
| Each post section in isolation               | Metadata or descriptions appearing before the heading, or entries revealing piecemeal   |
| Altered heading duration                     | Detail reveal depending on a duplicated fixed delay                                     |
| Two section instances                        | Shared selectors, identifiers, observers, or cleanup affecting the other instance       |
| Disposal during font wait or writing         | Late callbacks restarting motion or leaving hidden content                              |
| Reduced motion at startup and during writing | Unnecessary waits, remaining motion, or invisible details                               |
| JavaScript/font/module failure               | Content permanently hidden by enhancement                                               |
| Mobile layout and resize                     | Overflow, layout shifts, or stale measurement masks                                     |
| Homepage navigation                          | Missing initialization, duplicate sequences, or failure to clean up across Astro visits |

Component browser fixtures protect the independently observable behavior. Keep a
small homepage integration test to cover composition, independent viewport
triggers, and navigation. Add direct lifecycle tests only where they exercise
meaningful deterministic decisions more clearly than browser coverage. Do not
introduce interfaces solely to mock internal helpers.

Browser tooling and fixtures must run through Bazel, with reproducible targets
that can run each component's tests independently. The temporary local recording
and smoke-test scripts from earlier work are baseline evidence, not the permanent
test suite. The implementation plan must specify concrete targets and toolchain
wiring after inspecting the repository's available test infrastructure.

## Delivery checks

After source changes, run Gazelle immediately, followed by repository-wide
formatting. Run the new component tests, homepage integration coverage, existing
`//personal_website/...` tests, the full repository build, lint, and
`git diff --check`, using the repository's Nix/Bazel workflow.

Record actual browser evidence for the reveal order and mobile behavior. Passing
source or type checks alone does not demonstrate the animation sequence.

## Scope boundaries

This work covers homepage component extraction, independent animation lifecycle
ownership, complete detail sequencing, and durable tests. It does not redesign the
page, change content selection, add network loading, animate blog pages, replace
Anime.js, or regenerate the artwork.
