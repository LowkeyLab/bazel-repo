# Native child coverage attribution

[Bug #2672](https://github.com/LowkeyLab/bazel-repo/issues/2672) records the approved correction to native Node child attribution.
The maintainer approved the required measured-count changes under [specification #2632](https://github.com/LowkeyLab/bazel-repo/issues/2632).
The other compatibility requirements remain in effect.

## Cause and correction

The retained `rules_js` collector uses c8 and a manifest filter.
The launcher preserves the parent entry-point path in the runfiles tree.
A plain Node child resolves its entry-point symlink to a physical output path.
The manifest filter excludes that physical path.
The collector then supplies an empty-script baseline for the child, despite its positive raw V8 count.

The repository applies `rules_js_runfiles.patch` to the pinned `rules_js` integration.
The patch updates its source and committed bundle.
It matches physical aliases only for existing manifest files.
It preserves manifest spelling as the report identity.
It leaves ambiguous aliases and unmatched URLs unchanged.

The integration supplies normalized profile copies to c8 before c8 merges counts or supplies zero baselines.
It changes matching script URLs and source-map cache keys, without reconstructing source maps or changing profile ranges.
The integration retains the original V8 files unchanged and removes its temporary copies after report generation.
The maintained collector and Bazel `LCOV` merger still calculate the reports.
The Rust adapter, membership assertions, inventory, thresholds, and scope remain unchanged.

## Before and after

The complete unchanged JavaScript membership command ran locally at both revisions.
All 11 Bazel commands returned zero in each run.
The membership process returned one in each run for the different assertions below.
The evidence directories retain the complete selection, including the later failure.

| Check                             | Before: `2f18101098bf4c9e182b6579fe6d24a776492af4` | After: `0e94708fa0fd81ecae360dc1d3dc6e0a6dc27180`                |
| --------------------------------- | -------------------------------------------------- | ---------------------------------------------------------------- |
| Raw native child range            | Offsets 0 through 63, count 1                      | Offsets 0 through 63, count 1                                    |
| Native child line counts          | `[0, 0, 0]`                                        | `[1, 1, 1]`                                                      |
| Native parent line counts         | Eight counts of 1                                  | Eight counts of 1                                                |
| Native unused line counts         | Four counts of 0                                   | Four counts of 0                                                 |
| Combined native child line counts | `[0, 0, 0]`                                        | `[1, 1, 1]`                                                      |
| Membership result                 | Failed native child-hit assertion                  | Native assertion passed, then Angular child-hit assertion failed |

The corrected native report replaces the empty-script fallback with the measured record.
Its child branch count is 1.
The maintained collector no longer reports the fallback function for that measured child.
The native TypeScript fallback remains subject to its existing original-manifest limit.

An independent report audit found 365 unique repository-relative source identities after the correction.
It checked parent hits, child hits, native unused zeros, hidden-source zeros, and Angular unused-source zeros.
The suite stopped after the Angular child assertion.
The remaining membership assertions remain unverified.

Both executions cleared remote execution, remote cache, and build event service endpoints with an external Bazelisk wrapper.
The wrapper remained active inside the Nix shell and recorded each invocation.
All three selected measured tests ran without cached test results.
They used the `linux-sandbox` strategy in each execution.
The command logs contain no nonzero remote action or remote cache summaries.
Local action-cache reuse remained enabled.

## Regression and retained evidence

`//tools/coverage:native_node_collector_test` executes a real parent and child through the Bazel-supplied Node binary.
It invokes the maintained collector through its manifest and V8 profile interface.
It checks measured parent and child hits, unused zeros, unique source identities, and unchanged raw profile bytes.
The final regression failed against the original pinned collector with `Child hits are absent.`
It passed against the patched collector locally and remotely.
The existing JavaScript report command suite also passed remotely, including required-output validation and report preservation after failure.

The private evidence root is:

```text
/home/tacascer/.local/state/worktree-pool/agent-receipts/implement-2672-acquire-_3z044j_
```

| Relative path                                                               | Retained evidence                                                                            |
| --------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `before-evidence/`, `after-evidence/`                                       | Fixtures, commands, logs, source revisions, build events, inventories, reports, and receipts |
| `before-native-test-output/`, `after-native-test-output/`                   | Raw V8 profiles, native `LCOV`, functional test output, and test metadata                    |
| `before-execution.instrumented_files`, `after-execution.instrumented_files` | Exact native manifests                                                                       |
| `before-combined-native.lcov`, `after-combined-native.lcov`                 | Bazel combined `LCOV` before adapter filtering                                               |
| `before-audit.json`, `after-audit.json`                                     | Counts, unique identities, local execution metadata, and `SHA-256` hashes                    |
| `after-angular-test-output/`                                                | Output and profiles for the later Angular failure                                            |
| `final-regression-red.log`, `final-regression-green.log`                    | Final regression against original and patched collectors                                     |
| `command-tests-3.log`                                                       | Successful remote collector and report command regressions                                   |

The initial remote regression attempts failed in fixture setup because they invoked the Node wrapper as the collector runtime.
The final fixture uses the raw toolchain binary, as the maintained collector integration does.
Those failed attempts remain separate from the final successful checks.

## Remaining acceptance

Keep [#2636](https://github.com/LowkeyLab/bazel-repo/issues/2636) and [#2632](https://github.com/LowkeyLab/bazel-repo/issues/2632) open.
The full suite now fails at `angular/projects/nicknamer/src/coverage-child-ynhdi1_k.mjs`.
The Angular child functional test passed, but its combined report still has no positive line count.
The cause of that separate failure remains unknown.
The Angular attribution guarantees remain unchanged.

Browser and Astro acceptance remain unverified under the restrictions in the [migration acceptance record](rust-migration-acceptance.md).
These tests preserve website and browser behavior.
Command tests, local membership execution, remote regression tests, and hosted checks remain distinct evidence.
