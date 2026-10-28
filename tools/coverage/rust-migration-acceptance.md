# Rust coverage report migration acceptance

This record documents the verification scope in [ticket #2636](https://github.com/LowkeyLab/bazel-repo/issues/2636).
Native Node and Angular child attribution corrections are complete.
The complete local website selection and report checks passed.
Nineteen Astro originals retain explicit unknown-coverage limits.
The parent is [specification #2632](https://github.com/LowkeyLab/bazel-repo/issues/2632).
The [accepted architecture decision](../../docs/adr/0003-use-rust-for-native-coverage-reports.md) remains authoritative.

## Architecture and compatibility

The retained `go_report`, `jvm_report`, and `javascript_report` targets are Rust binaries.
The Go command retains its Rust reporting mode.
All three binaries use the `coverage_reports` library.
Its shared build-event module owns decoding, event selection, named-set traversal, artifact locations, and artifact tests.
Adapters consume this interface and retain their collector rules.

| Adapter    | Artifact policy and retained limits                                                                                                                                                                                                                       |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Go/Rust    | Workspace paths, `SHA-256`, byte lengths, baseline identity, aspect exclusion, and incomplete-group rejection. Rust executable-summary and child-profile limits remain explicit.                                                                          |
| JVM        | Local artifact handling, configured inputs, class/profile identity, source ownership, and standard JaCoCo analysis. Branch and child-profile limits remain explicit.                                                                                      |
| JavaScript | Restricted local-file URIs, configured input selection, Node profiles, browser archives, and lexical original-source matching. UTF-16 offsets, exact original content, Angular omissions, child profiles, and Astro limits retain their existing meaning. |

The Java analyzer, JavaScript collectors, and Bazel `LCOV` merger remain in use.
Inventory, reconciliation, baselines, and membership harnesses retain their existing implementations.
No Python production report implementation or Python-to-Rust report protocol remains.
The three Python command regression suites execute the Rust commands.
Rust tests verify the shared interface with `googletest`.

## Comparisons before replacement

| Family     | Python source revision                     | Passing comparisons | Retained run                                                                                     |
| ---------- | ------------------------------------------ | ------------------: | ------------------------------------------------------------------------------------------------ |
| Go/Rust    | `faa963828aed4a3a8273429937791bfdb1daa1a9` |                  44 | [Command comparisons](https://app.buildbuddy.io/invocation/abb8616e-8d5f-419f-8e40-7728a38832c2) |
| JVM        | `3c4a0dd2b1e7e868eafa8c73312cbcedab41258d` |                  37 | [Command comparisons](https://app.buildbuddy.io/invocation/7db89ae7-8bcb-4fcb-8c0f-c3cc470acc33) |
| JavaScript | `40fcc7a897f74566e8a26a8794529ef3d93dec08` |                  65 | [Command comparisons](https://app.buildbuddy.io/invocation/326aa7fc-7f0a-4642-9718-912463533498) |

Each family compared successful and rejected commands before removing its Python implementation.
The tests compared reports, receipts, exit statuses, and report preservation after failure.
The tests normalized only incidental JSON formatting and temporary paths where necessary.
The JVM record contains 14 successful cases and 23 rejected cases.
The JavaScript record contains 22 successful cases and 43 rejected cases.
Real collector execution requires separate evidence.

## Repeated JVM evidence

Local acceptance exposed a migration defect in retained evidence.
Rust `fs::copy` copied read-only artifact permissions.
The second report command failed to replace those retained files.
Python `shutil.copyfile` copied contents without copying those permissions.
The Rust adapter now copies contents without copying permissions.
This restores the existing repeated-command behavior.

The new command regression uses a read-only baseline and the real analyzer jar.
It requires two successful report invocations with one evidence directory.
It tests report bytes and a completed receipt.
The regression failed locally before the correction.
The full JVM command suite passed locally after the correction.
The initial remote regression passed despite read-only outputs.
The regression now verifies write permission on retained artifacts too.

## Historical local execution record

The acceptance source revision is `3db05d0933c212a822cf08ec61ec0893b6c10b8e`.
Evidence uses private runtime directories outside the checkout.
The command records identify the exact entry points, arguments, times, and exit statuses.
Reports, receipts, profiles, build events, and fixture sources remain separate from this document.

The initial Go attempt failed during JVM report generation.
A later attempt inherited remote settings after the Nix filesystem launcher reset `PATH`.
Neither attempt establishes local membership acceptance.
Their evidence remains separate from the verified runs.

The verified runs use the [Bazelisk wrapper hook](https://github.com/bazelbuild/bazelisk#toolsbazel) inside the Nix filesystem environment.
The hook supplies `--nohome_rc` and an external Bazel configuration file for this task.
That file clears `--remote_executor`, `--remote_cache`, `--bes_backend`, and `--bes_results_url`.
The hook records every invocation and remains outside the checkout.
The acceptance harnesses remain unchanged.

## Historical results from the existing membership suites

All four suites ran with source revision `3db05d0933c212a822cf08ec61ec0893b6c10b8e`.
The following table distinguishes command exit statuses from the suite result.
Expected failures count as passing tests only when the harness accepts their diagnostics.

| Suite      | Bazel commands | Result                                                                                                                                                                                                                                                                                          |
| ---------- | -------------: | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Go         |             34 | Passed. The suite accepted 28 successful commands and six expected failures. It verified ordinary and hidden sources, configured baselines, measured hits, and child execution.                                                                                                                 |
| Rust       |             14 | Passed. Thirteen commands succeeded and one failed as expected. Thirteen separate native collector cases verified profiles and objects. Eleven cases failed as expected. The valid case and documented missing-child-profile limit completed successfully.                                      |
| JVM        |             28 | Passed. The suite retained baseline, excluded-language, measured Java, and Kotlin reports. It verified child hits, configured ownership, class identity, and source limits. Failure behavior has separate command-regression evidence.                                                          |
| JavaScript |             11 | Failed. The Bazel commands succeeded, including functional tests and coverage generation. The harness then rejected zero measured hits for the native Node child. The assertion stopped the suite before its remaining validations. The final command regenerated `BUILD` files during cleanup. |

The action audit found no remote execution or remote cache hits in these suites.
Existing local action and test caches remained available.
The measured Go fixture executed freshly with `linux-sandbox`.
The Rust collector retained 24 raw profile files across valid and damaged cases.
The JVM reports retained four raw execution profiles.
Receipts retain source-specific limits instead of treating unknown coverage as measured zero.

### Historical native collector failure

The Node child executed successfully and printed its expected output.
Its raw V8 profile records one execution for the complete child script.
The native collector emitted zero line hits for that same source before report merging.
The membership assertion therefore failed for `coverage_js_04mp10nq/child.mjs`.

A separate replay used the retained native combined report and restored fixture sources.
It ran the exact Python command from revision `40fcc7a897f74566e8a26a8794529ef3d93dec08`.
It then ran the Rust command with the same initial report.
Both commands completed successfully and produced identical report bytes.
Their report `SHA-256` is `f1ec0201560e29f8e1409f4c4122397f2fa30873efd757e771de7f423038cb06`.
Both reports retain zero hits for all three child lines.
This establishes an existing collector attribution problem on this local execution path.

The first replay used the already-filtered final report.
Both validators rejected its missing TypeScript record and preserved that report.
That replay supplies rejection evidence only. Successful compatibility requires the valid input replay.
The successful replay used the native combined report, which retained the required original records.
All replay fixtures and the temporary Python command remain outside the checkout after cleanup.

Changing collector attribution changes coverage counts.
[Specification #2632](https://github.com/LowkeyLab/bazel-repo/issues/2632) requires separate approval before changing existing behavior.
The JVM permission correction restores the Python baseline behavior.
The historical native Node failure preceded the approved correction in [#2672](https://github.com/LowkeyLab/bazel-repo/issues/2672).
The later Angular child failure preceded [#2687](https://github.com/LowkeyLab/bazel-repo/issues/2687).
Both corrections are complete.
Those historical failures required separate approved repairs.

## Retained evidence

The private evidence directory is
[/home/tacascer/.local/state/worktree-pool/agent-receipts/implement-2636-acquire-l2gpk8ye](/home/tacascer/.local/state/worktree-pool/agent-receipts/implement-2636-acquire-l2gpk8ye).
These files remain on the workstation. Git excludes the evidence.

| Record                                                 | Contents                                                                          |
| ------------------------------------------------------ | --------------------------------------------------------------------------------- |
| `verified-local-acceptance-results.json`               | Entry points, arguments, source revision, times, and suite exit statuses.         |
| `local-execution-audit.json`                           | Action strategies, effective endpoints, test cache status, and report identities. |
| `*-verified-local-evidence/`                           | Per-command logs, build events, inputs, reports, profiles, and receipts.          |
| `rust-verified-local-evidence/collector-results.jsonl` | Native collector cases and their expected failure results.                        |
| `real-javascript-comparison-raw/result.json`           | Successful Python/Rust replay results, report hashes, and child counts.           |
| `real-javascript-comparison/result.json`               | The rejected final-report replay and its explicit input limitation.               |
| `wrapper-invocations.tsv`                              | Audited nested Bazel invocations.                                                 |
| `regression-red-local.log`                             | The locally failing JVM permission regression before correction.                  |
| `regression-green-local.log`                           | The complete JVM command suite after correction.                                  |

The Go harness performs fault tests after its measured run.
Those tests reuse Bazel output paths and can replace earlier raw test outputs.
The harness retains its measured report and build events before the fault tests.
A separate measured Go run retains raw output before any later test can replace it.
Post-fault files remain labelled as post-fault evidence.

## Historical supplemental local runs

The maintained `//predix/internal/domain/circle:circle_test` ran with `--nocache_test_results`.
The complete coverage command succeeded.
The test executed freshly in `linux-sandbox`.
Its native collector output contains measured hits.
The evidence retained that output before another test could reuse its path.
The output `SHA-256` is `1f717bcbb993736e404ec69a99e5c034ae1e6d2b9f47eda1a7168bd72e15a4e6`.
This file is the collector's pre-merge `LCOV` output.

The complete `//personal_website/...` selection ran locally with `--keep_going` and `--nocache_test_results`.
The first run lacked Playwright's downloaded browser.
The second run supplied the Nix Chromium executable with `--test_env=CHROMIUM_EXECUTABLE`.
Both runs executed all ten tests freshly in `linux-sandbox`.
Both had five passing tests and five failing tests.
The second run reached these local failures:

| Target                                               | Failure                                                                                      |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `personal_website:personal_website_test`             | `TestCacheHeaders` found no generated style sheet on the landing page.                       |
| `personal_website/tests/browser:recent_writing_test` | The `complete entries stay hidden and untabbable until heading completion` assertion failed. |
| `personal_website/tests/browser:hero_test`           | The page wait timed out.                                                                     |
| `personal_website/tests/browser:featured_work_test`  | The page wait timed out.                                                                     |
| `personal_website/tests/browser:homepage_test`       | Both late-module cases timed out in the font-watchdog assertion.                             |

The coverage command returned exit status 3 after the test failures.
The test failures stopped the command before report publication.
These results preserve the distinction between functional tests and report validation.
Complete browser and Astro acceptance remains unverified.
Astro attribution limits remain unchanged.

The `go-profile/` directory retains the successful supplemental Go command, report, receipts, and native output.
The `website-browser/` and `website-browser-chromium/` directories retain both complete selections and their failure logs.
The `supplementary-execution-audit.json` file records action strategies and fresh test execution.
These audits found no remote execution or remote cache hits.

## Reconciliation on 2026-10-29

The inspected merged source is `01e655c9f38a9984ae595d699ac08d985dd272a0`.
The report library, three adapters and entry points, and four membership harnesses still match the historical acceptance source bytes.
The audit rechecked every retained report hash in the family record.
It confirmed 24 Rust raw profiles and four JVM execution profiles.
The three Python command regression suites remain present.
Their former Python production implementations remain absent.
The accepted shared-library architecture and explicit artifact policies remain unchanged.

The historical Go, Rust, and JVM results retain their original revisions and execution limits.
Their command totals remain 34, 14, and 28 respectively.
This source and artifact audit does not repeat those collector executions.
Later dependency pins and inventory additions remain outside those historical execution claims.

### Corrected native Node and Angular membership

The [Angular correction record](angular-child-attribution.md) supplies the passing complete JavaScript membership result for its selected targets.
Its source revision is `399cdd882f48432f988c0164eaf7a7cbc7b30f1f`.
All 11 commands passed, with fresh native Node and Angular tests using `linux-sandbox`.
The report contains 373 unique repository-relative source identities.
Angular child counts are `[1, 1]`. Native child counts remain `[1, 1, 1]`.
Unused and hidden-source zero baselines remain explicit.
The retained audit and report evidence remain available.
This result covers the selected native Node and Angular targets.
The complete website selection has separate evidence below.

### Approved website repairs

[#2828](https://github.com/LowkeyLab/bazel-repo/issues/2828) removed a redundant Linux symlink lookup from the pinned Go bootstrap wrapper.
[PR #2837](https://github.com/LowkeyLab/bazel-repo/pull/2837) merged that repair as `0eaa5a37a94b1baaf77b2b9d3be3ea11a1bf770e`.
The wrapper previously required undeclared `readlink` and `dirname` commands in an action without PATH.
Linux Go already resolves its executable through `/proc/self/exe`.
The real local bootstrap passed after the repair.

[#2829](https://github.com/LowkeyLab/bazel-repo/issues/2829) corrected generated stylesheet lookup and paused-clock browser-test setup.
[PR #2846](https://github.com/LowkeyLab/bazel-repo/pull/2846) merged it as `01e655c9f38a9984ae595d699ac08d985dd272a0`.
Astro recorded sandbox page paths while Vite resolved the same files to execution-root paths.
The pinned dependency patch preserves both identities for page lookup.
All three controlled-clock featured-work scenarios also wait for component registration and advance their zero-delay mount timer.
Every existing assertion remains unchanged.
The original five-failure selection and two later failed clean-head runs remain retained.

### Complete local website selection

The successful source revision is `6b7d406d126aa5d1f05c0fba8b1486352c22665b`.
Its full Git tree matches merged source `01e655c9f38a9984ae595d699ac08d985dd272a0`.
The source was clean during the run.
The existing `//tools/coverage` entry point selected all of `//personal_website/...`.
It supplied `--keep_going`, `--nocache_test_results`, and `--test_env=CHROMIUM_EXECUTABLE`.
The unchanged ten-test selection passed freshly through `linux-sandbox`.
The complete coverage command returned zero and generated the combined report and collector receipts.

The external Bazelisk hook cleared remote execution, remote cache, and build event service endpoints for nested invocations.
The retained audit checked the wrapper, effective command options, local runners, and fresh test results.
Existing local action caches remained available.
No remote execution or remote cache result establishes this local acceptance claim.

The artifact audit checked 46 page profiles and 156 source maps.
It checked profile and report hashes, supplied script bytes, map contents, and exact original TypeScript source content.
The six observed originals have these measured line counts:

| Original under `personal_website/src/`   | Lines with hits | Executable lines |
| ---------------------------------------- | --------------: | ---------------: |
| `components/featured-work-animation.ts`  |              18 |               18 |
| `components/hero-animation.ts`           |             153 |              166 |
| `components/recent-writing-animation.ts` |              44 |               49 |
| `utils/animation-observation.ts`         |              87 |               94 |
| `utils/handwriting.ts`                   |             222 |              231 |
| `utils/section-animation.ts`             |             205 |              205 |

These counts describe the selected local run.
Existing coverage thresholds remain unchanged.
Nineteen Astro originals retain source-specific absence limits with owners, evidence locations, and missing guarantees.
Their coverage remains unknown.
Browser reports do not measure Astro's build-time template execution.
The collector adds no processed-script bridge or replacement source map.
Functional test completion and report validation remain separate evidence.

### Private reconciliation records

Resolve receipt locations beneath `${XDG_STATE_HOME:-$HOME/.local/state}/worktree-pool/agent-receipts`.
The following locations remain outside reusable Git checkouts:

| Receipt directory                                     | Relevant records                                                                                                                          |
| ----------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `implement-2636-fefdaf25-e317-4729-aa93-8e7972d0edd9` | `reconciled-evidence-audit.json`, `retained-family-audit.json`, `corrected-js-evidence.json`, and the earlier failed website selections.  |
| `implement-2828-083f2ac4-0760-4e00-9eae-16104d97ab06` | Real bootstrap failure and fresh local success, actual-wrapper regression results, and intermediate complete website runs.                |
| `implement-2829-5f1cd5a9-37c7-4a36-b559-f33ac18fb3d9` | `website-browser-rebased-local/`, source continuity, failed-run preservation references, source reviews, and merge verification receipts. |

The successful website directory retains the exact command, source hashes, build events, test outputs, profiles, maps, and report copies.
`acceptance-audit.json` records profile, source-map, original-source, and report checks.
`nested-execution-audit.json` records the wrapper and endpoint checks.
Earlier receipt locations in this document remain historical evidence.

### Acceptance reconciliation

| Requirement                                     | Evidence and boundary                                                                                                                                          |
| ----------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Three complete Rust commands and one library    | Architecture inspection confirms the retained entry points and shared `coverage_reports` library.                                                              |
| Shared event interpretation and family policies | `BuildEvents` retains explicit workspace-integrity, JVM-local, and JavaScript-local artifact handling.                                                         |
| Comparisons before Python removal               | The retained 44 Go/Rust, 37 JVM, and 65 JavaScript comparisons include successful and rejected commands.                                                       |
| Retained Python regression suites               | All three suites execute the Rust binaries. Python production report implementations and the report protocol remain absent.                                    |
| Real local Go, Rust, and JVM membership         | Historical complete suites and retained artifact hashes remain valid at their recorded source revisions.                                                       |
| Corrected native Node and Angular membership    | The complete selected suite passed after the approved attribution corrections.                                                                                 |
| Complete website selection and reports          | All ten local tests passed. Profiles, maps, reports, receipts, and source identities passed their separate audit.                                              |
| Browser and Astro limits                        | Nineteen Astro originals retain explicit unknown coverage. Build-time template execution remains outside measured browser coverage.                            |
| Preserved contracts                             | Scope, collectors, assertions, thresholds, baselines, inventory, reconciliation, harnesses, and attribution guarantees remain unchanged by this documentation. |
| Validation categories                           | Command regressions, local membership, configured execution, and hosted checks remain distinct.                                                                |

Completion of #2636 requires validated publication of this record.
The parent specification #2632 retains its separate closure reconciliation.
