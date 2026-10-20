# Automatic JVM ownership and inline attribution

This experiment answers [Test automatic JVM ownership and Kotlin inline coverage attribution](https://github.com/LowkeyLab/bazel-repo/issues/2480).
The research base is `39075c1280f357d58b06a9a471e617cd73949bdf`.
It reuses the fixture from `31f471eb582c17e79cd83ce7ffe70c4137354b1c`.
The new local run supplied the published compiler jars and execution records.

## Result

Bazel supplies source owners and original class jars for the fixture.
A basename lookup resolves nine class owners and rejects two ambiguous owners.
Both ambiguous classes come from different `Twin.kt` files in one Kotlin target.
The current providers supply the target's source list, but no per-class source map.

The standard JaCoCo analysis API correctly attributes the executed inline body to `ExecutedKotlin.kt:3`.
The native Bazel analyzer reports zero hits for that body.
JaCoCo core preserves SMAP fragments before Kotlin filters remove generated locations.
Its `CoverageBuilder` applies those fragments to original classes.
The pinned Bazel custom analyzer does not use that aggregation path.

This result supports further use of standard JaCoCo line analysis with preserved class identities.
It does not establish a complete adapter.
The experiment preserves unresolved source identities instead of guessing paths.
No collector exception or production change follows from this result.

## Automatic ownership

The [Bazel query](evidence/source-owners.xml) supplies direct `srcs` for every fixture target.
The [JavaInfo query](evidence/java-info.txt) supplies class jars and source jars.
The [provider dump](evidence/providers.txt) also records Kotlin `KtJvmInfo.srcs`.
The query includes disconnected libraries, the Java test, and the executable helper.
It does not start from the test dependency graph.

The [metadata inspector](analyze.py) reads each original class's `SourceFile` attribute.
It matches that basename against the owning target's Bazel source list.
It uses no fixed class-owner table.
The [ownership result](evidence/automatic-ownership.json) records every class, compiler hash, SMAP, candidate path, and lookup status.

Nine classes each have one candidate path.
The directory/package mismatches resolve because ownership does not require a package-path suffix.
`shared/SecondTwin` and `shared/TwinKt` each have two candidate paths.
Both classes record `SourceFile=Twin.kt`.
Neither class contains `SourceDebugExtension` metadata.
The experiment refuses to select one candidate.

This lookup tests a specific candidate.
It does not establish that all available compiler mechanisms lack ownership information.
A source parser or a new compiler mapping could resolve these identities, but this experiment proves neither mechanism.

## Inline lines and supported branches

The temporary [Original launcher](../../tools/coverage/jvm_fixture/java/adapter/Original.java) uses the standard JaCoCo API through reflection.
Its runtime dependency supplies the same pinned JaCoCo implementation as the native collector.
It analyzes `.class.uninstrumented` entries without loading application classes.
It obtains class records after `CoverageBuilder.getClasses()` applies inline fragments.
It reads each class before any source-filename grouping.

The [raw baseline](evidence/core-empty.tsv), [parent analysis](evidence/core-parent.tsv), and [child analysis](evidence/core-child.tsv) retain class identities.
Each `LINE` record contains class name, original line, covered instructions, total instructions, covered branches, and total branches.
The baseline includes eleven classes and 43 executable lines.
The [independent expected records](evidence/expected-lines.json) specify the expected executable-line set for each fixture class.
All three analyses match that set exactly.
The declaration-only class has no executable lines.
The disconnected initializers do not execute during metadata analysis.

The parent analysis contains this inline record:

```text
LINE kprobe/ExecutedKotlinKt 3 1 1 0 0
```

The native [parent report](evidence/native-parent.lcov) retains `DA:3,0` for the same inline body.
This difference identifies an adapter choice within the existing pins.
It does not require a Kotlin or JaCoCo upgrade.

The controlled Java and Kotlin conditions each retain two branches, with one covered branch.
Core JaCoCo returns branch counts, not stable per-branch identifiers.
These counts cannot establish a complete union when separate processes take different branches.
The experiment therefore emits no invented `BRDA` identities.
A complete adapter still needs stable branch expressions or another proved branch-identity mechanism.
Inline fragments in this JaCoCo version add instruction records with zero branch counters.
Inline branches need a separate experiment before a supported-branch guarantee.

## Child collection, baselines, and failures

The fresh local functional test passed.
Its parent assertion evaluated the inline result.
Its child executed the controlled child-only line and returned successfully.
The [test result](evidence/local-test-result.json) records fresh `linux-sandbox` execution.
The command disabled test-result reuse.
Compilation used local actions and existing local action-cache entries.
No configured-remote test execution or hosted upload occurred in this experiment.

The adapter consumes separate parent and child execution files.
The child analysis gives a positive hit at `Child.java:7`.
The parent analysis gives zero there.
The empty-data baseline gives zero for every executable line.
The original class jars remain present for disconnected libraries, tests, and helpers.

The line merge uses logical OR for positive line membership.
The [forward partial report](evidence/merged-forward-partial.lcov) and [reverse partial report](evidence/merged-reverse-partial.lcov) have identical bytes.
Both preserve the inline hit, child hit, and disconnected zero records.
Each report contains nine canonical original paths.
The two ambiguous classes remain in the raw class records and do not receive invented original paths.
These reports are partial diagnostic reports, not complete combined-coverage evidence.
The experiment does not merge aggregate branch counts as if they identified distinct branches.

After functional success, separate controls remove the expected child input, corrupt that input, and remove expected class metadata.
Each analysis exits with an error.
The evidence retains each control's stdout, stderr, and exit status.
These results prove adapter input checks for explicitly expected inputs.
They do not establish automatic process discovery or native collector failure detection.

## Remaining feasibility question

Can the pinned Kotlin compiler action supply a per-class original-source map for duplicate basenames within one multi-source target?
The map must preserve normal joint compilation and avoid a fixed fixture owner table.
A subsequent check must combine that map with class-preserving JaCoCo line analysis.
It must also preserve stable branch identities, including inline branches, before recommending a complete adapter.

The maintainer's collector decision remains open.
The accepted Python architecture, combined LCOV workflow, percentage thresholds, and Codecov policy remain unchanged.

## Sources and reproducibility

The repository still pins rules_java `cd2030164287fdcbfc104036f7aebb788ef48c08` and rules_kotlin `09931b5ce94251d94b803c0d0a6c535896e67d16`.
The Java tools revision remains `c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95`, with JaCoCo `0.8.14`.
The preserved [input hashes](evidence/input-sha256.json) identify the exact local compiler and execution records.
Execution session timestamps use zero replacement values.
Class identifiers and probe arrays remain unchanged.

Primary sources explain the result:

- [Bazel CoverageAnalyzer](https://github.com/bazelbuild/bazel/blob/c9b10ec53b5fa49a1bb9c593f08f73ff52f60d95/src/java_tools/junitrunner/java/com/google/testing/coverage/CoverageAnalyzer.java) uses custom probe expressions and groups records by package/source filename.
- [JaCoCo ClassAnalyzer](https://github.com/jacoco/jacoco/blob/v0.8.14/org.jacoco.core/src/org/jacoco/core/internal/analysis/ClassAnalyzer.java) captures original-line fragments before filtering.
- [JaCoCo CoverageBuilder](https://github.com/jacoco/jacoco/blob/v0.8.14/org.jacoco.core/src/org/jacoco/core/analysis/CoverageBuilder.java) applies those fragments before source aggregation.
- [JaCoCo KotlinInlineFilter](https://github.com/jacoco/jacoco/blob/v0.8.14/org.jacoco.core/src/org/jacoco/core/internal/analysis/filter/KotlinInlineFilter.java) filters generated inline locations.
- [Pinned Kotlin provider](https://github.com/bazel-contrib/rules_kotlin/blob/09931b5ce94251d94b803c0d0a6c535896e67d16/kotlin/internal/jvm/compile.bzl) supplies target sources and output jars.

[Reproduction commands](REPRODUCE.md) record the local configuration and analysis sequence.
The report distinguishes fresh local execution from source inspection and earlier research.
Automatic approval review rejected the requested configured-remote run twice.
It required explicit user authorization for the source and artifact payload sent to BuildBuddy.
The remote check remains unexecuted.

## Validation

Gazelle passed after the source edits.
Whole-tree formatting passed with hermetic Java 21 compatibility flags.
Strict Ruff and ty checks passed for the Python inspector.
Default Java lint failed because the base lacks exported `pmd.xml` and `checkstyle.xml` files.
The [validation record](evidence/validation.json) separates these outcomes.
The final formatted executables reproduced the ownership and parent analysis results.
This bounded experiment did not run whole-repository build or test acceptance.
Published command logs remove trailing whitespace. Private runtime receipts retain the original log bytes.
The research branch has no pull request and requires no merge.
