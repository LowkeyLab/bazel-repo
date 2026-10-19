# Browser and Astro original-source coverage guarantees

Date: 2026-10-19.
Repository base: `5e246a47cd7ea0a9c506e915db0c053d7d84d023`.
Question: [Determine browser and Astro original-source coverage guarantees][ticket].

## Result and evidence limits

Playwright supplies a Chromium coverage API. The maintained browser journeys do not call that API.
Node driver coverage cannot establish browser coverage. Astro rendering also occurs outside those drivers. [Browser API][pw-doc], [Website harness][harness], [Book Smartz journey][journey], [Website rules][site-rules]

The pinned Astro compiler requests maps for generated server JavaScript.
This request does not establish exact coverage of original template expressions.
Processed browser scripts and inline scripts use different paths.
Their original `.astro` line mappings remain unproven. [Compiler][astro-compile], [Virtual modules][astro-plugin]

This investigation inspected repository code and exact published dependency packages.
It ran no coverage fixture, browser journey, remote collection, or hosted upload.
The matrix distinguishes source-level capability from unproven behavior.
No exception receives approval. Production implementation remains outside this investigation.

The approved policy includes maintained production code, test code, and executable test helpers.
It requires original executable lines, disconnected zero-hit sources, supported branches, canonical paths, and first-party execution across process boundaries.
It requires failure on unexplained missing data.
Preserve Python architecture, combined LCOV, thresholds, and Codecov policy. [Membership decision][membership], [Investigation decision][investigation]

## Starting evidence and pins

The previous [mechanism research][mechanisms] separates instrumentation from disconnected-root selection.
The previous [evidence research][evidence] leaves browser and template guarantees unproven.
The [Node and Angular report][node-report] establishes native Node capabilities and silent omission paths.
Its [pending experiment][node-experiment] already addresses Node and Angular baselines, mappings, children, and missing output.
This report examines browser collection and Astro compilation beyond those boundaries.

Current main pins Astro `7.4.0`, `@astrojs/compiler-rs` `0.5.2`, and Playwright with `playwright-core` `1.64.1`.
The legacy `@astrojs/compiler` `2.13.1` serves other dependencies. Astro runtime compilation imports `@astrojs/compiler-rs`. [Lockfile][lock], [Compiler][astro-compile]

The inspected npm archives match their lockfile SHA-512 values:

```text
astro@7.4.0
sha512-6ThZU2bp7asGSceVcnh3r23dE88HpYN5vxtnCWYYrjr8lpQjx4eDW6/W8WgNlcc+T/b8zHmQ/EcS4DpQG4ZoOg==
playwright-core@1.64.1
sha512-BC4s8y6SEVA6Q1rSxZTto8hI1KUI3xXlHGZgvGqP4IqPdI/a6FX9H29IDNz7vKo9l21b6LfYosScPL6M3xxyDQ==
```

Package citations identify files inside these exact archives. [Astro archive][astro-package], [Playwright archive][pw-package]
The website browser rules select a digest-pinned Playwright worker.
Book Smartz worker documentation records Chromium `156.0.8078.4` and a matching Playwright image.
These declarations and historical observations do not establish this report's runtime version. [Browser rules][browser-rules], [Worker documentation][worker]

## Browser collection boundary

The website tests start Chromium through a shared Node harness.
The harness serves built HTML and assets. It creates pages and closes the browser without browser coverage collection.
The Book Smartz journey likewise starts Chromium without coverage collection.
Book Smartz declares maintained `submitting.js` separately. [Harness][harness], [Journey][journey], [Book Smartz rules][book-rules]

The missing API integration is a repository source observation.
It does not demonstrate lack of support in Playwright.
The native Node collector can observe the driver and Node helpers when its manifest includes them.
Node's inspector and `NODE_V8_COVERAGE` do not establish Chromium collection. [Node report][node-report], [Playwright package][pw-package]

In `lib/coreBundle.js`, `JSCoverage.start` enables the debugger and requests precise coverage with `callCount: true` and `detailed: true`.
It records parsed script identities and attempts to obtain script text.
Its `stop` method obtains function ranges and counts from Chromium.
It returns entries without source text when text retrieval fails.
Stopping an inactive collector returns an empty list. [Pinned implementation][pw-package]

The API returns generated JavaScript offsets. It does not itself emit original-source LCOV.
An adapter must preserve scripts, maps, and counts before browser closure.
It must convert ranges, normalize identities, and transfer records into the Bazel coverage action.
The official example uses `v8-to-istanbul` as a conversion step. [Browser API][pw-doc]

Default collection resets on navigation and excludes anonymous scripts.
Current official documentation warns that disabling reset does not guarantee retention across navigations.
A candidate adapter must preserve observations before destructive navigation or closure.
Frames, popups, and worker targets need separate checks when maintained tests execute first-party code there.
The inspected page collector supplies no complete target inventory. [Browser API][pw-doc], [Pinned implementation][pw-package]

Unrequested files cannot appear in a browser runtime profile.
Loaded zero-count ranges differ from disconnected sources that Chromium never loads.
An independent inventory and compiler-derived baselines remain necessary for disconnected originals.
A synthetic whole-file zero record does not establish exact executable lines. [Membership decision][membership], [Pinned implementation][pw-package]

## Astro regions and process boundaries

### Server scripts and templates

The website `build`, `fixtures_build`, and `timezone_build` targets run Astro through `js_run_binary`.
Browser tests consume output directories and serve static files.
Opening built HTML does not execute original frontmatter or server template expressions again.
A passing browser assertion cannot establish server execution during that browser test. [Website rules][site-rules], [Harness][harness]

`dist/core/compile/compile.js` calls `@astrojs/compiler-rs.transform` with the original filename and `sourcemap: "both"`.
`dist/vite-plugin-astro/compile.js` retains `transformResult.map` while appending module metadata.
This establishes a server-map candidate. It does not establish all executable line or branch mappings. [Compiler][astro-compile], [Compile adapter][astro-adapter]

`PostList.astro` contains frontmatter, a template conditional, callbacks, and expression interpolation.
Coverage needs original records for these executable expressions and their supported branches.
Static HTML, CSS, comments, and TypeScript declarations need language-based classification.
Generated renderer string literals do not establish that every markup line is executable. [Template example][post-list], [Membership decision][membership]

Two candidate paths need evaluation:

- Collect during a fresh instrumented render in a test action with controlled props and routes.
- Preserve profiles and maps from instrumented prerender actions through explicit dependencies into later coverage aggregation.

The second path needs a decision about build execution, action caching, and observation ownership.
A reused HTML artifact proves no fresh render or retained profile.
Astro's Container API can render a component with supplied props and slots.
It does not itself provide coverage instrumentation or exact template mappings. [Container API][container], [Website rules][site-rules]

### Processed browser scripts

Astro's virtual-module loader separates client scripts from server rendering.
In the server environment, it returns an empty client-script module.
For inline compiler script records, it returns `script.code` with TypeScript module metadata.
That return object has no separate `map` property.
This observation does not rule out embedded mappings or maps from later transforms. [Virtual modules][astro-plugin]

Astro's client build defaults `sourcemap` to false unless configuration overrides it.
The maintained website configuration supplies no such override.
Generic conversion of client chunks cannot establish original `.astro` positions from this configuration alone.
A candidate adapter must inspect compiler script metadata and the complete map chain. [Build configuration][astro-build], [Website configuration][site-config]

`ThemeToggle.astro` contains a processed script with conditions and event handlers.
Its original lines need browser observations and maps to positions inside the component.
A virtual path such as `?astro&type=script&index=0&lang.ts` must resolve to the canonical component path.
Do not publish that virtual module as another maintained source. [Theme toggle][theme], [Virtual modules][astro-plugin]

### Inline browser scripts

`AnimationBootstrap.astro` and `Layout.astro` contain `is:inline` scripts.
Astro leaves these scripts unprocessed, according to its script documentation.
Browser offsets can describe the served document instead of a standalone original file.
A mapping adapter needs original block positions and document identity.
The JSON-LD script in `Layout.astro` is data, not executable JavaScript. [Inline scripts][inline-doc], [Bootstrap component][animation], [Layout][layout]

Do not infer inline-script support from processed-script coverage.
Do not infer executable template coverage from browser script coverage.
Disconnected components and test fixtures need separate zero-hit evidence for every executable region. [Membership decision][membership]

## Capability matrix

“Source capability” means inspected code or official documentation supports a candidate mechanism.
“Unproven” means no preserved execution establishes the guarantee.
No capability below received a new coverage demonstration in this investigation.

| Source or boundary                    | Source capability               | Current gap                                  | Required evidence                              |
| ------------------------------------- | ------------------------------- | -------------------------------------------- | ---------------------------------------------- |
| Node drivers and executable helpers   | Native manifest collector       | Browser profiles remain separate             | Reuse Node experiment for driver records       |
| Chromium application JavaScript       | Playwright precise ranges       | No maintained collection calls               | Collect before closure and convert to LCOV     |
| Book Smartz `submitting.js`           | Declared original JavaScript    | Application URL differs from source identity | Known hits under one repository path           |
| Browser-evaluated test callbacks      | Browser executes injected code  | Original driver positions need maps          | Resolve original positions without duplication |
| Disconnected JS or TS                 | Inventory can select sources    | Browser never loads some sources             | Exact zero-hit executable baseline             |
| Astro frontmatter                     | Compiler requests server maps   | Separate prerender action                    | Fresh render with original line observations   |
| Astro template expressions            | Generated server-map candidate  | Exact lines and branches unproven            | Conditional, callback, interpolation fixture   |
| Astro processed script                | Client virtual modules          | Original block map chain unproven            | Browser hits at known `.astro` lines           |
| Astro inline script                   | Unprocessed browser script      | Document offsets need an adapter             | Block positions and source identity            |
| Disconnected `.astro` or test fixture | Compiler can process components | Exact baseline unproven                      | Compile without execution and check zeros      |
| Navigation and additional targets     | Target-specific API collection  | Whole-session completeness unproven          | Target and interval receipts                   |
| Branches                              | Detailed Chromium block ranges  | Language branch conversion unproven          | Known arms with stable `BRDA` identities       |
| Combined LCOV                         | Existing Bazel aggregation      | Browser transfer absent                      | Preserve native, server, and browser hits      |
| Broken collection                     | API calls can fail              | Missing text or inactive stop can omit data  | Reject missing expected artifacts              |

The matrix derives from [rules][site-rules], [browser implementation][pw-package], [compiler paths][astro-compile], and the [approved policy][membership].

## Adapter requirements and missing-data signals

A candidate browser adapter must register expected targets and collection intervals before execution.
It must preserve raw ranges, script text, maps, and completion receipts.
A missing receipt must fail coverage even when the functional test passes.
Intentionally blocked scripts still require eligible zero-hit records.

A candidate Astro adapter must distinguish server, processed client, and inline client regions within one original source.
It must retain maps and exact executable classifications for disconnected components and executable test fixtures.
Compilation must not execute a component merely to manufacture a zero baseline.
If compiler evidence cannot establish exact regions, require a bounded parser or instrumentation experiment before an exception.

Both adapters must normalize paths to repository-relative originals.
Explicit mappings must resolve runfiles prefixes, file URLs, HTTP origins, query strings, and generated chunks.
String removal alone cannot establish which source generated a chunk.
Merge repeated observations without replacing positive hits with baseline zeros.
Preserve distinct executable regions and supported branch identities. [Membership decision][membership]

Require failure for these cases:

- A required target or collection interval produces no receipt.
- A required script lacks text, maps, or a canonical original identity.
- Profiles disappear during navigation, termination, or process transfer.
- The collector returns empty output despite an executed sentinel.
- Corrupt ranges or conversion errors omit expected sources.
- An expected disconnected source lacks executable records or justified non-executable classification.

This report proposes these requirements. The current harness does not implement these checks.
Combined-report existence alone cannot prove these guarantees. [Harness][harness], [Coverage wrapper][coverage]

## Bounded follow-up experiment

Use one throwaway experiment for browser collection and Astro region mappings.
Do not repeat the pending Node and Angular experiment.
Preserve exact pins, source positions, commands, action/cache identities, profiles, maps, intermediate reports, and combined LCOV.

1. Create one component with frontmatter, a template conditional, interpolation, a callback, a processed script, and an inline script.
2. Add disconnected components, an executable test fixture, and disconnected JS or TS with known executable lines.
3. Include one Node test helper and one browser-evaluated helper.
4. Render controlled true and false cases in a fresh test action.
5. Compare fresh rendering with a browser test that only serves an existing build artifact.
6. Collect Chromium ranges before navigation and closure. Include another target only when the integration claims support for that target.
7. Inspect every map stage and identify exactly which original `.astro` regions retain positions.
8. Convert observations and compiler-derived zeros into canonical LCOV. Compare hits and branches before and after merging.
9. Include a Book Smartz script served through an application URL to check identity without Astro compilation.
10. Independently remove profiles, maps, text, or receipts. Corrupt one profile and interrupt one collection interval.
11. Require coverage failure for each missing-data case while functional assertions still pass.

Require `SF` with real `DA` records for known executable lines, including disconnected zeros.
Require supported `BRDA` records for controlled arms.
Require one original identity per source, with measured hits intact after merging server, browser, and zero observations.
Classify unmapped regions as unproven until a bounded experiment resolves them.
Approve no exception from missing evidence.

This experiment can establish local feasibility. Fresh configured-remote collection and hosted upload remain separate acceptance evidence.

## Validation scope

This report changes one Markdown file. It changes no production source or collector.
Gazelle does not apply because no source file changed.
Formatting, Markdown lint, strict prose checks, and whitespace checks examine the report separately from coverage feasibility.
Full repository build and full repository lint remain outside these report-only checks.

[ticket]: https://github.com/LowkeyLab/bazel-repo/issues/2443
[membership]: https://github.com/LowkeyLab/bazel-repo/issues/2417#issuecomment-6277250438
[investigation]: https://github.com/LowkeyLab/bazel-repo/issues/2438#issuecomment-6277423566
[mechanisms]: https://github.com/LowkeyLab/bazel-repo/blob/c277c511697f5d93b0fb7bcd1f3dcae3d573c848/docs/research/2026-10-18-coverage-inclusion-mechanisms.md
[evidence]: https://github.com/LowkeyLab/bazel-repo/blob/2b0cfa271ebfbe1dcc1701574883f61e62afb702/docs/research/2026-10-18-coverage-completeness-evidence.md
[node-report]: https://github.com/LowkeyLab/bazel-repo/blob/257619b1d73a6bc9ee577927bd5718ce3a5b4ff0/docs/research/2026-10-19-node-angular-collector-guarantees.md
[node-experiment]: https://github.com/LowkeyLab/bazel-repo/issues/2451
[lock]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/pnpm-lock.yaml
[harness]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/tests/browser/harness.ts
[journey]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/book_smartz/web/browser.journey.mjs
[site-rules]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/BUILD.bazel
[browser-rules]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/tests/browser/BUILD.bazel
[book-rules]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/book_smartz/web/BUILD.bazel
[site-config]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/astro.config.ts
[post-list]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/src/components/PostList.astro
[theme]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/src/components/ThemeToggle.astro
[animation]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/src/components/AnimationBootstrap.astro
[layout]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/personal_website/src/layouts/Layout.astro
[worker]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/book_smartz_browser_worker/README.md
[coverage]: https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/tools/coverage/coverage.sh
[pw-doc]: https://playwright.dev/docs/api/class-coverage
[pw-package]: https://registry.npmjs.org/playwright-core/-/playwright-core-1.64.1.tgz
[astro-package]: https://registry.npmjs.org/astro/-/astro-7.4.0.tgz
[astro-compile]: https://unpkg.com/astro@7.4.0/dist/core/compile/compile.js
[astro-adapter]: https://unpkg.com/astro@7.4.0/dist/vite-plugin-astro/compile.js
[astro-plugin]: https://unpkg.com/astro@7.4.0/dist/vite-plugin-astro/index.js
[astro-build]: https://unpkg.com/astro@7.4.0/dist/core/build/vite-build-config.js
[container]: https://docs.astro.build/en/reference/container-reference/
[inline-doc]: https://docs.astro.build/en/guides/client-side-scripts/
