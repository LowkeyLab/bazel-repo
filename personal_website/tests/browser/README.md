# Browser fixtures

The Bazel `fixtures_build` target injects test routes into a separate Astro build. The production build, sitemap, and image do not include these routes. Each browser test starts its own static server on an ephemeral loopback port and a fresh Chromium context. Local Nix shells supply `CHROMIUM_EXECUTABLE`; remote execution uses the pinned Microsoft Playwright 1.63.0 Noble image and its bundled browser.

Browser tests use the same embedded variable Caveat font as production, with all external network requests blocked. There is no separate test font or Google Fonts substitution. For delayed, held, and blocked font cases, the harness extracts the production font bytes from the built CSS and serves them through a controlled local route. Normal tests exercise the unmodified embedded font.

Font provenance is recorded in `src/assets/fonts/README.md`; the SIL Open Font License is published at `/caveat-OFL.txt`.

Run the four browser targets with `nix develop --command aspect test //personal_website/tests/browser:tests`. The homepage target serves the normal production build while the three component targets serve the separate fixture build. Every target starts its own loopback server, so an already running preview is unnecessary. For a local Chromium smoke run use `nix develop --command aspect test //personal_website/tests/browser:tests --strategy=TestRunner=local --test_env=CHROMIUM_EXECUTABLE`; the Nix shell supplies the executable path. The normal command uses the declared remote browser image.

## TypeScript and Bazel targets

The tests and shared `harness.ts` are strictly type-checked and compiled by Bazel. `gazelle_ts` discovers their sources and imports through the `ts_test` mapping in `tests/BUILD.bazel`, generating `personal_website_ts_test_sources` compilation targets and the harness library. The existing `js_test` targets run the emitted `.test.js` files and retain their fixture data, environment, and pinned browser image.

After editing a TypeScript source, run `nix develop --command bazel run //:gazelle` to update its BUILD dependencies. Run all migrated tests, including blog selection, with `nix develop --command aspect test //personal_website/tests/...`.

## Local animation diagnostics

Animation controllers emit a structured `SectionAnimationSettled` record after
restoring static content. In browser developer tools, enable debug messages to
inspect completed reveals and expected cancellations; unexpected static fallbacks
appear as warnings. This is local, synchronous, best-effort delivery to the browser
console, with no remote destination, retained queue, retries, or shutdown flush.

Each record contains a bounded section kind (`hero`, `featured_work`,
`recent_writing`, or `other`), document-local numeric `runId`, current `phase`, and
monotonic `elapsedMs`. A terminal `outcome` is `completed`, `cancelled`, or
`fallback`. Cancellation reasons distinguish preference changes, resize,
pagehide, disconnection, and other cancellation. Fallback reasons distinguish
unavailable fonts, unavailable initialization, watchdog expiry, and exceptions.
The general initialization reason does not assert an unobserved underlying cause.
Records exclude article text, URLs, user/storage data, and raw exceptions.

The inline bootstrap and controller share a terminal guard: a later module load,
font response, resize, or disposal cannot emit a second terminal record. When the
listener module never loads, the bootstrap emits its watchdog warning directly.
A failing listener receives no retry; a minimal sanitized direct warning reports
delivery failure without recursively emitting another animation event. With
JavaScript disabled, static-content tests remain the evidence of usability.

Browser tests capture structured console arguments, not rendered message strings.
They check outcomes together with visibility and keyboard accessibility, terminal
uniqueness, severity mapping, independent run IDs, and listener failure isolation.
