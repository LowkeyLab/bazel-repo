# Evidence for complete coverage reports

Date: 2026-10-18

Research question

[Determine how to prove complete coverage reports across languages](https://github.com/LowkeyLab/bazel-repo/issues/2416).

Inspected repository commit: `e9040f81672138d94a60e4c996a190f929394da6`.

## Proposed decision

Define inclusion through the combined LCOV report. A selected test, an instrumented target, or an `SF` record alone does not prove complete coverage.

Use separate evidence for source discovery, graph selection, instrumentation, collection, merging, and upload. Compare expected source identities with report identities. Check executable lines independently of source presence. Preserve existing percentage thresholds.

This report proposes acceptance cases. It does not implement enforcement or establish runtime success.

## Terms

- **Eligible source:** A production source that the selected configuration and explicit policy include.
- **Expected inventory:** The eligible sources, their owning targets, and their supported coverage dimensions.
- **Zero-hit source:** A source with executable lines whose execution counts all equal zero.
- **Orphan source:** A declared source outside every selected test's dependency closure.
- **Source identity:** The repository-relative path for one original source at one commit.
- **Coverage dimensions:** Line, function, or branch data that a collector can produce.

The expected inventory must exist independently of the instrumentation filter. Otherwise, an incorrect filter can remove a source from both expectation and observation.

## Repository constraints

The repository pins Bazel 9.3.0. Coverage enables combined LCOV, test instrumentation, and a manual project filter. It disables `build_tests_only`. However, `build_tag_filters=python-coverage` restricts non-test roots from wildcard selection. Broad instrumentation does not expand the selected graph. An orphan Go target still needs selection. [Repository configuration](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelrc#L89)

The Python wrappers generate baselines for declared first-party production sources. They exclude generated files, test-only targets, external repositories, and `3rdparty/` packages. The accepted Python ADR requires `aspect_rules_py`. Preserve this mechanism and its policy. [Python wrappers](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/defs.bzl), [Python ADR](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/docs/adr/0002-use-rules-py-for-first-party-python.md)

The existing Python membership test creates ordinary and hidden packages. It supplies explicit hidden-package roots and checks orphan sources and collection failures. Its source establishes available acceptance machinery. This investigation did not execute that test. [Membership check](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/coverage_membership_test.py)

The coverage wrapper checks that a combined report exists. It does not compare source membership or require executable records. CI uploads that report to Codecov with upload errors enabled. Successful upload does not establish source completeness. [Coverage wrapper](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/coverage/coverage.sh), [Hosted workflow](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.github/workflows/coverage.yml)

## Report evidence

LCOV uses `SF` for a source path and `DA` for a line number and execution count. `BRDA` identifies a branch and its count. `FN` and `FNDA` describe functions. A report can name a source without describing any executable line. Therefore, presence alone cannot establish a useful denominator. [LCOV trace file format](https://github.com/linux-test-project/lcov/blob/v2.3.1/man/geninfo.1#L1296)

Bazel's generic baseline writes source names without line counts. Its report printer can therefore produce `LF:0` and `LH:0`. Such records cannot represent untested executable code accurately. A language baseline must supply real executable lines when the acceptance case requires them. [Bazel baseline writer](https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/analysis/test/BaselineCoverageAction.java), [Go baseline explanation](https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/private/actions/baseline_coverage.bzl)

Bazel merges coverage by source identity. A baseline path and an execution path must match. Otherwise, zero-hit and executed data can form separate records. Check one canonical source path after merging. Do not require identical function names or branch meanings across languages. [Bazel merger](https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator/Coverage.java), [Go identity requirement](https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/private/actions/baseline_coverage.bzl#L76)

## Pinned language support

### Go

Pinned `rules_go` commit `7d3f58b093ddab21ecb4b429d8eefafaf64999f1` supplies real baselines. `GoBaselineCoverage` uses `go tool cover` metadata to obtain executable lines. It emits `DA:<line>,0` for packages that no test links. It also filters platform build constraints. Files without executable lines can correctly retain `LF:0`. [Baseline generator](https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/tools/builders/baseline_coverage.go)

The rule emits these baselines only when the configuration enables instrumentation and coverage. It also requires Bazel's baseline API and the toolchain's `covdata` tool. Check these conditions before classifying a missing record as unsupported. Select orphan roots separately from broad instrumentation. [Baseline rule](https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/private/actions/baseline_coverage.bzl)

Go maps import paths to execution-root source paths during instrumentation. Require matching paths between baseline and measured records. Treat Go statement or line coverage separately from branch coverage. This investigation does not establish Go branch records or arbitrary subprocess collection. [Go instrumentation](https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/tools/builders/cover.go)

### Python

The repository uses `aspect_rules_py` 1.12.1 with a local patch. Production wrappers use `coverage.py` to generate line and branch baselines. The baseline generator supplies sources explicitly and enables branch collection. This supports orphan sources when Bazel selects their production roots. [Dependency pin](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/MODULE.bazel#L15), [Baseline generator](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/coverage_baseline.py)

The runtime helper starts child collection through a private Python environment hook. It requires completion receipts and rejects missing or corrupt child data. This supports children that retain the configured environment and startup hook. It does not prove collection in an unrelated interpreter, a container, or a process that removes those settings. [Runtime helper](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/coverage_runtime.py), [Runtime checks](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/coverage_runtime_test.py)

### Rust

Pinned `rules_rs` commit `f6067c1f0568e24487468a01b7ae7df0b87434cd` provisions `rules_rust` commit `fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3`. The Rust rules use LLVM source instrumentation and respect Bazel's filter. The `rust_test(crate=...)` case can instrument its underlying library. Preserve the repository's explicit test instrumentation setting. [Archive pin](https://github.com/hermeticbuild/rules_rs/blob/f6067c1f0568e24487468a01b7ae7df0b87434cd/rs/rules_rust.bzl), [Rust coverage documentation](https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/docs/src/coverage.md)

The collector merges `.profraw` files and exports the test binary through `llvm-cov`. It excludes external and temporary source paths. It does not enumerate separate child executables as additional export objects. Therefore, a child profile alone does not establish complete coverage for a separate binary. [Rust collector](https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/coverage/collect_rust_coverage.rs#L155)

The collector deliberately emits an empty report when profiles or binary coverage data is absent. This can represent excluded instrumentation. It can also conceal an unexpected collection gap. Check an expected executed line independently. This inspection found no real executable-line baseline for a wholly orphan Rust crate. Classify that capability as unproven. [Empty-report behavior](https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/coverage/collect_rust_coverage.rs#L169), [Rust providers](https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/private/rust.bzl)

Do not assume LLVM line coverage implies branch coverage. Require an actual branch record for any proposed Rust branch guarantee.

### Java and Kotlin

The repository pins `rules_java` commit `500d0a792c747d050f788d1c38c3d92f175d5e93`. It pins `rules_kotlin` commit `09931b5ce94251d94b803c0d0a6c535896e67d16`. Kotlin enables instrumentation through the worker and propagates `.kt` and `.java` source metadata. The provider does not supply a real baseline file. [Repository pins](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/MODULE.bazel#L99), [Kotlin compilation](https://github.com/bazel-contrib/rules_kotlin/blob/09931b5ce94251d94b803c0d0a6c535896e67d16/kotlin/internal/jvm/compile.bzl#L970)

The Java rule creates a coverage manifest from the runtime class path. Bazel's JaCoCo runner analyzes supplied class metadata. Its formatter can emit lines, functions, and branches for those classes. This supports classes without execution hits in the analyzed metadata. It does not establish real line baselines for arbitrary libraries outside that metadata. [Java manifest](https://github.com/bazelbuild/rules_java/blob/500d0a792c747d050f788d1c38c3d92f175d5e93/java/common/rules/impl/java_binary_impl.bzl#L322), [JaCoCo runner](https://github.com/bazelbuild/bazel/blob/9.3.0/src/java_tools/junitrunner/java/com/google/testing/coverage/JacocoCoverageRunner.java#L177), [JaCoCo formatter](https://github.com/bazelbuild/bazel/blob/9.3.0/src/java_tools/junitrunner/java/com/google/testing/coverage/JacocoLCOVFormatter.java#L143)

Check Kotlin source identities after JaCoCo mapping. The repository's HTML wrapper tolerates negative generated JVM line numbers. Therefore, successful HTML generation cannot establish valid original-source records. Separate generated JVM methods from production-source membership. [HTML wrapper](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/coverage/coverage.sh#L50)

### Script languages

Pinned `aspect_rules_js` commit `e67442a2ccf329841c9227d66ee013afcf5633c6` includes V8 collection and a manifest-driven report generator. It enables `all: true` and restricts reporting to manifest files. Supported extensions include `.js`, `.ts`, `.jsx`, `.tsx`, `.mjs`, `.cjs`, `.mts`, and `.cts`. Missing runfiles do not appear through its manifest enumeration. [JavaScript report generator](https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/coverage/coverage.js#L16084)

The bootstrap enables child profiles and source-map collection. It fails when report generation fails. This supports eligible files in the selected test manifest. It does not independently discover an orphan project outside the test graph. Check original TypeScript paths, child-only execution, and zero-hit files separately. [JavaScript bootstrap](https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/node-bootstrap/coverage.cjs), [JavaScript rule](https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/js_binary.bzl#L906)

Angular also has custom Vitest adapters. The inspected Predix adapter copies Vitest LCOV and prefixes `angular/` paths. If the input is absent, it prints a message and returns. Require an expected executed-source record to detect this gap. Do not assume native JavaScript collection proves the adapter's output. [Angular configuration](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/angular/angular.json), [Predix adapter](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/angular/projects/predix/vitest-global-setup.ts)

### Other rules

The repository includes Clojure. Pinned `rules_clojure` commit `c0322b0251489b5d9f883e55ec093e90cb0dbdd9` declares libraries through `JavaInfo` and delegates tests to Java. This alone does not establish original `.clj` instrumentation or usable line mapping. Record Clojure source coverage as unproven. Do not classify it as a working language merely because its tests run on the JVM. [Clojure rules](https://github.com/griffinbank/rules_clojure/blob/c0322b0251489b5d9f883e55ec093e90cb0dbdd9/rules.bzl), [Clojure sample](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/clojure_sample/src/lowkeylab/clojure_sample/BUILD.bazel)

Shell tests, browser tests, code generators, and packaged executables need explicit collector evidence for the code they execute. A test label does not identify every language or process it starts.

## Proposed acceptance cases

These cases require controlled fixture sources with known executable lines. Their results should remain separate from percentage thresholds.

| Case                           | Required observation                                                                                                                 | Failure that the case detects                                       |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------- |
| New ordinary project           | Add a supported project without changing a central project list. Its original source appears with known positive and zero-hit lines. | Manual inclusion lists or missing collector wiring.                 |
| Hidden package                 | Add an eligible package beneath a hidden directory. Discovery identifies its BUILD target. Its known lines appear in combined LCOV.  | Directory-name assumptions or missing explicit roots.               |
| Orphan production source       | Check that no selected test depends on the source. Require real zero-hit executable lines through supported baselines.               | A filter that covers only the selected test graph.                  |
| Explicit exclusion             | Record its reason and scope. Its source remains absent. A nearby eligible source remains present.                                    | An exclusion that also removes sibling projects.                    |
| Broken collection              | Remove or corrupt expected collection data while functional tests pass. Require a failing invocation or a failing completeness test. | A successful report containing only baselines or partial languages. |
| Branch and subprocess          | Execute one branch or child-only line. Require its supported records and preserve the alternative without execution hits.            | Parent-only collection or missing branch support.                   |
| Merge identity                 | Exercise one source from two tests. Require one canonical path with both observations.                                               | Path aliases, duplicate denominators, or lost records.              |
| Generated and external sources | Apply the explicit language policy. Check original sources separately from generated outputs and external dependencies.              | Accidental inclusion or silent removal of original sources.         |

The hidden-package case must distinguish ignored paths from eligible paths. Preserve `.bazelignore`. Do not assume every hidden directory must appear in `//...`. Record discovery results and command roots explicitly. [Ignored paths](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelignore), [Existing hidden-package test](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/coverage_membership_test.py)

Run an acceptance case per collector family. One passing Python fixture does not establish Rust, JVM, JavaScript, or browser-process behavior.

## Evidence levels

| Evidence               | What it can establish                                                            | Additional evidence required                                              |
| ---------------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Source inspection      | Pins, graph rules, intended collector behavior, and candidate acceptance checks. | Runtime reports.                                                          |
| Local execution        | Collection and merging for the local configuration and actual executed tests.    | Remote execution and hosted upload.                                       |
| Fresh remote execution | Worker collection and output transfer when relevant actions execute remotely.    | Worker identity, no fallback, cache status, report artifacts, and upload. |
| Cached test results    | Reuse of previous outputs for matching action keys.                              | Original execution provenance and a fresh run for new worker behavior.    |
| Hosted upload          | A service accepted the specified report at the specified commit.                 | Source inventory comparison and service-side path agreement.              |

Record the exact commit, Bazel version, command, configuration, invocation identity, platform, and executed-versus-cached counts. Preserve raw test reports and the combined report. Record any local fallback separately.

For fresh remote evidence, prevent test-result reuse and remote cache acceptance as needed. Check execution metadata rather than inferring execution from flags. Preserve the existing local `CoverageReport` strategy and output-download configuration until a separate decision changes them. [Remote coverage configuration](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelrc#L98)

## Checks without new thresholds

A later implementation can compare sets and structural facts without adding percentage policy:

1. Compare eligible source identities with canonical `SF` identities.
2. Report unowned production files separately from declared orphan sources.
3. Require known executable lines in controlled zero-hit fixtures.
4. Require positive counts for controlled executed lines.
5. Require branch records only for a collector with established branch support.
6. Reject unexpected source aliases, missing child data, and collection failures.
7. Compare the report identity and digest with the uploaded artifact.

Do not require every source to contain executable lines. Declaration-only files can correctly contain no `DA` records. Use language metadata or controlled fixtures to distinguish that case from an empty baseline.

## Remaining decisions and limits

The inventory mechanism, exclusion policy, and treatment of unsupported dimensions remain separate design decisions. No proposal here changes percentage thresholds or replaces the Python coverage work.

This investigation inspected repository and upstream source. It did not execute coverage fixtures, remote workers, or hosted uploads. It does not establish a complete current report. It also does not establish orphan baselines for Rust, JVM, Clojure, or arbitrary browser processes.
