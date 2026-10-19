# Original-source coverage for JVM and Clojure code

## Result and evidence limits

Java and Kotlin supply a viable class-based collector. The current runner analyzes compiled classes without execution probes. It emits executable zero-hit lines for supplied metadata. This capability does not discover disconnected sources or collect arbitrary child JVMs. Original Kotlin mappings and child collection need a bounded execution check. [Runner][runner] [Analyzer][analyzer] [Formatter][formatter]

The Clojure rules provide neither coverage instrumentation nor original-source coverage metadata. This is a demonstrated integration gap in these rules. It does not establish that Clojure coverage is impossible. Cloverage provides a candidate original-form instrumenter and LCOV writer. Its Bazel integration needs a separate feasibility check. [Clojure jar][clj-jar] [Cloverage instrumenter][clover-inst] [Cloverage LCOV][clover-lcov]

This report records source inspection. It adds no runtime coverage, fresh remote execution, cache provenance, or hosted upload evidence. The earlier reports also supplied no JVM or Clojure execution evidence. Their findings establish the starting questions, rather than successful collection. [Mechanism research][previous-mechanisms] [Evidence research][previous-evidence]

No collector exception receives approval here. Preserve the accepted Python architecture, combined LCOV workflow, thresholds, and Codecov policy. Production code, test code, executable helpers, and disconnected maintained sources remain eligible. [Membership decision][membership] [Investigation decision][investigation]

## Current pins and prior evidence

Inspection used refreshed `origin/main` at `151e15cd495b5d63155753c0acf09f50102f0cbc` on 2026-10-19. The pool acquisition refreshed main before this checkout started.

| Component       | Current pin or provenance                                  | Consequence                                            |
| --------------- | ---------------------------------------------------------- | ------------------------------------------------------ |
| Bazel           | `9.3.0`                                                    | Defines report orchestration and default baselines.    |
| `rules_java`    | `cd2030164287fdcbfc104036f7aebb788ef48c08`                 | Replaces the earlier report's `500d0a7...` Java pin.   |
| Java tools      | `v22.0`, source `c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95` | Supplies the actual JVM runner and analyzer.           |
| JaCoCo          | `0.8.14` in the inspected Java tools artifact              | Determines instrumentation and analysis compatibility. |
| `rules_kotlin`  | `09931b5ce94251d94b803c0d0a6c535896e67d16`                 | Same Kotlin pin as the earlier reports.                |
| `rules_clojure` | `c0322b0251489b5d9f883e55ec093e90cb0dbdd9`                 | Same Clojure pin as the earlier reports.               |

Sources: [repository pins][modules], [Bazel version][bazel-version], [Java tools configuration][java-tools], and [published Java tools release][java-release]. The inspected external artifact's BUILD file names JaCoCo `0.8.14`. The release digest in `java/repositories.bzl` identifies that artifact.

Reproduce the provenance check from `JAVA_TOOLS_CONFIG` in the pinned rules source. It names `java_tools-v22.0.zip` with SHA-256 `b13ee1eaa8681d3ec4faffab4251902a0c1b5135b1f5c718831fc5798db82f9b`. Its `source_revision` is `c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95`. The release notes independently name that source revision. [Java tools configuration][java-tools] [Release][java-release]

The extracted archive member `BUILD`, lines 68–76, names `java_tools/third_party/java/jacoco/org.jacoco.agent-0.8.14.jar` and `org.jacoco.core-0.8.14.jar`. The inspected extraction is `rules_java++toolchains+remote_java_tools` under the Bazel output base. This report checked that extracted member. It did not independently download and rehash the archive.

The earlier JVM report cited Bazel `9.3.0` runner sources. Current Java tools use the later source revision above. That revision replaces the old formatter API with `CoverageAnalyzer` and `CoverageData`. Use the current sources for JVM capability claims. The prior broad-filter and metadata-boundary findings still apply. [Current runner][runner] [Current formatter][formatter] [Previous evidence][previous-evidence]

The current coverage filter includes `mindreadr`. It omits `ktor_tutorial` and `clojure_sample`. The non-test build filter selects `python-coverage` roots. These are selection limits, separate from collector capability. `--instrument_test_targets` is already enabled. [Coverage configuration][bazelrc]

The tracked-file scan found Kotlin application sources, tests, and executable helpers. It found no maintained `.java` file. Java remains relevant through mixed-source rule support and future membership. The Clojure sample uses an AOT production namespace and a resource-loaded test namespace. [Kotlin helper][kt-helper] [Clojure production declaration][clj-prod] [Clojure test declaration][clj-test]

## Capability matrix

“Source capability” means the pinned code contains the mechanism. “Unproven” means the required execution evidence remains absent.

| Required case                    | Java                                                                                  | Kotlin                                                                     | Clojure                                                                                     |
| -------------------------------- | ------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Executed original lines          | Source capability through offline classes and paths.                                  | Source capability. Original transformed locations need execution evidence. | Current rules omit instrumentation. Cloverage is a candidate.                               |
| Zero-hit classes inside metadata | Source capability without execution probes.                                           | Same runner capability, with original classes in the jar.                  | Current rules supply no corresponding coverage metadata.                                    |
| Disconnected zero-hit sources    | Needs automatic roots and metadata analysis adapter.                                  | Same requirement. A library build does not run the analyzer.               | Needs independent namespace discovery and instrumentation adapter.                          |
| Tests and executable helpers     | Instrumentation can include test classes. Check selected owners and paths.            | Source metadata covers selected test/helper targets.                       | Tests load source resources. `JavaInfo` does not instrument forms.                          |
| Branches                         | Source capability for mapped bytecode branches.                                       | Source capability, with compiler-filter and mapping limits.                | Inspected Cloverage LCOV emits no branches. Other mechanisms remain unproven.               |
| Separate JVMs                    | Launcher supports per-JVM reports. Arbitrary JVM processes need collection setup.     | Same requirement, including the matching offline runtime.                  | Needs instrumentation and export inside each child.                                         |
| Canonical paths                  | Embedded paths provide a mechanism. Unmatched paths disappear.                        | Package/path mismatches and inline mappings need checks.                   | Namespace/resource identities need explicit repository mappings.                            |
| Missing data                     | Some I/O failures halt the runner. Missing probes can legitimately produce zero hits. | Same ambiguity. Filenames alone cannot detect lost executed hits.          | No present coverage contract. Adapter must distinguish missing output from valid zero hits. |

Sources: [Java manifest][java-manifest], [Java instrumentation][java-instrument], [Kotlin instrumentation][kt-instrument], [Kotlin provider][kt-compile], [runner][runner], [analyzer][analyzer], [formatter][formatter], [Clojure jar][clj-jar], and [Cloverage LCOV][clover-lcov]. No cell claims demonstrated end-to-end behavior.

## JVM metadata and disconnected sources

The Java binary creates its coverage manifest from `runtime_classpath`. Its coverage configuration supplies that manifest and `SINGLE_JAR_TOOL`. Kotlin's launcher creates a metadata list from runtime jars. These lists describe selected executables. They are not inventories of all maintained sources. [Java manifest][java-manifest] [Java configuration][java-helper] [Kotlin launcher][kt-launcher]

Java's instrumentation processor copies original classes to `.class.uninstrumented` entries before modifying executable classes. Kotlin does the same for Kotlin, Java, and generated class directories. Kotlin writes `-paths-for-coverage.txt` from its Java and Kotlin source lists. These mechanisms preserve analysis inputs without executing each class. [Java instrumentation][java-instrument] [Kotlin instrumentation][kt-instrument]

The runner analyzes `.class.uninstrumented` entries. `CoverageAnalyzer` still processes a class when its execution probes are absent. The formatter emits `DA` values of zero or one. It emits branch outcomes of zero, one, or `-`. These values represent hit state, rather than execution frequency. [Runner][runner] [Analyzer][analyzer] [Formatter][formatter]

An unexecuted class inside supplied metadata differs from a source outside that metadata. The former has a source-level baseline mechanism. The latter needs automatic root discovery and explicit analysis inputs. Building a library alone does not invoke the runtime analyzer. Bazel's default baseline contains filenames without executable `DA` records. [Java manifest][java-manifest] [Runner][runner] [Default baseline][bazel-baseline]

A candidate adapter can analyze selected libraries' original classes with empty execution data. It must retain embedded source paths. Another candidate uses a coverage-only launcher with the selected jars. Either candidate can supply real baselines through the existing LCOV merge. Neither requires a new source parser. Neither receives implementation approval here. [Analyzer][analyzer] [Runner][runner]

## Tests, mappings, branches, and merging

Kotlin declares sources from `srcs`, with extensions `kt` and `java`. Its instrumentation provider traverses `associates`, `deps`, `exports`, `runtime_deps`, and `data`. It supplies no custom executable-line baseline. Tests and helpers still need instrumentation and inclusion in selected metadata. [Kotlin compilation][kt-compile]

The formatter resolves package-qualified source names against embedded paths. It supports explicit `repository-path///package/source` mappings and suffix matching. It silently skips a source when no path matches. The analyzer groups data by package-qualified source filename before path selection. Same-package duplicate basenames therefore present a source-level collision risk. A fixture must establish affected Kotlin layouts. Basename-only repair cannot establish canonical identity. [Analyzer][analyzer] [Formatter][formatter]

Kotlin's writer supplies source paths without constructing explicit `///` mappings. Kotlin package declarations can differ from directory paths. This case needs a fixture. Inline calls, lambdas, `when`, default arguments, and declaration-only sources also need original-line checks. [Kotlin instrumentation][kt-instrument] [Formatter][formatter]

The current analyzer applies JaCoCo filters through class and method probe mappers. It retains the source debug extension for filter context. This establishes filtering capability. It does not establish complete original Kotlin mappings for every transformation. [Class mapper][class-mapper] [Method mapper][method-mapper]

Require supported bytecode branches with stable identities across repeated observations. These records do not represent every source-language decision. JaCoCo's documented branch model excludes exception handling. The formatter uses per-line branch indices. Baseline and measured reports must preserve their order before merging. [Formatter][formatter] [Analyzer][analyzer] [JaCoCo counters][jacoco-counters]

Measured hits must survive zero-baseline merging in both input orders. Compare canonical `SF` paths, known `DA` lines, branch identities, and positive outcomes. The pinned merger combines source records and line observations. This supplies a merge mechanism, rather than end-to-end adapter evidence. [Source merger][merger-source] [Line merger][merger-line]

Filename presence alone cannot establish this property. The repository's HTML command ignores several LCOV format and mismatch errors. Successful HTML generation therefore does not check original-source records. [HTML wrapper][coverage-script] [Membership decision][membership]

## Separate JVMs and missing collection data

The runner installs a shutdown hook and creates unique `.exec` and `.dat` paths. It obtains the current JVM's agent data and generates that JVM's report. Its comments explicitly allow subprocess output without overwriting another report. This supports instrumented child launchers at source level. [Runner][runner]

A raw `java`, packaged executable, or custom `ProcessBuilder` process does not receive coverage merely because its parent uses the runner. Each child needs instrumented classes, a matching runtime, metadata, output paths, and a dump/report operation. Offline instrumentation requires the matching JaCoCo runtime. Long-lived or children that receive forced termination need an explicit dump and completion protocol. [Java instrumentation][java-instrument] [Kotlin instrumentation][kt-instrument] [Runner][runner] [JaCoCo FAQ][jacoco-faq]

The runner halts with status one for an `IOException` during report generation. However, a missing live agent can also represent successful execution without instrumented code. The runner then analyzes empty data. Lost execution probes therefore resemble valid zero coverage. [Runner][runner] [Analyzer][analyzer]

A proposed adapter must record expected processes and completed outputs. Preserve child identity, metadata identity, artifact digests, exit state, and report identity. Reject missing or corrupt expected outputs before baseline merging. A controlled child-only positive line detects lost measurement that a zero baseline would conceal. These describe proposed requirements, rather than existing runtime guarantees.

## Independent Clojure instrumentation

`clojure_library` delegates jar construction to `clojure_jar_impl`. That implementation returns `DefaultInfo` and `JavaInfo`, without `InstrumentedFilesInfo`. It passes no coverage flag to the worker. It creates no `.class.uninstrumented` entries or coverage path manifest. [Clojure rule][clj-rule] [Clojure jar][clj-jar]

The sample declares production `core.clj` through `srcs`, `resources`, and `aot`. Its test library declares `core_test.clj` only through `resources`. `clojure_test` invokes `java_test` with a custom runner and `use_testrunner = False`. Thus AOT production code and runtime-loaded test forms cross different instrumentation boundaries. [Production declaration][clj-prod] [Test declaration][clj-test] [Clojure rule][clj-rule]

The compile worker handles `.clj` and `.cljc` namespace resources and AOT compilation. The test runner requires requested namespaces and runs `clojure.test`. Neither contains an original-form coverage pass or LCOV export. AOT debug information alone does not establish original `.clj` coverage. The current JVM collector also requires its special original-class entries and paths. [Compile worker][clj-compile] [Test runner][clj-runner] [JVM runner][runner]

Cloverage at `e427762f75d3e1fe461554c08ad247a7365580d2` is an independently inspected candidate. It is not a repository dependency. Its loader accepts instrumentation roots separately from test roots. It instruments selected namespaces before tests. Its instrumenter propagates original line metadata through forms. Its LCOV writer emits `SF` and `DA` records for instrumented forms. [Cloverage loader][clover-coverage] [Instrumenter][clover-inst] [LCOV writer][clover-lcov]

Required adapters include hermetic provisioning, source-owner namespace discovery, pre-load instrumentation, canonical paths, and Bazel output export. Test namespaces and executable helpers must join the instrumentation set. AOT loading must not bypass instrumented forms. [Cloverage loader][clover-coverage] [Compile worker][clj-compile] [Membership decision][membership]

The inspected Cloverage LCOV writer emits no `BRDA` records. This is a source-level limit of that exporter. It is not proof that no Clojure branch collector exists. Macros, unexecuted namespaces, child processes, and Bazel collection remain unproven. These limits do not justify a whole-language exception. [Cloverage LCOV][clover-lcov]

## Two bounded feasibility investigations

### JVM metadata, original mappings, and child collection

Question: Can current Java tools analyze selected disconnected jars and collect child-only hits with canonical Java and Kotlin records?

Use one temporary Bazel fixture with Java and Kotlin targets. Include an executed library, a disconnected library, tests, and a helper. Add a package/directory mismatch, duplicate basenames, inline Kotlin, and a declaration-only source. Add a child-only branch in a separate JVM.

1. Use broad instrumentation and `--instrument_test_targets` without changing production configuration.
2. Preserve source owners, class jars, original classes, path manifests, and effective JVM commands.
3. Analyze disconnected metadata with empty data through the current runner API or a temporary coverage-only launcher.
4. Execute the normal test and child with unique outputs.
5. Merge zero baselines and measured reports in both input orders.
6. Remove one expected child report and corrupt another while functional tests still pass.

Required observations:

- Each known executable line has one canonical repository-relative `SF` identity.
- Disconnected executable lines contain `DA:<line>,0` without executing static initializers to produce the baseline.
- Test code and helpers contribute original executable records.
- Package mismatches and duplicate basenames preserve distinct, accurate identities.
- Kotlin inline and generated locations create no out-of-range original line claims.
- Controlled branches retain taken and untaken outcomes after repeated merging.
- The child-only line receives a positive hit without parent execution of that line.
- Baselines preserve positive hits regardless of input order.
- Missing or corrupt expected child output causes collection failure.
- Compiler evidence establishes declaration-only classification.

This experiment settles adapter feasibility and mapping limits. Preserve exact revisions, commands, artifacts, raw reports, combined reports, and execution/cache status. Local success does not establish configured-remote transfer or hosted acceptance. Those checks follow a collector choice.

### Clojure original-line instrumentation

Question: Can a hermetic Cloverage adapter measure original `.clj` production, test, helper, disconnected, and child-only forms under these rules?

Use one temporary fixture with the sample's AOT production path and resource-loaded test path. Add disconnected and helper namespaces, macro expansion, a multiline form, and a child-only line. Pin the candidate and its dependencies before execution.

1. Instrument selected namespaces before loading AOT or test forms.
2. Include tests and helpers in instrumentation independently of test discovery.
3. Preserve tracked forms and original file/line metadata before LCOV conversion.
4. Export per-process reports through a declared Bazel collection path.
5. Merge baseline and measured records under the same identities.
6. Repeat with missing and corrupt child artifacts.

Required observations:

- Known executed `.clj` lines receive positive hits at exact original paths and lines.
- Disconnected executable forms supply zero-hit lines without unexplained namespace omissions.
- Namespace initialization does not masquerade as a zero-hit baseline.
- Tests and helpers receive equivalent source-membership treatment.
- AOT loading does not bypass instrumentation.
- Macro and multiline records follow documented original-form semantics.
- Child-only execution contributes a positive original-line record.
- Missing output fails validation while valid zero-hit output remains valid.

The inspected candidate provides line LCOV only. Record that exporter's branch limit explicitly. Consider another exporter only if the later decision requires additional branch mechanisms. Do not infer an exception from an unexecuted experiment.

## Decision inputs

Prefer existing Java/Kotlin instrumentation and analysis over a new source parser. Require automatic roots, metadata baselines, checked identities, and process completion evidence. Check the adapter through the JVM experiment before selecting implementation.

Require independent Clojure evidence before selecting its collector or proposing an exception. Any future exception must name affected rules, sources, dimensions, scope, reasons, and evidence limits. This report proposes no exception. Both experiments remain separate from production implementation. [Investigation decision][investigation]

## Report checks

The research completion comment records checks and outcomes. Source inspection establishes no runtime completeness.

[modules]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/MODULE.bazel
[bazel-version]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/.bazelversion
[bazelrc]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/.bazelrc#L89
[coverage-script]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/coverage/coverage.sh#L55
[java-tools]: https://github.com/bazelbuild/rules_java/blob/cd2030164287fdcbfc104036f7aebb788ef48c08/java/repositories.bzl#L24
[java-release]: https://github.com/bazelbuild/java_tools/releases/tag/java_v22.0
[java-manifest]: https://github.com/bazelbuild/rules_java/blob/cd2030164287fdcbfc104036f7aebb788ef48c08/java/common/rules/impl/java_binary_impl.bzl#L322
[java-helper]: https://github.com/bazelbuild/rules_java/blob/cd2030164287fdcbfc104036f7aebb788ef48c08/java/common/rules/impl/java_helper.bzl#L225
[java-instrument]: https://github.com/bazelbuild/bazel/blob/c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95/src/java_tools/buildjar/java/com/google/devtools/build/buildjar/instrumentation/JacocoInstrumentationProcessor.java
[runner]: https://github.com/bazelbuild/bazel/blob/c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95/src/java_tools/junitrunner/java/com/google/testing/coverage/JacocoCoverageRunner.java
[analyzer]: https://github.com/bazelbuild/bazel/blob/c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95/src/java_tools/junitrunner/java/com/google/testing/coverage/CoverageAnalyzer.java
[formatter]: https://github.com/bazelbuild/bazel/blob/c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95/src/java_tools/junitrunner/java/com/google/testing/coverage/JacocoLCOVFormatter.java
[class-mapper]: https://github.com/bazelbuild/bazel/blob/c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95/src/java_tools/junitrunner/java/com/google/testing/coverage/ClassProbesMapper.java
[method-mapper]: https://github.com/bazelbuild/bazel/blob/c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95/src/java_tools/junitrunner/java/com/google/testing/coverage/MethodProbesMapper.java
[bazel-baseline]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/analysis/test/BaselineCoverageAction.java
[kt-compile]: https://github.com/bazel-contrib/rules_kotlin/blob/09931b5ce94251d94b803c0d0a6c535896e67d16/kotlin/internal/jvm/compile.bzl#L970
[kt-instrument]: https://github.com/bazel-contrib/rules_kotlin/blob/09931b5ce94251d94b803c0d0a6c535896e67d16/src/main/kotlin/io/bazel/kotlin/builder/tasks/jvm/JacocoInstrumentation.kt
[kt-launcher]: https://github.com/bazel-contrib/rules_kotlin/blob/09931b5ce94251d94b803c0d0a6c535896e67d16/kotlin/internal/jvm/common.bzl#L197
[kt-helper]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/ktor_tutorial/test/model/FakeTaskRepository.kt
[clj-rule]: https://github.com/griffinbank/rules_clojure/blob/c0322b0251489b5d9f883e55ec093e90cb0dbdd9/rules.bzl
[clj-jar]: https://github.com/griffinbank/rules_clojure/blob/c0322b0251489b5d9f883e55ec093e90cb0dbdd9/rules/jar.bzl
[clj-compile]: https://github.com/griffinbank/rules_clojure/blob/c0322b0251489b5d9f883e55ec093e90cb0dbdd9/src/rules_clojure/compile.clj
[clj-runner]: https://github.com/griffinbank/rules_clojure/blob/c0322b0251489b5d9f883e55ec093e90cb0dbdd9/src/rules_clojure/testrunner.clj
[clj-prod]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/clojure_sample/src/lowkeylab/clojure_sample/BUILD.bazel
[clj-test]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/clojure_sample/test/lowkeylab/clojure_sample/BUILD.bazel
[clover-coverage]: https://github.com/cloverage/cloverage/blob/e427762f75d3e1fe461554c08ad247a7365580d2/cloverage/src/cloverage/coverage.clj
[clover-inst]: https://github.com/cloverage/cloverage/blob/e427762f75d3e1fe461554c08ad247a7365580d2/cloverage/src/cloverage/instrument.clj
[clover-lcov]: https://github.com/cloverage/cloverage/blob/e427762f75d3e1fe461554c08ad247a7365580d2/cloverage/src/cloverage/report/lcov.clj
[jacoco-counters]: https://www.jacoco.org/jacoco/trunk/doc/counters.html
[jacoco-faq]: https://www.jacoco.org/jacoco/trunk/doc/faq.html
[membership]: https://github.com/LowkeyLab/bazel-repo/issues/2417#issuecomment-6277250438
[investigation]: https://github.com/LowkeyLab/bazel-repo/issues/2438#issuecomment-6277423566
[previous-mechanisms]: https://github.com/LowkeyLab/bazel-repo/blob/c277c511697f5d93b0fb7bcd1f3dcae3d573c848/docs/research/2026-10-18-coverage-inclusion-mechanisms.md
[previous-evidence]: https://github.com/LowkeyLab/bazel-repo/blob/2b0cfa271ebfbe1dcc1701574883f61e62afb702/docs/research/2026-10-18-coverage-completeness-evidence.md
[merger-source]: https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator/SourceFileCoverage.java
[merger-line]: https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator/LineCoverage.java
