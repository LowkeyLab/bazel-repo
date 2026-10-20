# Browser collection and Astro original mappings

Issue: [Validate browser collection and Astro original-source mappings](https://github.com/LowkeyLab/bazel-repo/issues/2452).
Predecessor: [Research browser and Astro coverage guarantees](https://github.com/LowkeyLab/bazel-repo/issues/2443).
The predecessor supplied source inspection. This experiment adds executed observations.

## Result

Chromium collection works for the controlled page and the application script.
Fresh Astro rendering also produces measured original frontmatter and template branches.
The tested mapping chain does not establish a faithful original-source denominator.
The processed-script map ends at reformatted virtual JavaScript.
Server conversion assigns an executable statement to an HTML comment.
Compiler zeros and browser mappings also produce different branch regions for one helper.

The experiment demonstrates mechanism limits within the tested configuration.
They are not browser provisioning failures.
This research approves no exception or production mechanism.

## Effective inputs and scope

The experiment starts from `df390e4205234e7ea4d8b09be436739a23de27a2`.
Its effective packages are Astro `7.4.0`, `@astrojs/compiler-rs` `0.5.2`, Playwright `1.64.1`, and Node `24.15.0`.
The converter is `ast-v8-to-istanbul` `1.0.7`.
The merge library is `@vitest/istanbul-lib-coverage` `1.0.2`.
These packages come from the existing lockfile through Bazel.
The experiment adds no NPM dependency.

Chromium is `156.0.8078.4`.
Its archive SHA256 is `c3e50596aef52c58f8fda3c6b3411c33d34c88b177b3ed1c2243359fd3b8ba20`.
The browser uses pinned Nix libraries on the local host.
This test does not establish a portable remote browser image.
Only the top-level page is in scope.
Frames, workers, popups, and navigation persistence remain untested.

The [Container API](https://docs.astro.build/en/reference/container-reference/) renders imported components with controlled props.
The [Playwright coverage API](https://playwright.dev/docs/api/class-coverage) exposes Chromium coverage and optional script content.
This experiment uses CDP directly to retain script text and parse metadata.
The [Profiler protocol](https://chromedevtools.github.io/devtools-protocol/tot/Profiler/#method-startPreciseCoverage) defines detailed counter collection and interval retrieval.
The [Debugger protocol](https://chromedevtools.github.io/devtools-protocol/tot/Debugger/#event-scriptParsed) defines script identifiers, text retrieval, document offsets, and source-map URLs.
It does not test Playwright's coverage wrapper failure behavior.

## Executed paths

`site_build` invokes the locked Astro CLI through the maintained `js_run_binary` pattern.
It renders the fixture before the browser action.
The browser then serves that existing output.
The browser server does not import or render an Astro component.
Browser execution therefore supplies no fresh server-template measurement.

The separate `server` probe compiles each component with the effective compiler.
It records compiler maps and AST positions before rendering.
It converts zero ranges for all three components without importing them.
The disconnected component contains throwing frontmatter.
Its baseline succeeds without executing that throw.

The fresh render then imports `Main` and `TestOnly`.
It renders `Main` with both Boolean props and `TestOnly` with a true prop.
Node precise coverage records those two components and the Node helper.
The disconnected component remains absent from measured profiles.

The initial standalone probe omitted Astro's `resolvePath` callback.
That setup emitted `createMetadata`, which the selected runtime did not export.
The actual Astro build succeeded.
The aligned standalone probe also succeeded after it supplied the callback.
This setup error does not establish compiler/runtime incompatibility.
The [pinned compile wrapper](https://unpkg.com/astro@7.4.0/dist/core/compile/compile.js) supplies that callback.

## Mapping observations

The server compiler map retains original component text and positions.
The controlled template condition records branch counts `[1, 1]`.
The map also gives the generated return statement a range starting at the HTML comment.
The converter consequently reports that non-executable comment as a positive `DA`.
The interpolation has no distinct statement-start record.
This output cannot prove exact original executable-line membership.

The built processed script has a source map.
That map reaches the browser helper and an Astro virtual script.
Its virtual `sourcesContent` contains reformatted JavaScript, not the original `.astro` text.
The compiler's script entry supplies `type` and `code`, without a second script map.
The experiment preserves those positives as `Main.astro.virtual.js` in separate evidence.
It excludes that virtual record from canonical original-source LCOV.
It does not fabricate a second mapping stage.

The inline script text exactly matches one unique block in `Main.astro`.
The temporary adapter tests byte identity and shifts its positions to that original block.
CDP parse metadata independently retains document offsets.
This bounded exact-text bridge does not establish a general inline-script mapper.

The browser-evaluated helper uses an explicit source URL.
The adapter tests its exact declared text before it assigns the original identity.
The application route serves the exact `book_smartz/web/submitting.js` bytes.
It does not pass that script through Astro.
Its form submission sets `aria-busy` and disables the button before collection.

## Known original lines

The table uses the final fixture text.
Counts come from the canonical combined LCOV.
The oracle classifies these lines independently from compiler output.

| File and line              | Known role                             | Observed record                       |
| -------------------------- | -------------------------------------- | ------------------------------------- |
| `Main.astro:1,6`           | Frontmatter delimiters                 | No `DA`                               |
| `Main.astro:3,4,5`         | Frontmatter operations                 | `DA` count `2` each                   |
| `Main.astro:7`             | Non-executable HTML comment            | Incorrect positive `DA:7,2`           |
| `Main.astro:8`             | Template interpolation                 | No distinct `DA`                      |
| `Main.astro:9`             | Conditional template                   | `BRDA` counts `1,1`, no distinct `DA` |
| `Main.astro:10`            | Executed callback                      | `DA:10,4`                             |
| `Main.astro:11`            | Static button markup                   | No `DA`                               |
| `Main.astro:14,15`         | Processed script and handler           | Positive virtual records only         |
| `Main.astro:19`            | Inline conditional                     | `DA:19,1`, branch counts `1,0`        |
| `Disconnected.astro:2,3,5` | Unexecuted frontmatter and conditional | Zero `DA` records                     |
| `Disconnected.astro:6`     | Disconnected client script             | No original client zero record        |
| `disconnected.js:2`        | Disconnected executable return         | `DA:2,0`, branch counts `0,0`         |
| `TestOnly.astro:4`         | Test fixture conditional               | `DA:4,1`, branch counts `1,0`         |

The [pinned converter](https://unpkg.com/ast-v8-to-istanbul@1.0.7/dist/index.mjs) walks generated JavaScript AST nodes and maps their positions.
It does not independently classify original Astro template lines.
Its statement-start records explain the comment-line result in this fixture.
The source-mapped browser helper and its original zero baseline retain different branch locations.
The merged helper consequently contains two branch pairs, including an extra zero pair.
Both merge orders preserve these regions.
Equal merge orders do not establish correct region identity.

## Capability matrix

| Question                                 | Observation                                                      | Limit                                        |
| ---------------------------------------- | ---------------------------------------------------------------- | -------------------------------------------- |
| Fresh frontmatter and template execution | Both props render and template arms measure positive             | Comment line also enters the denominator     |
| Template callback                        | Callback measures positive                                       | No complete template statement inventory     |
| Processed script                         | Browser click passes and generated ranges measure positive       | Original Astro mapping stops at virtual text |
| Inline script                            | Functional result and exact-text original position pass          | Bounded block bridge only                    |
| Disconnected component                   | Compiler zero statement and branch records exist                 | Client block baseline is incomplete          |
| Executable test component                | Test-only render records a positive                              | No universal test-source discovery           |
| Disconnected JavaScript                  | Real zero `DA` and zero branches exist                           | Explicit fixture inventory only              |
| Node helper                              | Both controlled branch arms measure positive                     | Separate Node interval                       |
| Browser helper                           | Original line measures positive                                  | Baseline and remap branch regions differ     |
| Evaluated helper                         | Exact-text original identity measures positive                   | Explicit source URL only                     |
| Application script                       | Original script measures positive through its application URL    | Controlled form path only                    |
| Canonical merge                          | Eight original identities and both merge orders retain positives | Extra helper branch regions remain           |
| Virtual script                           | Separate positive records remain available                       | Does not count as an original source         |

## Failure controls

The browser starts precise coverage before navigation.
It captures coverage and script text before closure.
Each successful interval records a separate completion receipt.
An interrupted interval first passes its functional assertion.
Closing its page then causes the CDP collection command to reject.

The adapter independently tests six modified artifact sets:

1. Remove the required browser profile.
2. Corrupt the required browser profile.
3. Remove the required processed-script map.
4. Remove required script text.
5. Remove the completion receipt.
6. Supply the incomplete artifacts from the interrupted interval.

Each collector process exits `1` while the functional receipt remains true.
These tests belong to the temporary adapter.
They are not intrinsic guarantees from Astro, CDP, or the conversion library.
A valid but incomplete profile can still require an independent expected-source inventory.
This experiment does not prove arbitrary profile completeness.

## Reproduction and evidence

The executable fixture is in `tools/research_astro`.
The evidence directory is [browser-astro-2452](browser-astro-2452/).
It contains compiler results, raw browser ranges, script text, maps, intermediate coverage, canonical LCOV, and control outcomes.
Unresolved virtual coverage has a separate file.

Use the commands in [REPRODUCE.md](../../tools/research_astro/REPRODUCE.md).
Build actions use the configured remote service.
The browser test executes locally.
Cached actions, fresh remote work, local execution, and hosted CI remain separate evidence classes.
No hosted CI ran for this research branch.

Validation results and final input hashes appear in the evidence manifest.
The report retains the missing mapping and denominator observations.
No production application files or toolchain registrations change.

## Validation

Required Gazelle and full repository formatting passed.
The final server probe, browser test, merge probe, and six controls passed their experimental assertions.
The final browser test ran locally for `0.809` seconds with test-result caching disabled.
Its [BuildBuddy invocation](https://app.buildbuddy.io/invocation/6b7c0434-62d4-4077-a696-e4b8113d149b) records four executed remote build processes and 547 action-cache hits.
The earlier [site build](https://app.buildbuddy.io/invocation/5c05b274-8c66-4456-951c-697b1b9219b1) records four remote processes and 1078 action-cache hits.
These counts do not describe browser execution on the remote service.

Focused `aspect lint //tools/research_astro/...` passed with KeepSorted and no findings.
That result supplies no ESLint or ShellCheck evidence.
No repository-wide application build or hosted CI ran.
The strict prose test reports no hard findings.

The evidence manifest compares twelve final inputs with their executed artifacts.
All twelve source hashes match.
It also records the native compiler and Chromium executable digests.
The Nix library input uses nixpkgs `f20cd90e28fe0403873f473b29e42662cd86e8c4`.
The LCOV writer emits line and branch records.
Raw Istanbul JSON separately retains function metadata.
