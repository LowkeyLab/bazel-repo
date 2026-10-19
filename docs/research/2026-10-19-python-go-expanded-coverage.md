# Python and Go coverage with expanded source membership

## Result and evidence scope

Keep the Python collector and its production baselines. Extend its source metadata, baseline ownership, and checks to include maintained tests and executable helpers. Current Go rules supply production baselines and child-executable collection. Go test-source baselines and collection failure detection need additional work. The current Go LCOV converter supplies line records, without branch records. [Python sources][python-defs], [Python patch][python-patch], [Go baseline rule][go-baseline-rule], [Go converter][go-lcov], [Go exit hook][go-exit]

This report resolves the investigation in [Validate Python and Go collectors against expanded source membership][ticket]. It proposes mechanisms and experiments. It approves no exception and changes no collector.

The research checkout came from a successful pool acquisition at refreshed main `151e15cd495b5d63155753c0acf09f50102f0cbc`. Earlier source inspection used `6cb69e579da75a8d18d1115e1d56670a4c957219`. GitHub API comparison checked the two intervening commits. Only Rust locks and Aspect/Nix provisioning changed. Collector sources and selected versions stayed unchanged. [Revision comparison][revision-compare]

Repository source links use the acquired revision. Exact upstream files came from GitHub's contents API at their pinned commits. This session ran no coverage collection, configured-remote coverage acceptance, or hosted upload.

The approved [membership decision][membership] includes production code, tests, executable helpers, hidden packages, and disconnected sources. It excludes generated outputs, vendored code, and external dependencies. The [investigation decision][investigation] also requires first-party code that tests execute in separate processes. Neither decision approves a collector exception.

## Selected versions

| Component          | Inspected selection                                   | Source                                  |
| ------------------ | ----------------------------------------------------- | --------------------------------------- |
| Bazel              | `9.3.0`                                               | [Version file][bazel-version]           |
| Python rules       | `aspect_rules_py` `1.12.1`, with the repository patch | [Module][module], [Patch][python-patch] |
| Python collector   | `coverage.py` `7.16.2`                                | [Lockfile][uv-lock]                     |
| Python interpreter | `3.15`, standalone release `20261013`                 | [Module][module]                        |
| Go rules           | `20008e8ca02add2d4ae2f3edf514d94afa3ee0be`            | [Module][module]                        |
| Go SDK             | `1.26.4`, selected through `go_sdk.from_file`         | [Module][module], [Go module][go-mod]   |

The earlier reports inspected Go commit `7d3f58b093ddab21ecb4b429d8eefafaf64999f1`. This report checks the changed pin directly. It retains their distinction between instrumentation, root selection, executable baselines, collection, merging, and upload. [Mechanism report][prior-mechanisms], [Evidence report][prior-evidence]

## Capability matrix

“Source capability” means that inspected code supplies the mechanism. It does not mean that this session executed the mechanism.

| Case                                      | Python                                                                                             | Go                                                                                             | Remaining evidence or adapter                                                          |
| ----------------------------------------- | -------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Executed original production sources      | Existing collector supplies line and branch records. Prior delivery records execution evidence.    | Instrumentation and LCOV conversion supply line records.                                       | Check new membership independently from the existing production result.                |
| Disconnected production sources           | Existing metadata wrappers supply real zero-hit lines and branches when selected.                  | Current `go_library` and `go_binary` supply real zero-hit lines when selected.                 | Automatic ordinary and hidden root discovery must select the required owners.          |
| Executed test sources                     | Current patch excludes test-only local sources. Collector starts before `pytest.main`.             | Test rules can instrument declared sources when `--instrument_test_targets` permits it.        | Extend Python metadata. Execute Go internal and external test-package cases.           |
| Disconnected test sources                 | `py_test` declares no custom baseline. Test-only libraries and binaries fail the production gate.  | `go_test` declares no custom executable-line baseline.                                         | Extend Python baselines. Investigate a Go baseline adapter for unexecuted test owners. |
| Executable test helpers                   | Current test-only gate excludes their own Python sources.                                          | Test-only `go_library` and `go_binary` use existing baseline mechanisms.                       | Select helper owners independently of whether a test executes them.                    |
| Branch records                            | Existing runtime and baseline generator enable branch collection.                                  | Current converter and baseline generator emit no `BRDA`, `BRF`, or `BRH`.                      | Preserve Python branches. Do not describe Go statement blocks as branch coverage.      |
| Separate executable                       | Existing private startup hook measures generated binaries and current-interpreter children.        | Coverage-built `go_binary` receives an exit hook and writes separate LCOV files.               | Check helper-only lines, inherited environment, child completion, and merged paths.    |
| Canonical paths and repeated observations | Existing manifest mapping restores source paths after symlink resolution.                          | Compile-time mappings restore exec-root-relative paths. Baselines use matching source strings. | Require one repository-relative identity and preserved positive counts after merging.  |
| Missing or corrupt data                   | Started-child receipts detect absent completion or data. Direct reads reject corrupt process data. | Several collection failures only print messages. Missing test profiles can return success.     | Add independent completeness checks and a process completion mechanism where needed.   |

The sections below identify the sources and limits for each matrix row.

## Python findings

### Expanded metadata and baselines

Two independent gates currently remove test-only local sources. The repository metadata rule computes `production` from `testonly`, repository identity, and the `3rdparty/` boundary. The upstream patch applies the same gate when declaring instrumented source attributes. Changing only the instrumentation filter or `--instrument_test_targets` cannot remove those gates. [Wrappers][python-defs], [Patch][python-patch], [Bazel inclusion check][bazel-instrumented]

The `py_library` and `py_binary` wrappers attach metadata through `data`. Their metadata rule selects original `.py` inputs and creates a `PythonCoverageBaseline` action. The generator explicitly touches supplied files with branch mode enabled. It obtains executable locations from the real collector. The existing checks cover zero-hit lines, branches, functions, symlinks, missing sources, and invalid syntax. [Wrappers][python-defs], [Generator][python-baseline], [Baseline checks][python-baseline-tests]

The shared `py_test` wrapper supplies pytest, coverage, and the runtime helper. It does not attach `_coverage_data`. Upstream `py_test` forces its auxiliary targets to `testonly=True`. The binary implementation obtains instrumentation metadata from the patched library helper. Therefore, both direct test sources and generated test support need deliberate membership treatment. Keep generated runner outputs excluded. [Wrappers][python-defs], [Upstream macros][py-macros], [Binary implementation][py-binary], [Patch][python-patch]

The smallest extension reuses these mechanisms. Permit maintained test sources in both metadata paths. Attach an executable baseline to each test owner, including tests excluded from execution. Keep source ownership and generated-source checks. Keep the existing production baseline path. The current membership check must replace assertions that reject every `_test.py` and test helper. [Membership check][python-membership]

### Startup and process boundary

The patched pytest runner starts collection before `pytest.main`, so collection can observe test-module import and test execution after metadata expansion. It imports pytest before collector startup. The generated runner and collector initialization also precede measurement. This sequence does not establish coverage of arbitrary code executed during interpreter startup. The generated runner is an excluded output. Any maintained startup hook needs its own classification and evidence. [Runner][py-runner], [Patch][python-patch]

The runtime helper creates a private configuration and `.pth` startup hook. Current-interpreter children reuse the test environment. The patched generated launcher installs the hook in its child environment. Children retain the absolute include paths and data destination. This mechanism requires the relevant environment and Python startup processing. It does not establish unrelated interpreters, `-S`, environment removal, containers, or abrupt termination as supported boundaries. [Runtime][python-runtime], [Launcher patch][python-patch]

A child creates a `.started` receipt before collection. On normal interpreter exit, it saves data and a `.saved` receipt. Parent combination rejects missing receipts, absent data files, and corrupt data. A child that never starts the hook creates no receipt. Thus, existing receipt checks cannot independently detect every bypassed hook. A baseline alone can conceal that missing execution. [Runtime][python-runtime]

The runtime checks deliberately supply test-file paths in manually constructed manifests. This establishes available test machinery, not Bazel's expanded source membership. The checks cover generated binaries, current-interpreter scripts, missing completion, absent data, corrupt data, and changed child directories. [Runtime checks][python-runtime-tests]

### Preserved execution evidence

The recorded resolution for [Capture and combine Python subprocess coverage][python-subprocess-delivery] reports 15 real-process cases and seven remotely executed Python actions. The recorded [production membership resolution][python-membership-delivery] reports ordinary and hidden temporary packages, disconnected zero-hit records, merged identities, and invalid-source failure. Its final hosted test and coverage actions came from cache.

These resolutions record prior delivery evidence. This session read the resolutions and current checks. It did not obtain or check their raw reports. They establish no expanded test-source acceptance at this revision. The remote and hosted acceptance tickets remain separate work. [Remote acceptance][python-remote], [Hosted acceptance][python-hosted]

## Go findings

### Baselines and selected roots

The current pin retains `baseline_coverage_kwargs` for `go_library` and `go_binary`. Coverage mode, target instrumentation, the baseline API, and the toolchain's `covdata` are prerequisites. The helper does not exclude `testonly` targets. It passes declared `.go` sources to a separate baseline action. [Library rule][go-library], [Binary rule][go-binary], [Baseline rule][go-baseline-rule]

The generator uses `go tool cover` metadata and `covdata textfmt`. It expands each coverage block into executable line records with zero counts. It applies the compilation configuration's build constraints. Cgo Go sources receive original-source locations before rewriting. An empty emitted metadata file means no function bodies. An absent metadata file, invalid profile, or failed metadata conversion causes an error. [Baseline generator][go-baseline]

These facts support a candidate mechanism for disconnected libraries and helper binaries. They do not discover roots. Current coverage uses `--build_tag_filters=python-coverage`, so wildcard non-test selection does not select ordinary Go roots. Automatic root discovery must replace that selection restriction for required coverage roots. Preserve existing test execution filters and ignored-path boundaries. [Coverage configuration][bazelrc], [Membership decision][membership]

The upstream baseline check includes disconnected libraries, a binary, declarations, cgo, constraints, and retained measured hits. This report treats the test as source evidence. This report does not claim that those upstream checks ran here. [Upstream baseline check][go-baseline-test]

### Test sources and helpers

`go_test` declares its own source attributes and propagates `data`, `deps`, `embed`, and `embedsrcs`. Bazel includes test-local sources only when `--instrument_test_targets` and the filter permit them. The repository already enables that switch. [Test rule][go-test], [Bazel inclusion check][bazel-instrumented], [Coverage configuration][bazelrc]

The rule creates an internal package and an external package. `new_go_info` supplies declared sources for instrumentation. Embedded library coverage propagates separately.

The builder's test filter divides files by package suffix `_test`, rather than excluding every filename ending `_test.go`. Internal tests remain in the internal package. External tests enter the external package. The builder then instruments selected sources after this partition. [Test rule][go-test], [Context][go-context], [Archive action][go-archive], [Builder filter][go-filter], [Compilation][go-compile]

This is source capability for internal tests, external tests, and their unexecuted functions within an executed package. Execute controlled cases before claiming repository acceptance. The generated test main explicitly disables its own instrumentation. That exclusion concerns generated output. [Test rule][go-test]

Unlike library and binary rules, `go_test` supplies no custom baseline. Bazel's fallback baseline names sources without executable lines. [Fallback action][bazel-baseline]

Thus, merely analyzing an unexecuted test owner does not establish the required denominator. A candidate adapter must derive executable lines for test-local sources. It must handle internal and external package groups separately. Reusing the existing baseline generator with mixed package names needs an experiment. [Test rule][go-test], [Bazel collector][bazel-instrumented], [Baseline generator][go-baseline]

### Supported dimensions

The selected Go compilation mode uses atomic block counters. The converter expands block spans into line counts. It uses the maximum count for blocks that overlap one line. Its output contains `SF`, `DA`, `LH`, `LF`, and record terminators. The baseline emits the same line dimensions. Neither implementation emits branch records or measured function records. [Compile action][go-compile-action], [Converter][go-lcov], [Baseline generator][go-baseline]

A converter comment claims branch coverage, but its implementation emits no branch fields. Treat the implementation as the capability evidence. Under the approved supported-branch requirement, this collector supplies no Go branch dimension to preserve. A separate requirement for Go branches would need another mechanism. This finding approves no source exception. [Converter][go-lcov], [Membership decision][membership]

### Separate executables and aggregation

In coverage mode, `go_binary` adds a generated shim that calls `bincov.AddExitHook`. The hook uses inherited `COVERAGE_DIR` and `TEST_TMPDIR`. On termination, it writes metadata and counters into a private scratch directory. It converts that process data into a unique `go_coverage.*.dat` file in the coverage directory. Separate invocations avoid a fixed output-name collision. [Binary rule][go-binary], [Shim][go-shim], [Exit hook][go-exit], [Converter][go-lcov]

Bazel's collection script passes that directory to its LCOV merger. The merger recursively discovers `.dat` files. Thus, the pinned source supplies a complete aggregation path for these child reports. Canonical source paths still matter. Distinct source strings remain distinct records. Bazel sums line observations for matching source identities, so merging zeros preserves measured hits. [Collection script][bazel-collection], [Merger][bazel-main], [Coverage map][bazel-coverage], [Line merge][bazel-lines]

Go's exit hook supports return from `main` and `os.Exit`. `RunOnFailure=True` also requests collection for nonzero exits through that hook. An unhandled panic follows a different runtime exit path. This inspection does not establish panic, runtime fatal errors, signals, or killed children as complete collection cases. A recovered panic followed by normal termination is a different case. [Go exit-hook API][go-runtime-hooks], [Go panic path][go-runtime-panic], [Collector hook][go-exit]

The repository's generation tests set `GOCOVERDIR` before starting Go subprocesses. This avoids Go runtime startup diagnostics before the rules hook runs. It is configuration source, without a current process-report artifact. Upstream includes a child-binary LCOV regression case. This report inspected that case without executing it. [Repository helper][repo-go-helper], [Upstream process check][go-coverage-test]

### Missing collection data

The child exit hook prints messages and returns on several scratch, metadata, counter, reader, and conversion failures. These paths do not directly fail an otherwise successful application. Missing `COVERAGE_DIR` also skips collection. The test converter returns success when its input profile is absent. [Exit hook][go-exit], [Converter][go-lcov]

The generated test main calls `ProcessCoverTestDir` without returning its error in the LCOV wrapper. The converter also does not check its scanner error or deferred flush error. These are concrete source-level failure limits. A baseline can still produce valid zero records when executed data disappears. [Generated main][go-test-main], [Converter][go-lcov]

Bazel rejects parsing failures that raise `IOException`. Its LCOV parser can ignore malformed records without that exception. A corrupt file does not always cause failure. [Parser][bazel-parser] However, its merger can create an empty report and return success when it finds no coverage. The repository wrapper checks report existence, without checking membership or expected execution. Therefore, an independent inventory and evidence for executed processes remain necessary. [Merger][bazel-main], [Repository wrapper][repo-coverage]

A suitable adapter must check both source completeness and process completion. Source membership alone cannot distinguish an unexecuted source from a lost child report. Candidate mechanisms include completion receipts around maintained executable starts, expected execution markers in controlled acceptance cases, and collector error propagation. The later mechanism decision must choose their scope. This report does not claim that every process requires a manually maintained filename list.

## Smallest remaining experiments

Each experiment must preserve its revision, command, configuration, raw reports, combined LCOV, and action provenance. These experiments establish local feasibility first. They do not replace the separate Python remote and hosted acceptance work.

| Experiment                 | Controlled setup                                                                                                                          | Required observations                                                                                                                                                                                       |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Python expanded membership | One ordinary package and one hidden package. Include an executed test, an unexecuted test, a test-only library, and a test-only binary.   | Each original source contributes known executable lines. Unexecuted sources retain zeros. Executed test import and body lines have hits. Branch alternatives remain visible. Generated outputs stay absent. |
| Python helper processes    | Start a test-only binary and a current-interpreter helper. Put a known line only in each child.                                           | Both child-only lines have hits under one canonical path. Missing completion, missing data, and corrupt data fail despite passing functional checks. A bypassed startup hook causes visible failure.        |
| Go test packages           | One internal `_test.go`, one external-package `_test.go`, one embedded library, and a test-only helper library. Include unused functions. | The executed report contains all maintained test sources and real zero lines for unused code. Generated test main stays absent. Compare switch-enabled and switch-disabled manifests.                       |
| Go test baseline           | One selected but unexecuted test owner with internal and external packages. Include one disconnected test-only library and binary.        | Each original source receives known zero-hit `DA` records. Separate package grouping works. Invalid input and absent metadata fail. A declaration-only source has independently established `LF:0`.         |
| Go child executable        | One test starts the same coverage-built helper twice and waits for both children. Use parent-only and child-only functions.               | Separate raw `.dat` files exist. Combined LCOV has one source identity. Child-only hits survive baseline merging. Exercise normal return and `os.Exit(0)` separately.                                       |
| Go process failures        | Make scratch output unavailable, remove one report, corrupt another report, bypass the environment, and terminate a child abnormally.     | The functional test can pass, but coverage validation fails for missing required evidence. Record panic and signal results separately. Do not treat absent data as a supported zero-hit result.             |
| Selection and exclusions   | Add ordinary and hidden disconnected owners without a project list change. Include generated and vendored neighbors.                      | Discovery selects eligible owners. Original sources receive executable records. Exclusions remove only intended sources. The ownership check exposes undeclared maintained code.                            |

The remaining feasibility question is [Determine whether Go baselines can include unexecuted test packages][go-test-baseline-question]. It must establish whether the current builder can supply test baselines before the later mechanism decision. The other experiments define later acceptance, unless another source gap requires a separate feasibility question.

The Go experiments must use the current rules pin and SDK. A passing historical test does not settle changed toolchain behavior. Avoid requiring branch fields from the current Go converter.

## Recommended disposition

1. Extend Python metadata and baselines for maintained test code and executable helpers.
2. Preserve Python production collection, subprocess collection, hermetic dependencies, and the shared pytest wrapper.
3. Reuse current Go library and binary baselines, with automatic root selection.
4. Use the [Go baseline investigation][go-test-baseline-question] before selecting a test baseline adapter.
5. Reuse Go child collection where its environment and exit conditions apply.
6. Add collection failure detection for missing Go process evidence.
7. Keep unsupported Go branch output distinct from unproven source and process cases.
8. Retain every unproven case as an investigation or adapter requirement until the maintainer chooses its disposition.

This report proposes no collector exception. Preserve combined LCOV, existing thresholds, Codecov policy, and the remaining Python acceptance scope.

## Validation and publication status

This report belongs to `research/python-go-expanded-coverage`. The branch contains one research Markdown file. No collector source changed. The research branch requires no PR or merge.

The strict prose check preserved the exact issue title and omitted Markdown reference definitions. Its final result contained zero findings. A reference check found no missing or unused definitions. GitHub tree queries checked all 44 cited source paths at their exact revisions.

Full repository formatting passed. The [repository build][report-build] also passed. The required `aspect lint` command exited zero with 64 advisory warnings. Its changed-file selection checked the Markdown target. The warnings concern technical terms and prose preferences. This result does not establish all-source lint execution.

These checks establish documentation integration and build success. They do not establish collector behavior. The resolution records the complete check results.

This documentation investigation ran no coverage collection, configured-remote coverage acceptance, or hosted upload.

[ticket]: https://github.com/LowkeyLab/bazel-repo/issues/2439
[membership]: https://github.com/LowkeyLab/bazel-repo/issues/2417#issuecomment-6277250438
[investigation]: https://github.com/LowkeyLab/bazel-repo/issues/2438#issuecomment-6277423566
[bazel-version]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/.bazelversion
[module]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/MODULE.bazel
[uv-lock]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/uv.lock
[go-mod]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/go.mod
[bazelrc]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/.bazelrc
[python-defs]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/python/defs.bzl#L12
[python-patch]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/python/coverage.patch#L38
[python-baseline]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/python/coverage_baseline.py
[python-baseline-tests]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/python/coverage_baseline_test.py
[python-membership]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/python/coverage_membership_test.py
[python-runtime]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/python/coverage_runtime.py
[python-runtime-tests]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/python/coverage_runtime_test.py
[python-subprocess-delivery]: https://github.com/LowkeyLab/bazel-repo/issues/2393#issuecomment-6252045748
[python-membership-delivery]: https://github.com/LowkeyLab/bazel-repo/issues/2394#issuecomment-6253385657
[python-remote]: https://github.com/LowkeyLab/bazel-repo/issues/2395
[python-hosted]: https://github.com/LowkeyLab/bazel-repo/issues/2396
[py-macros]: https://github.com/aspect-build/rules_py/blob/v1.12.1/py/defs.bzl
[py-binary]: https://github.com/aspect-build/rules_py/blob/v1.12.1/py/private/py_binary.bzl
[py-runner]: https://github.com/aspect-build/rules_py/blob/v1.12.1/py/private/pytest_main.py
[go-baseline-rule]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/private/actions/baseline_coverage.bzl#L19
[go-baseline]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/tools/builders/baseline_coverage.go
[go-baseline-test]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/tests/core/coverage/baseline_coverage_test.go
[go-library]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/private/rules/library.bzl
[go-binary]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/private/rules/binary.bzl
[go-test]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/private/rules/test.bzl#L78
[go-context]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/private/context.bzl
[go-archive]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/private/actions/archive.bzl
[go-filter]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/tools/builders/filter.go
[go-compile]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/tools/builders/compilepkg.go
[go-compile-action]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/private/actions/compilepkg.bzl
[go-lcov]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/tools/bzltestutil/lcov.go#L132
[go-exit]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/tools/bzltestutil/bincov/exit_hook.go#L32
[go-shim]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/private/coverage_shim.go
[go-test-main]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/tools/builders/generate_test_main.go
[go-coverage-test]: https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/tests/core/coverage/coverage_test.go
[go-runtime-hooks]: https://github.com/golang/go/blob/go1.26.4/src/internal/runtime/exithook/hooks.go
[go-runtime-panic]: https://github.com/golang/go/blob/go1.26.4/src/runtime/panic.go
[repo-go-helper]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/3rdparty/gazelle_rust/upstream/generation_tests/coverage_test.go
[bazel-instrumented]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/analysis/test/InstrumentedFilesCollector.java
[bazel-collection]: https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/collect_coverage.sh
[bazel-main]: https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator/Main.java
[bazel-coverage]: https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator/Coverage.java
[bazel-lines]: https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator/LineCoverage.java
[repo-coverage]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/coverage/coverage.sh
[prior-mechanisms]: https://github.com/LowkeyLab/bazel-repo/blob/c277c511697f5d93b0fb7bcd1f3dcae3d573c848/docs/research/2026-10-18-coverage-inclusion-mechanisms.md
[prior-evidence]: https://github.com/LowkeyLab/bazel-repo/blob/2b0cfa271ebfbe1dcc1701574883f61e62afb702/docs/research/2026-10-18-coverage-completeness-evidence.md
[revision-compare]: https://github.com/LowkeyLab/bazel-repo/compare/6cb69e579da75a8d18d1115e1d56670a4c957219...151e15cd495b5d63155753c0acf09f50102f0cbc
[bazel-parser]: https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator/LcovParser.java
[bazel-baseline]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/analysis/test/BaselineCoverageAction.java#L69
[go-test-baseline-question]: https://github.com/LowkeyLab/bazel-repo/issues/2446
[report-build]: https://app.buildbuddy.io/invocation/51d88cbd-fe10-46ad-b24d-064e580d74d1
