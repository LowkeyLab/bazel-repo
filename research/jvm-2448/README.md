# JVM metadata and collection investigation

This experiment answers [Validate JVM metadata baselines, original mappings, and child collection](https://github.com/LowkeyLab/bazel-repo/issues/2448).
It uses a temporary fixture. It changes no production collector.

The pinned runner can analyze disconnected Java and Kotlin metadata with empty execution data.
The native report does not preserve all original source identities.
It also omits the child report from the parent test report.
These results do not approve a collector mechanism or an exception.

## Inputs

The repository base is `5e246a47cd7ea0a9c506e915db0c053d7d84d023`.
Bazel is `9.3.0`.
The rules_java revision is `cd2030164287fdcbfc104036f7aebb788ef48c08`.
The rules_kotlin revision is `09931b5ce94251d94b803c0d0a6c535896e67d16`.
The pinned rules_java configuration selects Java tools `v22.0` from Bazel revision `c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95`.
The effective Java tools contain JaCoCo `0.8.14`.
The [tool input manifest](evidence/java-tools-inputs.json) preserves the effective coverage and JaCoCo jar hashes.
The [rules_java source](https://github.com/bazelbuild/rules_java/blob/cd2030164287fdcbfc104036f7aebb788ef48c08/java/repositories.bzl) records the Java tools revision.
The fixture uses the repository's Java 25 runtime settings.

See the [fixture](../../tools/coverage/jvm_fixture/BUILD.bazel), [source inventory](evidence/source-inventory.json), [class owners](evidence/class-owners.json), and [metadata manifest](evidence/metadata-manifest.json).
The metadata manifest records original class bytes and path manifests.
The evidence directory retains the instrumented jars with their original `.class.uninstrumented` entries.
The Java and Kotlin disconnected libraries do not occur in the test dependency graph.
Both contain initializers that throw if execution reaches them.

The generated [parent launcher](evidence/fixture_test-launcher.txt) and [child launcher](evidence/child-launcher.txt) record the effective runtime setup.
The parent starts the child through its generated Bazel launcher with the argument `take`.
The parent gives the child a separate `JAVA_COVERAGE_FILE` inside `TEST_UNDECLARED_OUTPUTS_DIR`.
The runner creates unique report and execution-data names.
The child uses the same pinned Java tools and instrumentation runtime as the parent.

## Native observations

| Required observation             | Result                                                                                                                                       |
| -------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Disconnected executable baseline | The metadata-only API emits real zero-hit `DA` records for both disconnected libraries. Initializers do not execute.                         |
| Java package mismatch            | The test executes `mismatch.DirectoryMismatch.value()`. Native measured LCOV omits its original file.                                        |
| Kotlin package mismatch          | The test executes `kodd.KotlinMismatchKt.mismatchValue()`. Native measured LCOV omits its original file.                                     |
| Duplicate basenames              | Both `shared.TwinKt.firstTwin()` and `shared.SecondTwin.secondTwin()` execute. Native LCOV assigns both methods to the first `Twin.kt` path. |
| Inline Kotlin                    | `selectKotlin(true)` executes the inline multiplication. Native LCOV records `FN:-1` for `doubled` and `DA:3,0` for its body.                |
| Branch identities                | Java `Executed.select` and Kotlin `selectKotlin` each emit a taken branch and an untaken branch at their controlled conditional.             |
| Child-only execution             | The child report contains `DA:7,1` for `Child.java`. The child completion marker and output confirm successful execution.                    |
| Parent aggregation               | Native combined LCOV does not contain the positive child-only line. Its placeholder for `Child.java` has `LF:0`.                             |
| Declaration-only source          | `Declaration.class` has no `Code` attribute. Its source record has no `DA` or `BRDA`, despite an abstract method record at line zero.        |

The [native measured report](evidence/native-measured.lcov), [native baseline](evidence/native-baseline.lcov), [native combined report](evidence/native-combined.lcov), and [child report](evidence/child.lcov) preserve these observations.
The native baseline contains source placeholders. Those placeholders alone do not establish executable coverage.
The separate [disconnected metadata baseline](evidence/disconnected-baseline.lcov) contains executable zero-hit lines.

## Source explanation

The pinned [runner constructor and metadata analyzer](https://github.com/bazelbuild/bazel/blob/c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95/src/java_tools/junitrunner/java/com/google/testing/coverage/JacocoCoverageRunner.java) accept empty execution data and supplied metadata jars.
The analyzer reads `.class.uninstrumented` entries. It does not load production classes.
This explains the successful disconnected baseline without initializer execution.

The pinned [LCOV formatter](https://github.com/bazelbuild/bazel/blob/c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95/src/java_tools/junitrunner/java/com/google/testing/coverage/JacocoLCOVFormatter.java) normally matches a source path by package/source suffix.
It also accepts an explicit `original-path////package/source` mapping.
A directory/package mismatch needs that explicit mapping.

The pinned [coverage analyzer](https://github.com/bazelbuild/bazel/blob/c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95/src/java_tools/junitrunner/java/com/google/testing/coverage/CoverageAnalyzer.java) combines classes by package plus `SourceFile` before formatting.
A formatter-only path substitution cannot separate the two `shared/Twin.kt` records after that combination.
An adapter must retain class ownership before this step.

## Bounded adapter

The temporary `Baseline` launcher calls the current runner API once for each supplied metadata jar.
The experiment isolates each original class in its own metadata jar.
Each jar supplies one explicit original-path mapping.
The fixture owner table supplies these mappings explicitly.
This table is an experimental input. It does not prove automatic compiler ownership discovery.

The adapter uses the parent and child execution probes from the successful remote test.
The public execution files preserve class IDs and probe arrays.
Their session names and timestamps use fixed replacement values.
This replacement removes worker identity without changing line or branch evidence.

| Capability                    | Bounded adapter result                                                                                                                 |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Disconnected executable lines | The current API emits zero-hit executable lines without loading the disconnected classes.                                              |
| Original paths                | Explicit owner mappings correct both package mismatches and separate the two `Twin.kt` paths.                                          |
| Tests and helpers             | The merged report includes the test source and the helper source under canonical original paths.                                       |
| Child-only execution          | The separately analyzed child probes retain the positive hit at `Child.java:7`.                                                        |
| Baseline merging              | Both input orders produce identical bytes and one source identity for each of 11 subject sources.                                      |
| Supported branches            | Both input orders preserve taken branches, untaken branches, and the fixture's branch keys.                                            |
| Missing child input           | Removing the expected execution file causes `NoSuchFileException` and exit 1 after the functional run passed.                          |
| Corrupt child input           | Corrupt bytes cause `Invalid execution data file` and exit 1 after the functional run passed.                                          |
| Original line bounds          | The combined report contains no out-of-range `DA` line. It retains the invalid function sentinels described below.                     |
| Inline Kotlin                 | The adapter still records zero hits for the executed inline body and retains `FN:-1`. Correct original attribution remains unresolved. |
| Automatic class ownership     | The explicit fixture owner table does not prove compiler-derived ownership for arbitrary multi-source targets.                         |

See the [adapter results](evidence/adapter-results.json), [parent report](evidence/adapter-parent.lcov), [child adapter report](evidence/adapter-child.lcov), and [baseline](evidence/adapter-baseline.lcov).
The [forward merge](evidence/merged-forward.lcov) and [reverse merge](evidence/merged-reverse.lcov) preserve the same positive hits.
The [negative controls](evidence/negative-controls.json) retain each failure separately.
These controls execute the adapter locally against artifacts from the passed remote test.
They do not establish native collector failure detection.

The experiment establishes metadata analysis, explicit owner isolation, and explicit child-input validation as usable primitives.
It does not establish a complete JVM collector adapter.
Correct inline original-source attribution remains a demonstrated failure of this native-analyzer candidate.
Automatic compiler-derived ownership remains unproven.
A further bounded investigation can reuse this fixture to test those two requirements before a mechanism choice.
No exception follows from either limit.

## Evidence limits

The [remote result](evidence/remote-test-result.json) records a fresh configured-remote test execution.
The [BuildBuddy invocation](https://app.buildbuddy.io/invocation/65c2bb1f-b1df-4a67-83c1-cb596f8817b0) records that run.
The run disables test-result reuse.
Cached compilation does not establish fresh compilation.
The metadata-only baseline and adapter executions ran locally. Their launchers built through the configured remote service.
[Reproduction commands](REPRODUCE.md) record the exact inputs, configuration, and control sequence.
The [coverage options](evidence/coverage-options.json) preserve collector flags in their original order. Credential headers remain private.
The 11 coverage subjects exclude the temporary analysis adapter itself. This is an experiment boundary, not a source-membership exception.
No hosted GitHub Actions run or Codecov upload forms part of this experiment.

The fixture owner mapping is explicit and bounded.
This experiment does not establish a general mapping from arbitrary Kotlin classes to original source owners.
The current inline-function observation does not establish correct attribution for all Kotlin SMAP cases.
The native `FN:-1` and declaration `FN:0` records need an explicit output policy.
A later mechanism decision must preserve the approved membership policy and original-source requirements.

The Python architecture, its remaining acceptance work, combined LCOV workflow, thresholds, and Codecov policy remain outside this experiment.

## Validation

Required Gazelle checks passed.
`aspect format --scope=all` failed with the default Java 25 runtime because google-java-format called an unavailable javac method.
The same whole-tree format command passed with hermetic Java 21 runtime and language flags.
The fixture coverage check then passed with the default Java 25 settings against the formatted source.
The format compatibility flags changed no repository dependency pins.

The focused `aspect lint //tools/coverage/jvm_fixture:all` check did not complete.
The repository exports `//:pmd.xml`, but that input file is absent at the recorded base.
This tooling failure does not establish a source lint result. The experiment does not change lint provisioning.
The [validation record](evidence/validation.json) preserves the separate check outcomes.

The research branch contains experimental code and evidence only.
It has no pull request and requires no production merge.
