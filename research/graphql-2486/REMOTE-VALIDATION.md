# Supplemental configured-remote GraphQL validation

The existing baseline test passed with fresh remote execution. The business integration test retains local TestRunner because its PostgreSQL fixture requires host Docker access. Configured BuildBuddy services handle build actions and cache reuse. Response observations remain distinct from direct resolver-entry counts.

The user approved research fixtures and build artifacts sent to `remote.buildbuddy.io`. This supplement follows closed [ticket 2486](https://github.com/LowkeyLab/bazel-repo/issues/2486). The [original report](README.md) retains the bounded model and its unresolved attribution question.

## Revision and execution boundaries

A new pool assignment acquired `origin/research/graphql-business-2486` at `745573d970dcba91fc582b95814c2a3241f34409`. Acquisition refreshed `origin/main` to `39075c1280f357d58b06a9a471e617cd73949bdf`. The supplemental branch is `research/graphql-business-2486-remote`. No fixture, production source, collector, dependency, BUILD file, or execution requirement changed.

The [baseline invocation](https://app.buildbuddy.io/invocation/82e000af-7a71-45a0-9579-b6d34d63bb5c) passed with final BEP strategy `remote`. Direct TestRunner records show runner `remote`, `cacheHit: false`, and exitCode 0. Two action-log entries describe that target. This report counts the one final BEP result as one test.

The successful [business invocation](https://app.buildbuddy.io/invocation/2ccc62b4-d30e-408a-928c-7229613ee395) uses the existing `--test_strategy=standalone --strategy=TestRunner=local` requirement. This preserves host Docker access and the maintained Apollo, Axum, Juniper, application-service, repository, and PostgreSQL path. It does not establish remote application or remote PostgreSQL test execution.

Both commands use `--config=ci-remote`, `--cache_test_results=no`, `--noremote_local_fallback`, `--remote_download_outputs=all`, and `--remote_upload_local_results=true`. The command limits jobs to four. Local CPU capacity is two, local memory capacity is 2048 MiB, and the initial Bazel JVM heap is 1536 MiB. The business retry bounds that heap to 3072 MiB. Raw BEP and action JSON remain private.

The business result is `PASSED`, final strategy `local`, with a direct `local` TestRunner record, `cacheHit: false`, and exitCode 0. A separate action record reports a remote cache hit for the same target. This does not replace the fresh final local result. Bazel reports 3752 action-cache hits, one remote-cache hit, one internal entry, and one local entry in the successful retry. It does not establish fresh remote compilation. These reported counters are not a disjoint action partition.

The [first business invocation](https://app.buildbuddy.io/invocation/0f0e6041-ddc2-4732-bbba-cb03a71b306b) exhausted the 1536 MiB heap before TestRunner. Its command ended with exit 33. The failed server remained active during restart. Process inspection checked the exact assigned workspace, output base, and two task-owned clients. The verified server received SIGTERM. No signal went to the unrelated workspace server. The bounded retry then passed without source changes.

## Record checks

The remote baseline retains all eight source identities. Its node metadata matches the original inventory. Its LCOV matches the original zero baseline byte for byte. Node remains `v24.15.0` and GraphQL remains `17.0.2`.

The existing collector ran locally through `bazel run` on the exported business products. It passed with exit 0. The exact expected and measured node counts match the original records. All 77 nodes on 75 lines across eight documents remain present.

The observations contain eleven dispatches, eleven router results, 89 response-field events, eight client directive decisions, four inline contexts, and four fragment contexts. Functional receipts record six maintained documents, four selection cases, one error case, eleven requests, one server, and two names.

The disconnected document remains zero. Both combined merge orders have identical bytes and match the original published LCOV. All separate dispatch, field, fragment, and directive views also match original bytes. Seven damaged-input controls retain their expected rejection: missing fields, fragments, directives, or router response, changed source hash, incomplete completion, and corrupt profile hash. The result retains `boundedResponseContextComplete: true` and `directResolverEntryObserved: false`.

The collector model remains operation dispatch, response field presence, response-context fragment traversal, and client directive decisions. The result does not establish direct resolver-entry counters. Failed or null-propagated fields, merged selections, repeated spreads, aliases, and abstract fragment conditions retain the original attribution question. No production collector or coverage exception follows from this supplement.

## Publication and receipts

Publication includes only this Markdown report and its index link. New raw logs, observations, damaged inputs, completion records, test outputs, action metadata, and archives remain private. Existing published evidence remains unchanged.

The runtime directory is `/home/tacascer/.local/state/worktree-pool/agent-receipts/graphql-2486-remote-validation-gRuWeF`. It retains command stdout, stderr, exit status, BEP, action JSON, selected runner proofs, downloaded outputs, and exact record checks.

| Private artifact             | SHA256                                                             |
| ---------------------------- | ------------------------------------------------------------------ |
| Baseline runner proof        | `4e76cbd68e95fd10dc4170db0cfda5babe7e386d3b3bb2d6632434a4b214a230` |
| Business runner proof        | `2bbeb0022c5d0e9d05fd9789dc865815b37074a4915af8f7f287d4060ed8a630` |
| Exact record checks          | `60151eb09276141150d93a7fad38be2a8bf560a7b2014326ed1e9418481c4d3d` |
| Combined LCOV                | `33c9528716ad0397c30f76aeaa4846d2f748977b48b7cf216dc3b29c53579896` |
| Original-source observations | `fb71b338219cf83bab5eb34d1f5aba27093c59e7a0c1a5a2a8619efbf24aced9` |

Whole-tree formatting and strict English checks passed. No implementation changed. The original lint scope remains unchanged.

This supplement does not establish whole-repository acceptance, GitHub CI, Codecov upload, production resolver instrumentation, or all GraphQL syntax. It creates no PR, merge, or map edit.

## Pool identities

Catalog authority: `de2f76d2-3cb1-4050-838e-9618087569c6`.
Repository: `207f1904-1165-417f-a4f0-399e8498ed22`.
Worktree: `0bed8bf4-6e60-4bdd-8edb-fde22454256a`.
Assignment: `eb02f252-fbfa-4b35-9d8e-960018944c7a`.
Acquisition operation: `6ba5cb10-badc-4950-822c-4928c0b623ce`.
Checkout: `/home/tacascer/.local/share/worktree-pool/worktrees/207f1904-1165-417f-a4f0-399e8498ed22/0bed8bf4-6e60-4bdd-8edb-fde22454256a`.

Catalog and acquisition receipts are `graphql-2486-remote-catalog-3CiUEu` and `graphql-2486-remote-acquire-5wlc59` under the same private receipt root. Both commands completed with exit 0. The final handoff records exact release and availability inspection separately.
