# Clojure original-line coverage

Date: 2026-10-20.
Question: [Determine a hermetic Clojure original-line coverage integration](https://github.com/LowkeyLab/bazel-repo/issues/2449).
Parent: [Choose automatic project inclusion for Bazel coverage](https://github.com/LowkeyLab/bazel-repo/issues/2413).
Repository base: `61e9f955bc2d75f7108c6332abae49e232ea21c4`.

## Result

The bounded adapter provisions Cloverage through Bazel and collects original production, helper, macro-call, multiline, and child-only records.
It does not satisfy complete source membership.
The executed test assertion bodies have no tracked forms or line records.
Their original expected locations remain failed collection results.

The baseline also has a lifecycle limit.
Cloverage evaluates namespaces and records 45 positive initialization forms before the adapter resets the registered counters.
The resulting baseline contains 86 registered forms with zero hits.
This result does not prove discovery without namespace evaluation.

The remote action completes the requested independent observations despite those limits.
A separate child returns 42 and contributes its original line record.
The adapter rejects missing and corrupt expected child output after functional tests pass.
Orchestration success remains separate from source-location acceptance.
No exception or final collector mechanism receives approval.

## Pins and inputs

The [prerequisite report](https://github.com/LowkeyLab/bazel-repo/blob/9f6c186ec7ad5a479754fdbc19a4251f86791746/docs/research/2026-10-19-jvm-clojure-coverage-guarantees.md)
identified the AOT and resource-loading boundaries.
This fixture uses the actual maintained sample libraries and small research namespaces.
The [research targets](../../clojure_sample/research/BUILD.bazel) retain the sample's production `srcs`, `resources`, and `aot` pattern.
The existing sample test library retains its resource-loaded declaration.

| Input                     | Pin                                                                |
| ------------------------- | ------------------------------------------------------------------ |
| Bazel                     | 9.3.0                                                              |
| Effective JVM             | Java 25.0.4                                                        |
| Effective Clojure         | 1.12.6                                                             |
| `rules_clojure`           | `c0322b0251489b5d9f883e55ec093e90cb0dbdd9`                         |
| Cloverage source          | `e427762f75d3e1fe461554c08ad247a7365580d2`                         |
| Cloverage archive SHA-256 | `ccd574a9e5180c4e3834fa9c4b44d97d45f799c9639868b6689d4aaab407bf50` |
| Maven artifacts           | Twelve checksum-pinned artifacts in the research lock              |

[MODULE.bazel](../../MODULE.bazel) defines the isolated research repositories.
The [Maven lock](../../clojure_sample/research/maven_install.json) records versions and artifact hashes.
The candidate's [dependency declaration](https://github.com/cloverage/cloverage/blob/e427762f75d3e1fe461554c08ad247a7365580d2/cloverage/project.clj)
supplies tools.reader 1.5.2, tools.cli 1.1.230, tools.logging 1.3.0, tools.namespace 1.5.0, and java.classpath 1.1.0.
It also supplies data.xml 0.2.0-alpha9, data.json 2.5.1, Riddley 0.2.2, and Slingshot 0.12.2.
The experiment uses the repository's Clojure 1.12.6 instead of the candidate's provided 1.12.0 declaration.

The initial Maven graph contained redundant edges from both spec libraries to Clojure.
Those edges created a Bazel dependency cycle.
The research declarations exclude those back edges.
All twelve artifact versions and byte hashes remain unchanged after that correction.
The existing sample still uses its maintained `@deps` declarations.
This research does not audit the complete repository bootstrap contract.

The evidence preserves original source bytes, source hashes, effective process commands, and tracked forms before LCOV conversion.
The input inventory also records the runtime classpath hashes and original AOT class resources.
These artifacts establish the bounded action inputs.
They do not establish arbitrary source discovery or a repository-wide integration.

## Instrumentation lifecycle

The [adapter](../../clojure_sample/research/src/coverage_research/runner.clj) declares seven namespace-to-source mappings.
It checks that both production `__init.class` resources exist before instrumentation.
It also checks that neither production namespace loaded yet.
It instruments source forms in dependency order before requiring the test namespaces.

Cloverage's [namespace loader](https://github.com/cloverage/cloverage/blob/e427762f75d3e1fe461554c08ad247a7365580d2/cloverage/src/cloverage/coverage.clj)
instruments each namespace and marks it loaded.
The adapter uses that path before normal `require` can load the AOT definitions.
The successful original production hit establishes this ordering for the fixture.
The experiment does not instrument existing AOT bytecode or test already-loaded namespace references.

The [instrumenter](https://github.com/cloverage/cloverage/blob/e427762f75d3e1fe461554c08ad247a7365580d2/cloverage/src/cloverage/instrument.clj)
evaluates top-level forms as it instruments them.
Each process therefore preserves these stages separately:

1. Initialization registers 86 forms and records 45 positive forms.
2. The adapter resets each registered atomic counter to zero.
3. The baseline snapshot preserves those registered executable forms with no hits.
4. The measured phase performs the requested functional calls, or performs none in baseline mode.

The explicit initialization marker equals 1 in every process.
The baseline process contains zero measured forms after the reset.
It still evaluated namespace initialization before that reset.
This is a reset baseline over registered metadata, not a parser-only baseline or proof of side-effect-free discovery.
Initialization reports remain separate from measured LCOV merges.

The namespace inventory includes both test namespaces, the helper, the disconnected namespace, and the child namespace.
It does not depend on test discovery to select those namespaces.
The disconnected library is a separate runner dependency outside the native test's dependency path.
The inventory remains explicit and bounded.
Automatic membership discovery remains outside this experiment.

## Original locations and test-body omission

| Required source location      | Observation after reset        | Collection result                   |
| ----------------------------- | ------------------------------ | ----------------------------------- |
| Maintained `core.clj:7`       | `DA:7,5`                       | Positive original production record |
| Research `production.clj:10`  | `DA:10,4`                      | Positive multiline binding record   |
| Research `production.clj:11`  | `DA:11,3`                      | Positive original macro-call record |
| Research `helper.clj:4`       | `DA:4,3`                       | Positive helper record              |
| Maintained `core_test.clj:6`  | No tracked form or `DA` record | Failed original expectation         |
| Research `cases_test.clj:7`   | No tracked form or `DA` record | Failed original expectation         |
| Research `disconnected.clj:4` | Explicit `DA:4,0`              | Valid uncalled body record          |
| Research `child.clj:4`        | Parent 0, child 3              | Positive separate-child record      |

Both test bodies execute successfully.
The maintained test namespace registers only original lines 1 and 5.
The research test namespace registers only original lines 1 and 6.
Those records describe namespace and test definitions.
They do not describe the executed assertions.
The adapter preserves the failed expectations rather than moving them to definition lines.

Clojure's [deftest implementation](https://github.com/clojure/clojure/blob/clojure-1.12.6/src/clj/clojure/test.clj)
places the test body in the Var's `:test` metadata.
The preserved expanded form contains the original assertion at lines 6 and 7 inside that metadata.
The inspected Cloverage `:def` walker instruments the initializer and retains the name metadata.
It does not traverse that test function in this path.
The raw tracked forms and executed omission support this diagnosis.
They do not prove that every possible Clojure test adapter has this limit.

The custom macro body at production line 7 receives initialization hits.
Its runtime invocation at line 11 receives measured hits.
The raw form records retain original metadata and expanded forms for both phases.
This result does not select a general policy for compile-time macro execution.

Cloverage's [LCOV exporter](https://github.com/cloverage/cloverage/blob/e427762f75d3e1fe461554c08ad247a7365580d2/cloverage/src/cloverage/report/lcov.clj)
uses covered-form counts per line.
Its [line statistics](https://github.com/cloverage/cloverage/blob/e427762f75d3e1fe461554c08ad247a7365580d2/cloverage/src/cloverage/report.clj)
count covered forms rather than function-call frequency.
The child calls its function once, but its line receives 3 for three covered forms.
The inspected exporter emits no `BRDA` records.
This exporter limit does not prove that every Clojure collector lacks branch support.

## Canonical identities and process evidence

The native exporter retains classpath-relative names for source resources inside jars.
The adapter preserves that raw LCOV before applying an exact resource-to-repository mapping.
Its source bytes stay unchanged, so line offsets stay unchanged.
The mapping covers only the seven declared original sources.
Unknown form identities fail collection rather than disappearing.

The parent, baseline, and child run in separate JVMs.
Their receipts record process identities, source hashes, effective versions, and functional results.
The parent waits for each child and records its completion and exit status.
The child command uses the same Bazel-provided JVM and classpath.

The parent runs two tests containing three assertions, with zero failures and errors.
The baseline process performs no controlled measured calls.
The separate child returns 42 and records positive original `child.clj:4` coverage.
The parent retains an explicit zero for that same line.
The adapter checks matching source hashes, expected identities, process identity, and completion before accepting child output.

A [research action](../../clojure_sample/research/probe.bzl) declares the entire evidence tree as a Bazel output.
The action sets a 180-second [Bazel timeout](https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/actions/Spawns.java#L143).
Each child has a 60-second limit.
This is experimental output export, not production coverage orchestration or hosted acceptance.

## Fault controls and aggregation

The adapter retains the valid child artifacts unchanged.
One control supplies the child receipt without its expected LCOV file.
Another control supplies that receipt with invalid LCOV text.
Both controls fail collection after functional tests and the valid child complete.
The independent zero-hit baseline remains valid for its registered form set.
These controls do not establish detection of every possible corruption.

The pinned Bazel merger combines the reset baseline, parent measurement, and child measurement.
Initialization reports do not enter that merge.
Both filename orders produce identical combined bytes.
The combined report retains production, helper, and child positives under the same canonical identities.
It retains the explicit disconnected zero body record.
The missing assertion lines remain missing.
Aggregation does not repair source membership.

See the [reset baseline](clojure-original-line-coverage/baseline.lcov), [parent measurement](clojure-original-line-coverage/parent.lcov),
[child measurement](clojure-original-line-coverage/child.lcov), and [combined report](clojure-original-line-coverage/combined-forward.lcov).
The [failed location results](clojure-original-line-coverage/expected-locations.json) retain the unchanged original expectations.

## Decision input

[Choose collector mechanisms and exceptions after feasibility research](https://github.com/LowkeyLab/bazel-repo/issues/2445)
can use this bounded mechanism evidence.
Checksum-pinned provisioning, pre-load source instrumentation, explicit identities, declared outputs, and separate-child collection work for the tested forms.
Two limits remain before complete acceptance:

- The instrumenter omits executed test functions in Var metadata.
- Registered-form discovery requires namespace evaluation before the reset baseline exists.

The later decision must address those limits and the desired treatment of macro initialization.
This report proposes no source exception.
It preserves the existing Python architecture, combined LCOV workflow, thresholds, and Codecov policy.

## Checks and preserved evidence

| Check                                       | Outcome                                                  | Execution scope                                     |
| ------------------------------------------- | -------------------------------------------------------- | --------------------------------------------------- |
| Instrumentation, baseline, and child action | Orchestration passes, original-location acceptance fails | Fresh remote execution                              |
| Native maintained and research tests        | Both pass                                                | Two fresh remote tests                              |
| Both LCOV merge orders                      | Identical combined bytes                                 | Local merger with cached Bazel tool build           |
| Final formatted build                       | Pass                                                     | Cached action results, no fresh collector execution |
| Applicable lint aspects                     | Pass                                                     | Checkstyle, KeepSorted, and PMD                     |
| Full formatting                             | Pass                                                     | Repository formatter                                |
| Clojure source generator                    | Pass                                                     | Zero generated BUILD changes                        |

The [fresh collector invocation](https://app.buildbuddy.io/invocation/d8848b4b-788f-4a39-abed-131b4881a1b0)
records 52 action-cache hits and two remote actions.
The [native test invocation](https://app.buildbuddy.io/invocation/95312339-255a-4f06-997a-7c1b98221aff)
executes both tests, with one remote-cache hit and three remote actions.
The [formatted build invocation](https://app.buildbuddy.io/invocation/14e1ddd2-77fe-42d9-9025-58386d596001)
reuses 196 cached actions and performs one internal action.
The [lint invocation](https://app.buildbuddy.io/invocation/39fd0e3b-7a6b-407e-8ca7-17b3c8fdd398)
passes the applicable aspects with no findings.
Those aspects do not provide a Clojure-specific source linter.

The [first executed location failure](https://app.buildbuddy.io/invocation/2ae10fbb-54ee-4c19-bc6f-d8de074c2e5f)
remains a failed command.
A local reproduction preserves its raw forms and error report.
The later adapter records the same failed expectations and permits the independent cases to complete.
It changes orchestration, not source acceptance.
Earlier archive, dependency-cycle, and launcher failures remain setup evidence.
They do not establish collector capability or impossibility.

The [reproduction commands](clojure-original-line-coverage/commands.txt) retain the meaningful flags.
The [evidence archive](clojure-original-line-coverage/evidence.tar.gz) contains source snapshots, raw forms, reports, receipts, and completion records.
The [expanded test form](clojure-original-line-coverage/deftest-metadata.json) preserves the untouched metadata body.
The [source manifest](clojure-original-line-coverage/source-manifest.json) distinguishes historical, executed, and formatted inputs.
The [archive manifest](clojure-original-line-coverage/archive-manifest.json) records hashes of publication bytes.

Published logs use placeholders for checkout, home, cache, temporary, and evidence directories.
The archive retains technical versions, source identities, artifact hashes, process identities, and statuses.
Private raw originals remain intact.
No hosted CI, Codecov upload, or production acceptance ran.
