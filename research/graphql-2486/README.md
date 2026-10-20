# GraphQL business operations and original-source observations

The bridge observes original field locations through response materialization. It also records fragment context and client directive decisions. These events do not establish direct resolver-entry counts.

All six maintained business documents pass through Apollo, the maintained Axum/Juniper router, the application services, the repositories, and PostgreSQL. A separate business-shaped document tests fragments and directives. Complete resolver attribution remains an open feasibility question.

## Scope and revision

This report resolves [ticket 2486](https://github.com/LowkeyLab/bazel-repo/issues/2486). It continues [ticket 2458](https://github.com/LowkeyLab/bazel-repo/issues/2458#issuecomment-6286592475) under the [approved experiment scope](https://github.com/LowkeyLab/bazel-repo/issues/2445#issuecomment-6287451818).

The refreshed base is `39075c1280f357d58b06a9a471e617cd73949bdf`. The research branch is `research/graphql-business-2486`. The experiment changes no production source, membership policy, threshold, Python architecture, or Codecov policy.

The current pins match the predecessor: Bazel 9.3.0, Node 24.15.0, GraphQL 17.0.2, Apollo 4.3.4, Axum 0.8.9, Juniper 0.17.1, Juniper Axum 0.3.0, and Tower 0.5.3. See the [source and pin hashes](evidence/source-and-pin-hashes.json), [Cargo.lock](../../Cargo.lock), [pnpm-lock.yaml](../../pnpm-lock.yaml), and [MODULE.bazel](../../MODULE.bazel). Runtime observations record Node and GraphQL versions. Apollo records its version in transmitted request extensions.

The [maintained inventory](evidence/maintained-source-inventory.json) records all six documents and their SHA256 hashes. Each research input matches its maintained source byte for byte. Original source identities remain under `angular/projects/nicknamer2-web/src/app/graphql/`.

## Executed application path

The [probe](../../nicknamer2/research/graphql_business/probe.mjs) uses Apollo `HttpLink` with a custom fetch function. The [bridge](../../nicknamer2/research/graphql_business/bridge.rs) receives each serialized request through standard input. It calls the maintained `server::create_router` and `graphql_schema::create_schema`.

The router receives GraphQL POST requests through Axum `oneshot`. Juniper uses the maintained services, repositories, and database migrations. The fixture imports the declared Bazel PostgreSQL image and starts an isolated database container. Docker inspection records 536870912 bytes of memory and 1000000000 nano-CPUs. See the [runtime inspection](evidence/raw/execute/bridge-ready.json.txt).

The test does not start a GraphQL TCP listener. It does not start Angular, a browser, or the generated Angular operation services. The fixture uses `AlwaysAllow` with a fixed research authorization header. It does not test production authentication. PostgreSQL communication uses the container's mapped TCP port.

The successful cases execute these maintained documents:

| Document                   | Functional assertion                             |
| -------------------------- | ------------------------------------------------ |
| `create-server.graphql`    | Creates server 42 with the expected display name |
| `create-name.graphql`      | Creates nickname Alpha                           |
| `create-names.graphql`     | Creates nickname Beta through the batch mutation |
| `get-dashboard.graphql`    | Returns a total server count of one              |
| `get-servers.graphql`      | Returns the created server                       |
| `get-server-names.graphql` | Returns Alpha and Beta in order                  |

The fixture also sends `GetDashboard(first: 0)`. The maintained resolver returns the expected pagination error. Four additional cases exercise every combination of the probe's `show` and `hide` Boolean variables. Final database queries record one server and two names. See the [functional receipt](evidence/raw/execute/functional.json.txt) and [database totals](evidence/raw/execute/bridge.stderr.txt.txt).

## Original locations and event meanings

The baseline parses original bytes with `Source(text, canonicalPath)`. It records AST node starts with original offsets, lines, and columns. The source-backed inventory runs independently of dispatch observations. A handwritten line table checks the candidate lines before execution. Handwritten case rules define expected node counts.

The research inventory contains 77 nodes on 75 source lines across eight documents. Comments, variable definitions, punctuation-only lines, and closing delimiters remain outside this candidate set. This is an experimental location model. It is not an approved complete executable-location contract.

| Event                | Exact observation                                                                                                      | Limit                                                                         |
| -------------------- | ---------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| `dispatch`           | Apollo forwards an operation                                                                                           | Does not prove server execution or field completion                           |
| `response-field`     | The serialized server response contains the original field's response key in a materialized parent                     | Does not record resolver entry or failed fields lost through null propagation |
| `fragment-context`   | The client traverses an original spread or definition in a materialized response context with a matching concrete type | Does not prove a separate server fragment execution event                     |
| `inline-context`     | The client traverses the original inline fragment in a matching response context                                       | Tests only a concrete `Server` condition                                      |
| `directive-decision` | The client evaluates the original Boolean directive argument during response-context traversal                         | Does not record Juniper's directive evaluation                                |
| `router-result`      | The maintained router returns the serialized response                                                                  | Does not identify every resolver call                                         |

The [GraphQL specification](https://spec.graphql.org/September2025/#sec-Field-Collection) separates field collection, field execution, resolution, and value completion. The experiment keeps these distinctions. The [Apollo documentation](https://www.apollographql.com/docs/react/api/link/apollo-link-http) defines the custom fetch boundary.

The raw server response includes Apollo's generated `__typename` fields. The collector uses those values to check concrete fragment conditions. It does not assign generated fields to original source locations. Original location metadata comes from the unmodified document AST.

## Exact expected and observed records

The [expected node counts](evidence/raw/collection/expected-node-counts.json.txt) and [measured node counts](evidence/raw/collection/measured-node-counts.json.txt) match exactly. The [raw observations](evidence/raw/execute/observations.json.txt) contain 11 dispatches, 11 router results, 89 response-field events, eight client directive decisions, four inline contexts, and four fragment contexts.

The failed dashboard request exposes the semantic difference directly. Its operation has two dispatch hits across the normal and error cases. Its root field has one response-materialization hit. The second response reports the server error. A dispatch counter cannot replace a field-completion counter.

The controlled [selection document](../../nicknamer2/research/graphql_business/selection.graphql) has these expected counts:

| Original location                   | Expected event count            |
| ----------------------------------- | ------------------------------- |
| Operation, line 1                   | Four dispatches                 |
| Fields, lines 2, 3, 4, 5, 6, and 10 | Four response-field events each |
| Fragment spread, line 7             | Two response contexts           |
| Fragment definition, line 16        | Two response contexts           |
| Fragment fields, lines 17 and 18    | Two response-field events each  |
| `displayName`, line 8               | Two response-field events       |
| Directives, lines 7 and 8           | Four client decisions each      |
| Inline fragment, line 9             | Four matching response contexts |

Each directive has two true decisions and two false decisions. The probe checks that skipped fields disappear from the actual response. Branch records describe these client decisions only. They do not claim server branch coverage.

The collector exports separate [dispatch](evidence/raw/collection/dispatch-only.lcov.txt), [field](evidence/raw/collection/fields-only.lcov.txt), [fragment](evidence/raw/collection/fragments-only.lcov.txt), and [directive](evidence/raw/collection/directives-only.lcov.txt) views. The [combined experimental view](evidence/raw/collection/combined-zero-first.lcov.txt) uses the maximum node count on each line. Its `TN` identifies a response-context research model. A line with a directive can therefore differ from the field-only view.

The baseline includes `disconnected/unused.graphql` at zero. The execute target excludes that document from its declared inputs. The test checks its absence from runfiles. Both the [zero-first merge](evidence/raw/collection/combined-zero-first.lcov.txt) and [zero-last merge](evidence/raw/collection/combined-zero-last.lcov.txt) retain all eight identities and produce identical records.

## Missing-observation controls

Seven controls fail after the functional operations succeed:

- Remove response-field observations.
- Remove fragment observations.
- Remove directive observations.
- Remove one router response.
- Change a source hash.
- Mark collection incomplete.
- Corrupt the profile hash.

Semantic controls recompute the completion hash after they change observations. Their failures therefore test required observations, not only checksum validation. See the [control results](evidence/raw/collection/controls.json.txt) and the exact damaged inputs in the [collection evidence](evidence/raw/collection).

Only the specified response-context model receives `boundedResponseContextComplete`. The result explicitly records `directResolverEntryObserved: false`.

## Validation and reproduction

Two fresh local tests passed after formatting. The collector then passed exact record checks, seven damaged-observation controls, and both merge orders. Build actions reused local results. Test processes executed with the local TestRunner strategy. See [test stdout](evidence/final-test.stdout), [test stderr](evidence/final-test.stderr), and the [BEP extract](evidence/final-endpoint-and-test-evidence.json).

```bash
nix develop --command bazel --host_jvm_args=-Xmx1536m test \
  --config=ci-remote --remote_executor= --remote_cache= --bes_backend= \
  --remote_upload_local_results=false --jobs=4 \
  --local_resources=cpu=2 --local_resources=memory=2048 \
  --test_strategy=standalone --strategy=TestRunner=local \
  --cache_test_results=no \
  //nicknamer2/research/graphql_business:baseline_test \
  //nicknamer2/research/graphql_business:execute_test
```

Run `bazel run //nicknamer2/research/graphql_business:collect` through the same Nix environment and local endpoint overrides. Pass the exported observations path, completion path, and output directory after `--`. See [collection stderr](evidence/collection.stderr) for the executed target.

Gazelle passed after source edits. Whole-tree formatting passed with Java 21 formatter settings. Focused Aspect lint passed with no findings. The applicable linters were Clippy and KeepSorted. This does not claim that ESLint checked the standalone probe. The experiment did not run a whole-repository build.

Canonical BEP extracts record empty remote cache, executor, and BES endpoints for [formatting](evidence/format-endpoint-and-test-evidence.json), [tests](evidence/final-endpoint-and-test-evidence.json), and [lint](evidence/lint-endpoint-and-test-evidence.json). Result uploads remain disabled. Fresh configured-remote execution and hosted uploads remain absent while user approval is pending.

The evidence retains setup failures. Bazel rejected the first test-only dependency configuration, differing source-copy tags, and the unsupported `local` test strategy name. A sandboxed run then failed to connect to the container's host port. The final command applies `--strategy=TestRunner=local` to this integration fixture.

Raw exports use `.txt` suffixes to preserve exact bytes. The [artifact manifest](evidence/artifact-hashes.json) records their SHA256 hashes.

## Remaining feasibility question

Can server observations map resolver attempts and field collection back to every original document occurrence? The mapping must handle failed or null-propagated fields, merged selections, repeated spreads, aliases, and abstract fragment conditions.

This experiment does not resolve that question. Response materialization supports useful original-source evidence, but it cannot establish complete resolver execution. The controlled fragment and directive cases also do not prove all GraphQL syntax. No collector selection or coverage exception follows from this result.

## Assignment receipts

Catalog authority: `de2f76d2-3cb1-4050-838e-9618087569c6`.
Repository: `207f1904-1165-417f-a4f0-399e8498ed22`.
Worktree: `0bed8bf4-6e60-4bdd-8edb-fde22454256a`.
Assignment: `89e4261c-8780-4e84-9bea-2e10b3f35a8d`.
Acquisition operation: `d340425d-a512-49f1-a931-d52f94219951`.
Checkout: `/home/tacascer/.local/share/worktree-pool/worktrees/207f1904-1165-417f-a4f0-399e8498ed22/0bed8bf4-6e60-4bdd-8edb-fde22454256a`.

Private receipt directories are under `/home/tacascer/.local/state/worktree-pool/agent-receipts/`: `graphql-2486-catalog-9vgUQC` and `graphql-2486-acquire-SXfRgw`. Both commands completed with exit 0. The final handoff records publication and release separately.
