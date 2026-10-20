# Clojure assertions and compiler metadata baselines

Date: 2026-10-20.
Question: [Test Clojure assertion coverage and baselines without namespace evaluation](https://github.com/LowkeyLab/bazel-repo/issues/2481).
Base: `39075c1280f357d58b06a9a471e617cd73949bdf`.
Predecessor: [Clojure original-line research](https://github.com/LowkeyLab/bazel-repo/blob/66ad2ba16762002d7300dc649001a3b04e51a41a/docs/research/2026-10-20-clojure-original-line-coverage.md).
Scope: [Approved collector experiments](https://github.com/LowkeyLab/bazel-repo/issues/2445#issuecomment-6287451818).

## Result

A bounded Cloverage adapter records executed assertion bodies at their original lines.
A separate process reads compiler line metadata without evaluating a target namespace.
That metadata supplies disconnected body zeros and all nine controlled body locations.
It omits the original macro-definition body at `production.clj:7`.
It therefore does not supply the complete original-form denominator.

Metadata discovery and metadata production have different effects.
The discovery process creates no initialization marker and loads no target namespace.
The compiler process evaluates initialization when it produces metadata for resource-loaded namespaces.
This experiment does not establish metadata production without namespace evaluation.
The old counter-reset baseline remains a diagnostic artifact only.
It is not an unevaluated baseline.

The remaining feasibility question is precise:
How can an adapter retain original macro-body executable locations and provision metadata for resource-loaded sources without namespace evaluation?
A maintainer must resolve that question before selecting a complete Clojure baseline mechanism.
No source exception receives approval.

## Inputs and bounded mechanism

Current main retains Clojure `1.12.6` and `rules_clojure` commit `c0322b0251489b5d9f883e55ec093e90cb0dbdd9`.
The maintained sample sources and tests match the predecessor.
The fixture restores the predecessor's checksum-pinned Cloverage and Maven declarations on this separate research branch.
It uses Cloverage commit `e427762f75d3e1fe461554c08ad247a7365580d2`, Bazel `9.3.0`, and Java `25.0.4`.
The [Maven lock](../../clojure_sample/research/maven_install.json) records the twelve artifact pins.
The [source and process evidence](clojure-assertions-compiler-baseline/evidence.tar.gz) retains effective inputs and hashes.

Clojure's [deftest macro](https://github.com/clojure/clojure/blob/clojure-1.12.6/src/clj/clojure/test.clj) stores the test function in Var metadata.
The predecessor established that Cloverage did not traverse this function.
The research [adapter](../../clojure_sample/research/src/coverage_research/runner.clj) extends Cloverage's `:def` walker.
It wraps the `:test` metadata expression through the existing instrumenter, then delegates the original definition handling.
It preserves the original assertion forms and metadata.
It changes no maintained test source.
This bounded hook does not establish support for every metadata convention or test framework.

The parent instruments seven explicit namespaces before normal AOT loading can bypass instrumentation.
It preserves initialization counters, resets measured counters, and executes two tests with three assertions.
A separate child executes the child-only function and returns `42`.
The compiler process compiles the resource-loaded namespaces and inventories existing AOT classes from runtime jars.
The discovery process reads the resulting 34 class files through Clojure's bundled ASM reader.
It preserves 337 raw line entries with class, method, original line, and canonical source identity.
It never loads those target classes or requires those target namespaces.

The scanner uses an explicit namespace-to-path inventory and class-name prefixes.
This inventory is a bounded experiment, not automatic repository source discovery.
The initial scanner omitted existing AOT jar classes because `compile` reused those classes.
The final scanner inventories those existing classes too.
The evidence retains both runs and the correction.

## Original records

| Original location             | Parent measured record | Compiler baseline |
| ----------------------------- | ---------------------- | ----------------- |
| Maintained `core.clj:7`       | `DA:7,5`               | `DA:7,0`          |
| Maintained `core_test.clj:6`  | `DA:6,26`              | `DA:6,0`          |
| Research `production.clj:10`  | `DA:10,4`              | `DA:10,0`         |
| Research `production.clj:11`  | `DA:11,3`              | `DA:11,0`         |
| Research `helper.clj:4`       | `DA:4,3`               | `DA:4,0`          |
| Research `cases_test.clj:7`   | `DA:7,29`              | `DA:7,0`          |
| Research `cases_test.clj:8`   | `DA:8,29`              | `DA:8,0`          |
| Research `disconnected.clj:4` | `DA:4,0`               | `DA:4,0`          |
| Research `child.clj:4`        | Parent `0`, child `3`  | `DA:4,0`          |

The exporter counts covered forms per line, rather than function calls.
These counts do not represent assertion counts.
The [expected records](clojure-assertions-compiler-baseline/expected-locations.json) retain the original locations and actual records.
The [compiler expectations](clojure-assertions-compiler-baseline/metadata-expectations.json) retain the independent body-location checks.

The compiler baseline and original-form baseline contain identical line sets for six sources.
For `production.clj`, compiler metadata contains lines `1,3,4,6,9,10,11`.
The original-form collector also registers line `7`, the macro-definition body.
The [comparison](clojure-assertions-compiler-baseline/baseline-comparison.json) preserves this mismatch.
The runtime macro call at line `11` has a positive measured record.
That call does not restore the missing original macro-definition location in compiler metadata.

## Initialization and baseline limits

The disconnected namespace writes an external marker when its initialization executes.
The compiler process creates that marker.
The baseline discovery process does not create its separate marker.
The baseline also checks that all seven target namespaces remain absent before and after discovery.
Its source hashes match the measured source inventory.
All compiler-baseline line counts equal zero.
These observations establish metadata discovery without target namespace evaluation for this fixture.

Clojure's [compiler source](https://github.com/clojure/clojure/blob/clojure-1.12.6/src/jvm/clojure/lang/Compiler.java#L7839-L7846) calls both `emit` and `eval` for compiled top-level expressions.
The observed compiler marker records this lifecycle in the experiment.
Existing AOT classes permit metadata scanning without a new source evaluation.
The resource-loaded test and helper sources first need compiler artifacts.
This experiment produces those artifacts through evaluation.
It establishes no general side-effect-free source-to-baseline path.

The parent's initialization phase records 52 positive forms.
Those initialization reports remain separate from measured coverage and both merges.
The diagnostic reset baseline contains no positive forms.
Neither reset counters nor a successful compiler subprocess prove unevaluated source discovery.
The missing macro-definition location also prevents a claim that compiler metadata is a faithful full original-form baseline.

## Process, fault, and merge checks

The parent, compiler, baseline, and measured child run in separate JVM processes.
Completion records retain commands, process identities, and exit status.
The child collector checks source hashes, mode, process identity, expected source records, and child-only hits.
After functional assertions pass, missing child output and corrupt child output both fail collection validation.
Valid disconnected zero records remain valid.
The controls establish these bounded failures, not detection of every possible corruption.

The pinned Bazel LCOV merger receives compiler-baseline, parent-measured, and child-measured records.
Initialization and diagnostic reset records do not enter these merges.
Both merge orders preserve measured positives and disconnected zeros under canonical source identities.
The evidence records exact merger commands and output bytes.
A merged line from another input cannot establish that compiler metadata contained that line.

## Validation limits

The final collector action ran locally in a Bazel sandbox with remote executor, remote cache, and BES disabled.
The action executed freshly and passed its functional, location, child, and fault checks.
Its metadata comparison still identifies the macro-definition omission.
Two earlier required Gazelle invocations used configured remote services and returned cached tool actions.
They do not establish remote collector execution.

Configured remote validation remains subject to the coordinator's pending explicit approval request.
No fresh remote collector result or hosted upload exists in this report.
The generator, full repository formatting, and both native test targets pass locally.
The two native tests execute freshly with test-result caching disabled.
Applicable Checkstyle, KeepSorted, and PMD lint aspects pass with no findings.
Those aspects supply no Clojure-specific source lint.
Repository-wide build and lint acceptance remain outside this bounded experiment.
The inspected Cloverage exporter still supplies no branch records, as the predecessor records.
The report does not change Python architecture, combined LCOV, thresholds, Codecov policy, or production code.
