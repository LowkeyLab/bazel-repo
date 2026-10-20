# Node and Angular original-source records

Date: 2026-10-20.
Question: [Test faithful Node and Angular original-source coverage records](https://github.com/LowkeyLab/bazel-repo/issues/2482).
Base: `39075c1280f357d58b06a9a471e617cd73949bdf`.
Predecessor: [Node and Angular runtime feasibility](https://github.com/LowkeyLab/bazel-repo/blob/086722b6def7e657d21a0bd401b4e59be45f14d6/docs/research/2026-10-20-node-angular-runtime-validation.md).
Scope: [Approved collector experiments](https://github.com/LowkeyLab/bazel-repo/issues/2445#issuecomment-6287451818).

## Result

The bounded adapters retain positive original TypeScript records, test records, helper records, child records, and disconnected zeros.
Their combined line sets match the independent original statement models for twelve sources.
Zero baselines do not replace measured positive counts.

Native AST conversion still exposes a compiler-lowering limit.
ES5 async lowering adds a mapped function-header line outside the original statement model.
An original-AST projection removes that generated attribution while preserving the original statement hits.
However, the mapped branch changes from an original conditional expression to a generated `if`.
Its arm counts have the opposite meaning from the original conditional.
The report retains that failed branch expectation.
A complete native original-branch mechanism remains unresolved.

Angular explicit inclusion still omits the selected spec and an imported maintained test.
A bounded hook around the pinned provider's inclusion method adds their original records.
This hook demonstrates test-body collection.
It does not establish a stable public integration API.

All six maintained teardown snapshots accept missing or partial reports after functional success.
An independent validator rejects all twelve faulty controls and accepts all six complete controls.
A native process validator also rejects missing profiles and a missing child profile.
These validators belong to the research adapter.
No production adapter receives a completeness guarantee from its normal exit status.

## Inputs and semantics

Current main retains Node `24.15.0`, TypeScript `6.0.3`, Angular build `22.2.3`, and Vitest/provider `5.0.5`.
The effective AST converter is `ast-v8-to-istanbul` `1.0.7`.
`rules_js` remains at `e67442a2ccf329841c9227d66ee013afcf5633c6`.
`rules_angular` remains at `bfe6e98434982bad38c393f1bd472ec33f16cd32` with the maintained compatibility patch.
The research adds no package dependency and changes no production target.

The [fixture](../../tools/coverage/node_research/faithful.mjs) uses Bazel-provisioned tools.
It preserves original bytes from maintained website utilities, a maintained test, and executable fixture helpers.
The Angular run also executes the maintained Predix `createMockObject` helper.
The [snapshot manifest](../../tools/coverage/node_research/maintained/sources.json) records each original path and SHA-256.
The controlled spec and child script are static research files with their own canonical identities.
Every source mapping uses exact copied bytes, with no line offset.
This bounded inventory does not establish automatic repository discovery.

The line model uses original statement starts and implicit returns from arrow functions.
It includes variable initializers, returns, expressions, throws, `if` statements, and `for-of` statements.
It excludes comments, interfaces, type aliases, import declarations, blank lines, and function declaration headers.
These are explicit statement-line semantics for this experiment.
The model covers the observed syntax.
It does not establish a complete parser policy for every TypeScript construct.

The independent expectations precede conversion.
Handwritten sets cover the controlled modules, the maintained SEO utility, and the selected Angular test.
A separate TypeScript AST pass derives the complete statement sets for the copied originals.
The [line comparison](node-angular-original-records/original-line-verification.json) preserves all twelve expected and actual sets.
The raw converter records remain separate from the projected records.

## Native conversion and the remaining branch question

The native runner compiles original TypeScript through the pinned compiler and preserves emitted JavaScript plus maps.
It executes the maintained test, production code, executable helpers, and an awaited child.
Two V8 profiles preserve the parent and child executions.
The child alone executes the controlled child module and maintained SEO utility.
A disconnected module remains unexecuted.

The research adapter calls the pinned [AST converter](https://github.com/AriPerkkio/ast-v8-to-istanbul/blob/v1.0.7/src/index.ts) with generated code, generated AST, compiler maps, and measured V8 functions.
It derives a separate zero baseline from the same mapped AST with no measured functions.
This path avoids the predecessor's original-manifest replacement of measured TypeScript hits with synthetic zeros.
It does not call the unchanged native `rules_js` converter.
The predecessor's native-manifest defect therefore remains a production collector limitation.

The simple controlled modules and maintained utility produce the expected original statement lines.
The ES5 async control also produces an extra record at `lowered.ts:2`, its declaration header.
The declared original statement set contains only line `3`.
The projection requires every original modeled line to exist in compiler-mapped data.
It then retains the mapped counts at those original lines.
It does not assign zero to an observed original positive line.
The [projection record](node-angular-original-records/native-projection.json) retains every removed attribution and its count.

The branch control calls `lowered(true)` and receives `9`.
The original conditional expects a positive true arm and a zero false arm.
The mapped native record instead has type `if`, with arm counts `0,1`.
The simple production `if` control retains its expected `1,0` arm meaning.
The [branch expectations](node-angular-original-records/native-branch-expectations.json) preserve both outcomes.
Equal branch counts and an original line number do not establish original branch identity or polarity.

The precise remaining feasibility question is how to recover original branch identity and arm meaning through control-flow lowering.
The bounded line projection does not solve that problem.
The raw branch records remain available for inspection and merge checks.
Their preserved counts do not establish correct original branch semantics.

## Angular test membership and source records

The Angular experiment uses the pinned application and unit-test builders with explicit `coverageInclude`.
Both runs enable subprocess attachment and set coverage exclusions to an empty list.
Both execute the maintained assertions and the controlled parent, helper, and child calls successfully.

The first run still omits the selected spec and imported maintained test from LCOV.
The second run permits the bounded workspace's files through the provider's `isIncluded` method.
It supplies positive original assertion records for both tests.
It also retains maintained production and helper hits, child-only hits, and executable disconnected zeros.
The [provider source](https://github.com/vitest-dev/vitest/blob/v5.0.5/packages/coverage-v8/src/provider.ts) filters scripts through `isIncluded` before conversion.
The experiment preserves each inclusion decision, remapping input, source map, provider profile, and raw Istanbul map.

The hook uses an internal method on a pinned provider.
It is a candidate seam for a later collector design, not a supported public configuration promise.
The fixture uses exact maintained source copies and one controlled Angular workspace.
It does not execute all maintained Angular application targets or prove every Angular template mapping.

Canonical normalization maps the exact originals to repository paths.
The empty `main.ts` record contains no executable lines and remains a documented non-executable input.
Unknown executable identities fail normalization.
The [canonical mapping](node-angular-original-records/angular-canonical-mapping.json) preserves every accepted identity.
Raw temporary-workspace paths remain in the evidence archive.

## Faults and aggregation

The six teardown functions execute from exact maintained snapshots.
For each function, the experiment supplies a missing report, a partial report, and the complete report from the successful Angular run.
All eighteen teardown calls return normally.
All six partial reports omit required executable lines.
The independent validator rejects every missing or partial report and accepts every complete report.
These are snapshot-function results, not executions of the six maintained private test targets.

Native validation requires positive records from the known parent and child executions.
It rejects no profiles and removal of the child profile after functional success.
It accepts the complete profiles.
Disconnected zero records remain valid.
The controlled observations do not establish detection of every possible corruption.

Both pinned Bazel merge orders combine the native projected baseline, native projected measurement, and canonical Angular measurement.
Both orders produce identical bytes.
The independent check preserves all available line and branch counts under the serialized identities.
All twelve combined line sets match their original models.
The unresolved native branch polarity remains unresolved after merging.
A successful merge does not establish branch provenance.

## Validation and decision input

The final research test executes freshly in a local Bazel sandbox with test-result caching disabled.
Remote executor, remote cache, and BES remain disabled for these commands.
Full formatting, required Gazelle runs, applicable lint, strict English, source hashes, and archive hashes pass.
The applicable lint aspects are ESLint and KeepSorted.
Fresh configured-remote validation remains unproved while the coordinator awaits explicit payload approval.
No hosted CI, Codecov upload, or repository-wide production acceptance ran.

Candidate direction: use mapped AST conversion plus an independent original statement model for the tested native line semantics.
Use an explicit test-membership adapter and subprocess attachment for the demonstrated Angular path.
Require independent source, executable-record, process, and expected-output validation.
Retain the native lowering and branch-polarity failure as a required mechanism question.
These findings approve no collector exception or production implementation.
They preserve Python architecture, combined LCOV policy, thresholds, and Codecov policy.

The [commands](node-angular-original-records/commands.txt) and [evidence archive](node-angular-original-records/evidence.tar.gz) preserve exact inputs and results.
Published command logs use directory placeholders.
Raw profiles, maps, and emitted code retain exact bytes and technical execution paths.
Private raw records remain intact outside the reusable checkout.
