# Supplemental configured BuildBuddy validation

The configured build and fresh browser test passed. Chromium executed locally. This result does not establish remote Chromium execution or fresh remote Astro compilation.

The user approved the research fixtures and build artifacts sent to `remote.buildbuddy.io`. This supplement follows closed [ticket 2483](https://github.com/LowkeyLab/bazel-repo/issues/2483). It preserves the mapping gaps in the [original report](README.md).

## Revision and execution boundary

A new assignment acquired `origin/research/browser-astro-2483` at `a4055e39122d102fa0a221b7db2feb9299fbb287`. Acquisition also refreshed `origin/main`. The supplemental branch is `research/browser-astro-2483-remote`. No fixture, application, collector, dependency, or BUILD source changed.

[BuildBuddy invocation 4b7bb634-9f1a-4575-90df-78e06b8152e4](https://app.buildbuddy.io/invocation/4b7bb634-9f1a-4575-90df-78e06b8152e4) passed for these existing targets:

- `//tools/research_astro:browser_test`
- `//personal_website:build`

The browser target retains `no-remote-exec`. Its final BEP result records `PASSED` with strategy `linux-sandbox`. The action log records local TestRunner execution with `cacheHit: false`. The command disabled test-result reuse and local fallback. Chromium remains `156.0.8078.4`, with Node `v24.15.0`.

Bazel reports 539 remote cache hits, 676 internal entries, two local sandbox entries, and one remote entry. These reported counters are not a disjoint action partition. The fresh remote entry is a `CopyFile` action for `//tools/research_astro:browser`. Astro build actions used cached results. This invocation therefore proves configured service access and fresh local browser testing, not fresh remote Astro compilation.

The command used `--config=ci-remote`, `--cache_test_results=no`, `--noremote_local_fallback`, and `--remote_upload_local_results=true`. It bounded jobs to four, local CPU capacity to two, local memory capacity to 2048 MiB, and the Bazel JVM heap to 1536 MiB. The existing browser dependency shell supplied local Chromium libraries.

## Existing record and failure checks

The existing server probe, maintained-site browser interval, merge, and checked mapping stages each returned exit 0. These programs ran locally through `bazel run` against the downloaded build artifacts. Their commands disabled remote services. The maintained interval used the actual built site at its local `/maintained/` URL.

The checks reproduce the existing bounded results:

- All fifteen expected original lines remain present across three components.
- Processed-script lines 14 and 15 each retain one hit.
- Four original conditional identities retain counts `1/1`, `1/0`, `0/0`, and `1/0`.
- Compiler-only baseline lines remain zero.
- Both partial merge orders have identical bytes.
- Five existing controls each return exit 1 while the functional receipt remains true.
- Source hashes match the original report.

The controls remove or corrupt the maintained profile, remove completion, remove captured script text, and remove the fixture bundle map. The controlled semantic mutation also remains inside the existing mapping check.

The maintained interval again captures ten scripts and 829 ranges. All ten script records lack a source-map URL. Captured module bytes match the built maintained JavaScript. The original-source mapping gap remains open.

Default initializers still have independent expected count zero. The converter still returns counts two for `Main` and one for `TestOnly`. This supplement does not resolve that discrepancy or approve an exclusion.

## Private receipts and selected hashes

Raw logs, BEP, action records, browser profiles, scripts, compiler maps, and control copies remain private. Publication contains only this report and its index link. Existing published fixture evidence remains unchanged.

| Private artifact              | SHA256                                                             |
| ----------------------------- | ------------------------------------------------------------------ |
| Partial merged LCOV           | `bf79327d8ab57dc1509ff0ff3fd7e98c596c5aa98bcd5b560968482af68d34f2` |
| Original zero baseline        | `835e06eecebaa105f1741132d66f88cbd96a89333b8439384996df29fd8cabb5` |
| Original conditional records  | `4c13f72d727845d1c63f94e69334fbee888fa4c9362e613101e4c9b0aedaf74d` |
| Maintained browser profile    | `cd56af594d74554b9a452d81b7b182a8cc174859aefedcc19a8efbb2c3b7365c` |
| Expected-record check summary | `234c1d16c2a1d32a481a6238e49e338d51284ef46ce218fdcbb0c8d681cbf63a` |

The runtime directory is `/home/tacascer/.local/state/worktree-pool/agent-receipts/browser-2483-remote-validation-ighGKg`. It retains every stage's stdout, stderr, exit status, raw products, and selected check records.

Whole-tree formatting and strict English checks passed for this supplement. No implementation changed. This validation does not establish complete original-source coverage, GitHub CI, deployment, or Codecov upload. It changes no production policy and creates no PR or merge.

## Pool identities

Catalog authority: `de2f76d2-3cb1-4050-838e-9618087569c6`.
Repository: `207f1904-1165-417f-a4f0-399e8498ed22`.
Worktree: `0bed8bf4-6e60-4bdd-8edb-fde22454256a`.
Assignment: `e7a8fc45-a104-4391-8540-25bec4816533`.
Acquisition operation: `6bb1accf-fd63-4f82-9f3d-106984e30e02`.
Checkout: `/home/tacascer/.local/share/worktree-pool/worktrees/207f1904-1165-417f-a4f0-399e8498ed22/0bed8bf4-6e60-4bdd-8edb-fde22454256a`.

Catalog and acquisition receipts are `browser-2483-remote-catalog-5zNQNU` and `browser-2483-remote-acquire-2zzWb9` under the same private receipt root. Both commands completed with exit 0. The final handoff records release separately.
