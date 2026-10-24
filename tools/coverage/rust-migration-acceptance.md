# Rust coverage report migration acceptance

This record documents the verification scope in [ticket #2636](https://github.com/LowkeyLab/bazel-repo/issues/2636).
Complete local acceptance remains blocked by native JavaScript child attribution.
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

## Local execution record

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

## Results from the existing membership suites

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

### Native collector acceptance blocker

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
The native JavaScript attribution blocker remains unresolved.
Ticket #2636 and its parent remain open.

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

## Supplemental local runs

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
