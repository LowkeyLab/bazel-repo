# Supplemental configured-remote Askama validation

All five existing Askama research targets passed with fresh remote test results. Downloaded outputs match the original bounded expectations. Supported syntax and unresolved mapping requirements remain unchanged.

The user approved the research fixtures and build artifacts sent to `remote.buildbuddy.io`. This supplement follows closed [ticket 2485](https://github.com/LowkeyLab/bazel-repo/issues/2485). It preserves the scope and limits in the [original report](README.md).

## Revision and direct execution evidence

A new pool assignment acquired `origin/research/askama-2485` at `6f1e5ae04f8373ced896ab34cde84cb9194a2528`. Acquisition also refreshed `origin/main`. The supplemental branch is `research/askama-2485-remote`. No fixture, adapter, collector, dependency, or BUILD source changed.

[BuildBuddy invocation b20b4bae-14f9-45b0-ac84-9d459a73f104](https://app.buildbuddy.io/invocation/b20b4bae-14f9-45b0-ac84-9d459a73f104) returned exit 0. Final BEP results record `PASSED` and strategy `remote` for all five targets:

| Existing target              | Final strategy | Direct runner | cacheHit |
| ---------------------------- | -------------- | ------------- | -------- |
| `maintained_native_test`     | `remote`       | `remote`      | `false`  |
| `syntax_test`                | `remote`       | `remote`      | `false`  |
| `whitespace_comparison_test` | `remote`       | `remote`      | `false`  |
| `subset_comparison_test`     | `remote`       | `remote`      | `false`  |
| `subset_collection_test`     | `remote`       | `remote`      | `false`  |

Each target has direct TestRunner records with exitCode 0. The JSON log contains two records per target. This report uses the five final BEP results as the test count. It does not infer ten independent attempts from those duplicate entries.

Bazel reports 1420 remote cache hits, 671 internal entries, and 23 remote entries. These reported counters are not a disjoint action partition. The action log also records remote Rustc and AskamaResearchInstrument entries. Compilation therefore includes both cache reuse and remote execution.

The command uses `--config=ci-remote`, `--cache_test_results=no`, `--noremote_local_fallback`, `--remote_download_outputs=all`, and `--remote_upload_local_results=true`. It bounds jobs to four, local CPU capacity to two, local memory capacity to 2048 MiB, and the Bazel JVM heap to 1536 MiB. Raw BEP and direct JSON action records remain private.

## Downloaded expected-record checks

Local inspection of downloaded products reproduces the original results:

- The native test passes 21 render cases across all 16 maintained templates.
- Transformation accepts eight maintained files and rejects eight.
- Four additional unsupported-syntax controls retain their expected failures.
- The old adapter changes all four whitespace renders. The corrected adapter matches native bytes in all four cases.
- Native and instrumented table renders match in both empty and populated cases.
- The observed LCOV matches the existing expected file exactly across eleven executable lines and three source identities.
- Unused lines and branches remain zero. Both supported Boolean arms retain one hit.
- Both merge orders have identical bytes.
- Five mapping/completion controls each return exit 1 after functional rendering succeeds.
- Maintained source hashes and snapshot bytes match the original inventory.

The five controls cover missing included-template metadata, a changed source hash, an unknown event, incomplete completion, and a corrupt event hash. The remote test executes these controls. The later local step only reads and checks downloaded records.

Inheritance, block overrides, if-let, conditionals without else, general condition spans, and loop branch semantics remain unresolved. This supplement does not expand syntax support or select a production collector. Unsupported syntax remains visible. No coverage exception follows from these results.

## Private receipts and selected hashes

Publication changes only this Markdown report and its index link. Raw logs, test outputs, HTML, profiles, damaged inputs, and archives remain private. Existing published evidence remains unchanged.

| Private artifact              | SHA256                                                             |
| ----------------------------- | ------------------------------------------------------------------ |
| Direct runner/cacheHit proof  | `8ae45e7d8c8b6f095415e230a8a7f21a16a9525e99d5b9f5575c96fb524109e9` |
| Expected-record check summary | `ff543000dbfb7f8f27a22e355d0f4a7be2f463f06b1274e2e34efe42c78ee140` |
| Observed combined LCOV        | `35b3261463e3800a08173d1ef64166705f871f7cf5e7fe3ff100ac41bc813609` |
| Original-source events        | `f66ebb3088f787cdfa32918c57483b0e5b0c1eb8640af36425dd3b8141cf8961` |
| Syntax outcome matrix         | `cd2b08e0aa4c8e2f70abc71752350443e92fed944ee0e4f719daf2fef6fff71c` |

The runtime directory is `/home/tacascer/.local/state/worktree-pool/agent-receipts/askama-2485-remote-validation-913Vxv`. It retains raw stdout, stderr, exit status, BEP, action JSON, downloaded outputs, direct runner proof, and expected-record checks.

Whole-tree formatting and strict English checks passed for this supplement. No implementation changed. The original source lint limits remain unchanged. This result does not establish whole-repository acceptance, GitHub CI, or Codecov upload. It creates no PR, merge, production policy change, or map edit.

## Pool identities

Catalog authority: `de2f76d2-3cb1-4050-838e-9618087569c6`.
Repository: `207f1904-1165-417f-a4f0-399e8498ed22`.
Worktree: `0bed8bf4-6e60-4bdd-8edb-fde22454256a`.
Assignment: `7fbcd834-181e-405a-a407-a32fca49199a`.
Acquisition operation: `ae0c19b2-0785-4ba6-99b8-b2eb2b13525c`.
Checkout: `/home/tacascer/.local/share/worktree-pool/worktrees/207f1904-1165-417f-a4f0-399e8498ed22/0bed8bf4-6e60-4bdd-8edb-fde22454256a`.

Catalog and acquisition receipts are `askama-2485-remote-catalog-0sq0Ui` and `askama-2485-remote-acquire-Aew6jl` under the same private receipt root. Both commands completed with exit 0. The final handoff records release separately.
