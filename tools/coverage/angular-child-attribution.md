# Angular child coverage attribution

[Bug #2687](https://github.com/LowkeyLab/bazel-repo/issues/2687) records the approved correction to Angular child attribution.
The maintainer approved the required measured-count change under [specification #2632](https://github.com/LowkeyLab/bazel-repo/issues/2632).
The other compatibility requirements remain in effect.

## Cause and correction

Angular already enables `autoAttachSubprocess` in the retained Vitest V8 provider.
The worker and child inherit the correct `NODE_V8_COVERAGE` directory.
The raw child profile contains a positive range count before the provider consumes that profile.
Node resolves the child entry-point symlink to a physical path below `bazel-out`.
The provider excludes this path because it is outside the logical project root.
The combined report then contains an empty-script baseline for the child.

The repository applies `vitest_subprocess.patch` to `@vitest/coverage-v8@5.0.5` through `npm_translate_lock`.
The patch matches physical aliases to existing files that the provider's own roots and unchanged filters admit.
It uses the physical `realpathSync` operation that `rules_js` retains under `fs._unpatched`.
It changes only extended-context script URLs in memory, before the maintained provider merges and filters profiles.
Known logical URLs remain unchanged.
Ambiguous aliases and unmatched URLs remain unchanged.
The patch preserves raw files, ranges, counts, and source maps.
The maintained provider and Bazel `LCOV` merger still calculate the reports.
The Rust adapter, source scope, thresholds, inventory policy, reconciliation, and membership assertions remain unchanged.

## Before and after

The complete unchanged JavaScript membership command ran locally at both revisions.
All 11 Bazel commands returned zero in each run, including fixture cleanup.
The membership process failed the Angular child assertion before the correction.
The complete membership process passed after the correction.

| Check                                 | Before: `be6c77deab5caed98474b37dc142883256d8136d` | After: `399cdd882f48432f988c0164eaf7a7cbc7b30f1f` |
| ------------------------------------- | -------------------------------------------------- | ------------------------------------------------- |
| Raw Angular child range               | Offsets 0 through 63, count 1                      | Regression retains positive raw child ranges      |
| Angular child line counts             | `[0, 0]`                                           | `[1, 1]`                                          |
| Angular parent `app.ts` counts        | `[3, 2]`                                           | `[3, 2]`                                          |
| Angular unused counts                 | `[0]`                                              | `[0]`                                             |
| Native child counts                   | `[1, 1, 1]`                                        | `[1, 1, 1]`                                       |
| Native unused counts                  | `[0, 0, 0, 0]`                                     | `[0, 0, 0, 0]`                                    |
| Unique repository-relative identities | 372                                                | 373                                               |
| Membership result                     | Angular child assertion fails                      | Complete suite passes                             |

The new regression source adds one eligible file.
The source-selection policy remains unchanged.
Both executions cleared remote execution, remote cache, and build event service endpoints through an external Bazelisk wrapper.
The wrapper remained active inside the Nix shell and recorded each invocation.
All three selected measured tests ran without cached test results and used the `linux-sandbox` strategy.
The command logs contain no remote build-event stream or nonzero remote execution summaries.
Local action-cache reuse remained enabled.

## Regression and retained evidence

`//tools/coverage:vitest_subprocess_collector_test` runs the maintained provider through the public Vitest API.
It executes a real parent and child with the Bazel-supplied Node binary.
It checks a unique physical alias, ambiguous physical aliases, and an existing logical URL with ambiguous physical aliases.
It checks parent hits, child hits, unused zeros, unique identities, and exclusion of a foreign source.
It retains raw child profiles before the provider consumes them.
The final regression failed against the original pinned provider and passed against the patched provider.
The existing adapter and report suites passed, including required-output validation and report preservation after failure.
The full build, lint, format, and 181-test suite passed.
These remote checks remain separate from local membership acceptance.

The private evidence root is:

```text
/home/tacascer/.local/state/worktree-pool/agent-receipts/implement-2687-4eb8d73f-e1f7-4a40-8595-dbe4b4e55273
```

| Relative path                                                                 | Retained evidence                                                                    |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `before-evidence/`, `after-evidence/`                                         | Fixtures, commands, logs, revisions, build events, inventories, and combined reports |
| `before-angular-test-output/`, `after-angular-test-output/`                   | Native reports, outer profiles, functional test output, and test metadata            |
| `debug-profile-output/`                                                       | Child profile before provider consumption, with physical URL and positive range      |
| `effective-configuration.json`, `debug-worker-output/`, `debug-flush-output/` | Effective provider configuration, inherited environment, and child profile location  |
| `final-regression-red.log`, `final-regression-green.log`                      | Regression against the original and patched provider                                 |
| `final-regression-red-output/`, `final-regression-green-output/`              | Raw profiles and reports from the regression                                         |
| `acceptance-audit.json`                                                       | Counts, unique identities, command results, and local execution metadata             |
| `wrapper-invocations.tsv`, `local.bazelrc`                                    | Invocation record and cleared remote endpoints                                       |

## Remaining acceptance

Keep [#2636](https://github.com/LowkeyLab/bazel-repo/issues/2636) and [#2632](https://github.com/LowkeyLab/bazel-repo/issues/2632) open.
This correction completes the JavaScript membership suite for the selected native Node and Angular targets.
It does not establish broader Angular attribution guarantees.
Browser and Astro acceptance remain unverified under the [migration acceptance record](rust-migration-acceptance.md).
The earlier [native child correction](native-node-attribution.md) remains in effect.
