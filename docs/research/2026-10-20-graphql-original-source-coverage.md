# GraphQL original-source coverage

Date: 2026-10-20.
Question: [Determine GraphQL operation original-source coverage](https://github.com/LowkeyLab/bazel-repo/issues/2458).
Parent: [Choose automatic project inclusion for Bazel coverage](https://github.com/LowkeyLab/bazel-repo/issues/2413).
Repository base: `5e246a47cd7ea0a9c506e915db0c053d7d84d023`.

## Result

The bounded adapter records dispatches against original GraphQL operation locations.
The maintained Apollo/Axum/Juniper stack returns the expected query and mutation results.
The probe observes both values of a conditional directive through the response.
The executed document is a new introspection fixture.
The six existing business documents remain unexecuted.
It does not collect server field-resolution events.
A complete original-source executable-line or branch model remains unproved.

The parser baseline discovers a disconnected document outside the selected test dependencies.
Its candidate locations remain zero without execution.
The adapter rejects seven damaged collection records after functional operations succeed.
These checks belong to the research adapter.
They do not establish a native collection guarantee.

The [earlier feasibility report](https://github.com/LowkeyLab/bazel-repo/blob/0dd2c8293fe0375af1377c93996bf848dcd3ff80/docs/research/2026-10-19-remaining-source-coverage.md#graphql-executable-operations) requested a maintained query experiment.
This fixture establishes the maintained stack boundary only within its introspection case.

## Scope and pins

The research targets are [BUILD.bazel](../../nicknamer2/research/graphql/BUILD.bazel).
The [client probe](../../nicknamer2/research/graphql/probe.mjs) uses the maintained Apollo package.
The [bridge](../../nicknamer2/research/graphql/bridge.rs) calls the maintained `server::create_router` and `graphql_schema::create_schema` directly.
The maintained router receives a serialized GraphQL POST request through Axum `oneshot`.
Apollo `HttpLink` uses a fixture fetch function that invokes this bridge.
The test does not start a TCP listener.

The bridge creates a lazy PostgreSQL pool with port 1.
The introspection operations do not need a database connection.
The fixture uses the existing `AlwaysAllow` test service.
It does not test authentication, business mutation effects, PostgreSQL behavior, or a deployed service.

| Component            | Effective pin          |
| -------------------- | ---------------------- |
| Bazel                | 9.3.0                  |
| Node                 | 24.15.0                |
| GraphQL parser       | `graphql` 17.0.2       |
| Apollo               | `@apollo/client` 4.3.4 |
| Axum                 | 0.8.9                  |
| Juniper              | 0.17.1                 |
| Juniper Axum adapter | 0.3.0                  |
| Tower                | 0.5.3                  |

The observation records contain the effective Node and GraphQL versions.
Apollo adds its version to the actual request extensions.
`Cargo.lock`, `pnpm-lock.yaml`, `MODULE.bazel`, and `.bazelversion` supply the remaining pins.
The evidence manifest records source hashes and the maintained router/schema hashes.

## Location model

The fixture source remains an original `.graphql` document.
The probe parses its bytes with `Source(text, canonicalPath)` and preserves `loc.start`.
`getLocation` supplies the original line and column.
The inventory retains offsets, node kinds, and names before Apollo transforms the document.

The candidate inventory includes operation definitions, fields, fragment spreads, fragment definitions, inline fragments, and directives.
It omits comments, variable declarations, punctuation-only lines, and closing delimiters.
The fixture does not contain an inline fragment.
The candidate inventory is a research choice, not an approved executable-location specification.

| Original location          | Candidate meaning          | Direct observation                         |
| -------------------------- | -------------------------- | ------------------------------------------ |
| Selected line 1, column 1  | Query operation start      | Two Apollo dispatch events                 |
| Selected line 2, column 3  | `__schema` field           | Result content only                        |
| Selected line 3, column 5  | `queryType` field          | Result content only                        |
| Selected line 4, column 7  | `name` field               | Result content only                        |
| Selected line 5, column 7  | `TypeName` fragment spread | No collector event                         |
| Selected line 5, column 19 | `include` directive        | False/true variables and response outcomes |
| Selected line 9, column 1  | Fragment definition        | No collector event                         |
| Selected line 10, column 3 | `kind` field               | Absent with false, `OBJECT` with true      |
| Selected line 13, column 1 | Mutation operation start   | One Apollo dispatch event                  |
| Selected line 14, column 3 | `__typename` field         | Response value `MutationRoot`              |

GraphQL defines conditional selection through `@include` and field collection.
It also distinguishes operation execution from field resolution.
See the [GraphQL specification](https://spec.graphql.org/September2025/#sec-Field-Collection).
These rules explain the observed outcomes.
They do not turn client dispatch events into resolver events.

The [Apollo HttpLink documentation](https://www.apollographql.com/docs/react/api/link/apollo-link-http) permits a custom fetch implementation.
The fixture retains both the Apollo link document and the serialized request received by the bridge.
Apollo inserts generated `__typename` fields into the query.
Those fields shift transmitted positions and have no original location in this fixture.
The adapter maps only the operation name to its unique original definition.
It does not map transmitted field offsets to original source locations.

## Baseline and boundary evidence

The execute target declares only `selected.graphql` as a document input.
The baseline target declares both the selected document and `disconnected/unused.graphql`.
The execute test asserts that the disconnected path is absent from its runfiles.
The baseline test reads that source independently.
Both tests use explicit labels and bounded input lists.

The disconnected source has candidate lines 1, 2, 3, and 4.
The parser baseline emits zero for all four lines.
The selected source has nine distinct candidate lines.
Its fragment spread and directive share line 5.
LCOV `DA` records cannot preserve that distinction, but the JSON location inventory retains it.

The query executes twice with `fetchPolicy: 'no-cache'` and `$show` set to false, then true.
The field `name` returns `QueryRoot` both times.
The `kind` field appears only for true.
The mutation executes once and returns `MutationRoot`.
The bridge records HTTP status 200 and no GraphQL errors for all three requests.

The adapter emits measured dispatch counts only at operation starts: `DA:1,2` and `DA:13,1`.
Its remaining zero counts are baseline placeholders for unmeasured candidate locations.
They do not mean that the server failed to execute those fields.
The adapter emits no `BRDA` records.
Directive outcome checks do not establish a general branch-coverage model.

## Collection faults

The functional calls complete before the adapter checks collection records.
The independent validator rejects the following modified records:

- Missing output.
- Empty output.
- Invalid JSON.
- An empty event list.
- A partial event list.
- A wrong source hash.
- A wrong location inventory.

The checks establish bounded receipt validation for the expected three operations.
They do not authenticate arbitrary event contents or detect every possible semantic corruption.
The probe does not change the maintained router to make these checks pass.

## Capability matrix

| Capability                    | Result                        | Limit                                                               |
| ----------------------------- | ----------------------------- | ------------------------------------------------------------------- |
| Original operation identity   | Supported by bounded adapter  | Unique names and fixed source hash                                  |
| Query dispatch                | Measured twice                | Dispatch does not prove resolver invocation                         |
| Mutation dispatch             | Measured once                 | Introspection only, no business effect                              |
| Directive outcomes            | Response observations pass    | No native directive event or `BRDA` mapping                         |
| Fragment field result         | Conditional presence observed | No fragment-entry or field-resolution event                         |
| Original parser locations     | Preserved                     | Transformed field mapping remains absent                            |
| Disconnected zero baseline    | Passes                        | Candidate AST model only                                            |
| Damaged collection detection  | Seven checks pass             | Research validator only                                             |
| General executable-line model | Unproved                      | Fragment definitions and merged fields need a decision              |
| General source mapping        | Unproved                      | Apollo transforms and generated fields need explicit origin mapping |

## Decision input

[Choose collector mechanisms and exceptions after feasibility research](https://github.com/LowkeyLab/bazel-repo/issues/2445) can use these findings.

Original-source operation dispatch coverage is a feasible candidate for this stack.
A parser can supply stable identities and disconnected baseline locations.
A strict collection receipt can detect the bounded loss cases tested here.

A complete field model needs collector events at the server execution boundary.
Those events need original document identity, selection location, response path, and applicable fragment/directive context.
A general adapter also needs explicit origin mappings for transformed documents.
Repeated fields, aliases, merged selections, inline fragments, errors, and partial results need separate semantic decisions.
This report neither selects those semantics nor approves source exceptions.

The existing maintained business documents remain outside the executed fixture.
This experiment tests the maintained transport/schema stack through introspection only.
Host-language coverage alone does not establish the missing GraphQL execution events.

## Native coverage and merge evidence

The configured remote coverage run passed its functional test.
The authoritative combined LCOV contains only `bridge.rs` with `LF:0` and no measured lines.
Neither original `.graphql` source appears in that report.
The test's undeclared outputs separately retain the parser locations, requests, responses, and dispatch LCOV.
The successful native command therefore does not establish original GraphQL coverage.

The pinned Bazel coverage merger combines the native report, parser baseline, and adapter dispatch report.
Both filename assignments for baseline and measured input produce identical combined bytes.
The result retains selected counts 2 and 1, plus all four disconnected zero counts.
This checks preservation of adapter observations through aggregation.
It does not increase their semantic scope.

See the [native report](graphql-original-source-coverage/native-combined.lcov), [baseline](graphql-original-source-coverage/baseline.lcov),
[dispatch report](graphql-original-source-coverage/dispatch-only.lcov), and [combined report](graphql-original-source-coverage/combined-forward.lcov).
The [evidence archive](graphql-original-source-coverage/evidence.tar.gz) retains raw test outputs and sanitized command logs.
The [archive manifest](graphql-original-source-coverage/archive-manifest.json) records publication-byte hashes.
The [source manifest](graphql-original-source-coverage/source-manifest.json) records tested source hashes.

Publication logs replace checkout, home, cache, temporary, and private evidence prefixes with documented placeholders.
Private originals remain intact.
The substitutions remove machine identity and retain package pins, flags, results, and source positions.
This report claims no hosted CI run.

## Check summary

| Check                            | Result                | Execution evidence                                                               |
| -------------------------------- | --------------------- | -------------------------------------------------------------------------------- |
| Initial baseline and router test | Pass                  | Two fresh remote tests, 1163 remote cache hits, 8 remote actions                 |
| Native coverage                  | Pass with GraphQL gap | One fresh remote test, 641 remote cache hits, 17 remote actions, 2 local actions |
| Final formatted fixture          | Pass                  | Two fresh remote tests, 2 remote cache hits, 4 remote actions                    |
| Both pinned merge orders         | Pass                  | Cached merger build, local merger execution                                      |
| Bounded lint                     | Pass                  | Clippy and KeepSorted, 635 remote cache hits, 9 remote actions                   |
| Full formatting                  | Pass                  | Repository formatter                                                             |

The final test ran the committed fixture bytes after source formatting.
The earlier native coverage test ran the preserved initial source snapshot.
The source manifest distinguishes these snapshots.
The [reproduction commands](graphql-original-source-coverage/commands.txt) retain the meaningful flags.

The [final test invocation](https://app.buildbuddy.io/invocation/40364e8d-78f9-46fb-b1fc-5a3a4feab1bb) executed both tests.
The [native coverage invocation](https://app.buildbuddy.io/invocation/b95ef228-706a-4e54-9b15-517bf2a2f138) produced the authoritative report.
The [lint invocation](https://app.buildbuddy.io/invocation/6bf30dc3-7dd9-482b-9dc8-f251963ce984) checked the research bridge and test targets.
The configured lint aspects applied Clippy and KeepSorted.
They did not apply ESLint to this `.mjs` probe.
