# Supplemental configured-remote JVM validation

The existing JVM fixture passed a fresh configured-remote test. The downloaded artifacts satisfy the original expected-record checks. This result adds remote execution evidence. It does not resolve duplicate-source ownership or stable branch identities.

The user approved the research fixtures and build artifacts sent to `remote.buildbuddy.io`. This supplement follows the closed [research ticket 2480](https://github.com/LowkeyLab/bazel-repo/issues/2480). It preserves the scope and limits in the [original report](README.md).

## Revision and execution boundary

The separate pool assignment acquired `origin/research/jvm-2480` at `16255b4848eb147b7de5f34bab846e23f3e9b7b8`. Acquisition also refreshed `origin/main` to `39075c1280f357d58b06a9a471e617cd73949bdf`. The supplemental branch is `research/jvm-2480-remote`. The fixture sources, pins, and expected records remain unchanged.

[BuildBuddy invocation e729bea4-048e-4bd0-9e66-d6b8767fddbf](https://app.buildbuddy.io/invocation/e729bea4-048e-4bd0-9e66-d6b8767fddbf) passed. The [BEP extract](remote-evidence/remote-evidence.json) records `fixture_test` as `PASSED` with execution strategy `remote`. The command disabled test-result reuse and local fallback.

The invocation records 202 actions: 119 remote cache hits, 81 internal actions, two local actions, and one remote execution. The two local actions have mnemonic `CoverageReport`. This result does not claim fresh compiler execution for cached actions.

The [execution summary](remote-evidence/action-execution-summary.json) contains two `TestRunner` records. One records a remote cache hit. The other records `runner: remote` and `cacheHit: false`. The fresh record also has `remoteCacheable: false`. The final BEP test result records remote execution.

The remote test executes the parent assertions and its child process. The native coverage reports run locally. The standard JaCoCo launcher and ownership inspector also run locally against the downloaded remote products. They do not run on BuildBuddy in this supplement.

## Exact record checks

The [check result](remote-evidence/expected-record-checks.json) verifies all three raw analyses against the unchanged [expected lines](remote-evidence/expected-lines.json) and [expected branches](remote-evidence/expected-branches.json).

- All 11 classes retain the expected 43 executable lines.
- The empty-data analysis has zero covered instructions.
- Parent data gives `kprobe/ExecutedKotlinKt:3` one covered instruction out of one.
- Child data gives `probe/Child:7` four covered instructions out of four.
- Parent data gives that child-only line zero covered instructions.
- Both disconnected classes remain zero.
- The controlled Java and Kotlin conditions each retain one covered branch out of two.
- Both partial line-merge orders have identical bytes.
- Missing child data, corrupt child data, and missing class metadata each return exit 1.

The [raw empty](remote-evidence/core-empty.tsv), [parent](remote-evidence/core-parent.tsv), and [child](remote-evidence/core-child.tsv) records preserve class identities. The native [parent LCOV](remote-evidence/native-parent.lcov) still reports `DA:3,0` for the inline body. Standard JaCoCo analysis still reports its positive original-line hit.

The [ownership result](remote-evidence/automatic-ownership.json) retains nine unique owners and two ambiguous classes. Both ambiguous classes still have two `Twin.kt` candidates. The lookup uses the unchanged Bazel owner metadata from the original research branch. This supplement does not claim a new compiler mapping.

The [forward](remote-evidence/merged-forward-partial.lcov) and [reverse](remote-evidence/merged-reverse-partial.lcov) reports contain nine canonical paths. They remain partial diagnostic reports. They contain no invented branch identities. The collector decision remains open.

## Commands and artifacts

The [exact coverage command](remote-evidence/coverage-remote.command.txt) uses the configured service endpoints. It sets `--cache_test_results=no`, `--noremote_local_fallback`, `--remote_download_outputs=all`, and `--remote_upload_local_results=true`. Resource bounds are four jobs, two local CPUs, 2048 MiB of local memory, and a 1536 MiB Bazel JVM heap.

The command requests JSON action logs with `--execution_log_compact_file=` and `--execution_log_json_file=<private>/execution.json`. Raw BEP and execution logs remain private because configured command metadata can contain credentials. The published summaries omit command arguments and environment variables.

The [input manifest](remote-evidence/input-sha256.json) identifies the six downloaded class jars and separate parent/child execution files. The [artifact manifest](remote-evidence/artifact-sha256.json) identifies all supplemental records. All six downloaded jars match the prior local jar bytes. See the [jar comparison](remote-evidence/jar-comparison.json).
Execution files retain their actual session data. The supplement does not replace timestamps with zero.

Use the original [analysis sequence](REPRODUCE.md) with `research/jvm-2480/remote-evidence` as the evidence directory. Disable remote endpoints for the local analysis launches. The raw records and each control retain stdout, stderr, and exit status.

No source implementation changed. Whole-tree formatting passed with local-only endpoints and Java 21 formatter settings. Strict English lint passed for this supplement. The existing source lint results remain those in the original report. This supplement did not repeat whole-repository build, test, or lint acceptance.

The BuildBuddy invocation supplies hosted build-event evidence. This result does not establish GitHub CI, Codecov upload, or a complete coverage adapter. No PR, merge, policy change, or exception approval follows from it.

## Pool receipts

Catalog authority: `de2f76d2-3cb1-4050-838e-9618087569c6`.
Repository: `207f1904-1165-417f-a4f0-399e8498ed22`.
Worktree: `0bed8bf4-6e60-4bdd-8edb-fde22454256a`.
Assignment: `1c7ab439-966a-4e08-8a87-5859466715b5`.
Acquisition operation: `29dad399-500a-45ce-af06-bdd844cb5766`.
Checkout: `/home/tacascer/.local/share/worktree-pool/worktrees/207f1904-1165-417f-a4f0-399e8498ed22/0bed8bf4-6e60-4bdd-8edb-fde22454256a`.

Private receipts are under `/home/tacascer/.local/state/worktree-pool/agent-receipts/`: `jvm-2480-remote-catalog-WNixpJ`, `jvm-2480-remote-acquire-YfE0Zg`, and `jvm-2480-remote-validation-tmKpYo`. Catalog inspection, acquisition, and remote coverage each completed with exit 0. The final handoff records release separately.
