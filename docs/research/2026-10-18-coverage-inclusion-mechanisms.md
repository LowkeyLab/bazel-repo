# Compare Bazel mechanisms for automatic coverage inclusion

Date: 2026-10-18.
Research question

[Compare Bazel mechanisms for automatic coverage inclusion](https://github.com/LowkeyLab/bazel-repo/issues/2415).
Repository base: `e9040f81672138d94a60e4c996a190f929394da6`.

## Findings

A workspace-wide instrumentation filter can remove the manual project list. It cannot select missing build roots or create executable-line records.
Use it with Bazel-derived root discovery and existing rule metadata. Add a language adapter only after evidence identifies a remaining gap.
This combination is a feasible option. The maintainer must still choose source policy and exclusions.

The following operations are separate:

| Operation          | Input                                                   | Result                                                | Limit                                           |
| ------------------ | ------------------------------------------------------- | ----------------------------------------------------- | ----------------------------------------------- |
| Select tests       | Target patterns and test filters                        | Tests that Bazel runs                                 | Does not define all production sources          |
| Select build roots | Target patterns and build filters                       | Non-test targets that Bazel analyzes and builds       | Can add baselines outside test dependencies     |
| Instrument sources | Coverage mode, rule support, and instrumentation filter | Compiler instrumentation or a runtime source manifest | Applies only to targets in the selected graph   |
| Add a baseline     | Rule metadata and a language tool                       | Coverage records for sources without execution        | Generic Bazel baselines contain no line records |
| Check membership   | Expected sources and combined LCOV                      | Evidence of omissions and exclusions                  | Does not itself measure execution               |

These distinctions follow from Bazel's [loading options][loading], [instrumented-file collector][collector], and [combined report builder][report].

## Current repository constraints

The repository pins Bazel `9.3.0`. Coverage uses `--combined_report=lcov`, `--nobuild_tests_only`, and `--build_tag_filters=python-coverage`.
It also enables `--instrument_test_targets` and a manual package filter.
The positive build tag selects additional Python roots. Test filters separately select tests.
[Repository configuration][rc] and [Bazel loading options][loading] establish these facts.

The Python migration already supplies source discovery, production baselines, child-process collection, and membership validation.
Shared wrappers add `python-coverage` to production libraries and executables. Their metadata rules use `baseline_coverage_files`.
The patch excludes `testonly` and external Python sources. The wrappers also exclude packages under `3rdparty/`.
Preserve this architecture and the accepted [Python ADR][python-adr].
See the [wrappers][python-defs], [patch][python-patch], and [membership validation][python-membership].

The [coverage workflow][workflow] calls the [existing wrapper][wrapper] and uploads `coverage-report.lcov`.
The remote configuration downloads top-level outputs and merges through a local `CoverageReport` strategy.
It enables pure Go mode and excludes `no-coverage` tests.
The imported remote configuration also excludes `no-remote` tests. Do not interpret an excluded test as an excluded production dependency.
[Repository configuration][rc] defines these settings.

## Option comparison

| Mechanism                      | Automatic inclusion                                   | Explicit exclusions                      | Zero-hit lines                                          | Main risk                                          |
| ------------------------------ | ----------------------------------------------------- | ---------------------------------------- | ------------------------------------------------------- | -------------------------------------------------- |
| Broad instrumentation filter   | New eligible labels in the selected graph             | Negative label regular expressions       | No independent guarantee                                | Missing roots remain missing                       |
| Bazel-derived discovery        | New eligible build targets and their source owners    | Query attributes and a documented policy | Only through selected rule baselines                    | Configuration differences and incomplete ownership |
| Existing providers and aspects | Sources that rules expose through declared attributes | Rule semantics and provider checks       | Existing adapters can provide lines                     | Provider presence does not prove line data         |
| Additional language adapter    | A demonstrated rule-specific gap                      | Adapter policy and source metadata       | Yes, if the language tool supplies executable locations | Extra maintenance and incorrect source mapping     |

### Workspace-wide instrumentation filter

An illustrative filter is `^//`, followed by explicit negative patterns. It matches main-repository labels without naming project directories.
For example, `^//,-^//tools[:/]` excludes the `tools` package tree. This example does not recommend excluding tools.
The existing Python policy includes production tooling.

Bazel applies regular expressions to rule labels. It does not apply them directly to source paths.
Exclusions take priority over inclusions. Pattern order does not change that result.
Use package boundaries such as `[:/]` to avoid matching a sibling prefix.
A filter with only negative patterns also permits external labels unless an exclusion matches them.
An explicit positive `^//` avoids that result.
[Bazel RegexFilter][regex] and [the collector][collector] define these semantics.

Removing the explicit filter invokes Bazel's package heuristic. That heuristic starts from selected test packages.
It rewrites Java test paths and removes certain package suffixes. It does not calculate all production source ownership.
It can miss a library outside those package prefixes, even when a test depends on that library.
[Bazel's filter implementation][inference] establishes this limit.

Keep `--instrument_test_targets` until language evidence supports its removal.
The flag permits test-rule instrumentation but still applies the filter.
The pinned Rust compiler also checks the underlying crate for `rust_test(crate = ...)`.
That special case does not establish equivalent behavior for independent integration-test binaries.
See [CoreOptions][core] and [the pinned Rust compiler][rustc].

### Bazel-derived discovery

Discover eligible rules from Bazel packages rather than a checked-in project directory list.
Possible inputs include `kind`, `attr`, `labels`, and existing production tags.
Use declared source ownership, `testonly`, repository identity, generated-file status, and documented exceptions to form the inventory.
This report proposes these policy inputs. The maintainer must define production code.

`query` inspects the unconfigured graph and includes possible `select()` branches.
`cquery` resolves the graph for a configuration. Use the coverage configuration when a configured inventory is necessary.
Do not silently compare a host inventory with a remote inventory.
The official [query reference][query] and [`cquery` reference][cquery] explain this difference.

A discovery process must distinguish expected sources from build roots.
Adding a source path to an inventory does not cause its owner to become a coverage root.
Bazel's report builder collects baseline artifacts from configured top-level targets, including non-test roots.
At least one compatible test must remain, or the builder creates no combined report.
[CoverageReportActionBuilder][report] establishes both conditions.

`--build_tests_only` discards non-test command-line roots. It therefore prevents standalone production roots from supplying baselines.
The current Python design deliberately uses `--nobuild_tests_only`.
Its positive build filter still excludes ordinary non-test Go and Rust roots from wildcard selection.
A future option can widen those roots through compatible tags or a Bazel-generated target list.
Selecting every binary, image, deployment target, and cross-platform target would add unnecessary work and possible failures.
The last sentence is a migration risk inferred from widening the selected graph.
See [LoadingOptions][loading] and [repository configuration][rc].

Preserve packages under hidden directories when they contain eligible targets.
The Python acceptance test explicitly selects `//.github/...` and checks new hidden production packages.
It also checks a source outside every test dependency closure.
A replacement must retain those cases. It must respect the repository's [`.bazelignore`][ignore].
An inventory of Bazel-owned sources cannot identify a source that has no target unless a separate source-ownership validation detects it.
See the [existing membership validation][python-membership] and the [query model][query].

Tags are selection metadata, not proof of coverage support.
`manual` affects wildcard build and test selection. Query and explicit target selection require separate care.
`no-coverage` is a repository convention applied through test filters.
Neither tag removes a source that another selected target reaches as a dependency.
See [common target attributes][attributes], [loading options][loading], and [repository configuration][rc].

### Existing providers and aspects

`coverage_common.instrumented_files_info` declares source attributes, dependency attributes, extensions, metadata, and optional baseline artifacts.
It does not supply a universal language parser or install compiler instrumentation.
The collector forwards dependency providers independently of whether the current rule's sources match the filter.
It excludes generated sources by default unless the generated-file coverage option permits them.
See [CoverageCommon][common] and [InstrumentedFilesCollector][collector].

An aspect can inspect rule metadata across chosen dependency attributes and produce an inventory or diagnostic output.
Its traversal starts from selected roots. It cannot discover disconnected packages by traversing test dependencies alone.
An aspect that returns coverage metadata still needs a demonstrated path into the combined report.
Do not assume an arbitrary aspect output group automatically becomes a baseline input.
The [aspect model][aspects] defines traversal. The [report builder][report] defines baseline collection.

Bazel's default `BaselineCoverage` action writes an `SF` record and `end_of_record` for each source.
It writes no `DA` records. An `SF` record alone cannot prove that executable lines without execution contribute to the denominator.
Language support must provide real line records where the required policy includes zero-hit lines.
See [BaselineCoverageAction][baseline].

The pinned rules already differ:

| Rules                   | Inspected pin                              | Relevant behavior                                                                                         |
| ----------------------- | ------------------------------------------ | --------------------------------------------------------------------------------------------------------- |
| `aspect_rules_py`       | `1.12.1`, with repository patch            | Shared wrappers supply production line and branch baselines                                               |
| `rules_go`              | `7d3f58b093ddab21ecb4b429d8eefafaf64999f1` | `GoBaselineCoverage` supplies zero-hit executable lines through `baseline_coverage_files`                 |
| `rules_rs`              | `f6067c1f0568e24487468a01b7ae7df0b87434cd` | Provisions `rules_rust` at `fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3`                                     |
| Underlying `rules_rust` | `fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3` | Compiler supplies source and dependency metadata without a custom baseline argument in the inspected path |
| `rules_kotlin`          | `09931b5ce94251d94b803c0d0a6c535896e67d16` | JVM compile path supplies coverage metadata without a custom baseline argument                            |
| `aspect_rules_js`       | `e67442a2ccf329841c9227d66ee013afcf5633c6` | Pin identified only. This report does not establish complete browser or JavaScript coverage support       |

Sources: [module pins][modules], [Python wrappers][python-defs], [Go library][go-library], [Go baseline][go-baseline],
[Rust provisioning][rust-provision], [Rust compiler][rustc], and [Kotlin compiler][kotlin].
The Rust and Kotlin observations concern the inspected provider paths. They do not prove that every runtime collector lacks other baseline behavior.

Go needs no new parser for the demonstrated baseline case.
Its baseline requires coverage mode, matching instrumentation, Bazel API support, and a toolchain with `_covdata`.
It respects Go build constraints and uses compiler-compatible paths.
Selection must still include the target outside test dependencies.
See [the pinned Go baseline action][go-baseline].

### Additional language adapters

Add an adapter only when a maintained case shows a gap after correct selection and instrumentation.
Prefer the language's compiler or existing coverage tool to a new source parser.
Connect its artifacts through `baseline_coverage_files` or a checked merge stage that preserves the current LCOV contract.
The existing [Python adapter][python-defs] and [Go adapter][go-baseline] demonstrate the first path.

For Rust and JVM sources, first inspect the pinned collectors for a compiled unit without execution.
For browser code, first establish how transformed files map to original sources and which runner emits LCOV.
Treat unsupported cases as explicit inventory results until evidence supports inclusion.
A filename extension or provider alone does not prove executable-line coverage.
This report proposes these acceptance requirements. They do not establish current support.

## Migration evidence

A future implementation needs the following evidence:

1. A new project enters the expected inventory without a configuration edit.
2. A new eligible source outside test dependencies produces executable zero-hit records.
3. A new hidden package receives the same treatment.
4. Existing executed sources retain their hit counts after baseline merging.
5. Explicit exclusions remove intended sources without removing sibling prefixes.
6. Test support, external sources, and generated sources follow the approved policy.
7. Failed discovery, failed collection, and missing data cause visible failures.
8. Local and configured-remote runs produce equivalent normalized source sets for their shared supported targets.
9. Remote evidence separates executed tests from cache results and records the exact commit.
10. The existing Python acceptance checks and combined LCOV upload continue to pass.

Do not change thresholds to hide denominator growth.
A broad filter can expose previously omitted test sources, tools, or dependencies.
A complete baseline can reduce a percentage without reducing exercised behavior.
Compare source membership, executable-line counts, and measured counts separately before choosing the final policy.
These are migration risks and proposed controls inferred from the preceding mechanisms.

## Research validation

This report uses current main and pinned upstream source. It makes no implementation change.
It does not claim a fresh coverage run, configured-remote execution, or complete language support.
The ticket comment records documentation checks and the report commit.

[rc]: https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelrc
[modules]: https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/MODULE.bazel
[ignore]: https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelignore
[python-adr]: https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/docs/adr/0002-use-rules-py-for-first-party-python.md
[python-defs]: https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/defs.bzl
[python-patch]: https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/coverage.patch
[python-membership]: https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/coverage_membership_test.py
[workflow]: https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.github/workflows/coverage.yml
[wrapper]: https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/coverage/coverage.sh
[core]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/analysis/config/CoreOptions.java
[loading]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/pkgcache/LoadingOptions.java
[regex]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/util/RegexFilter.java
[inference]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/buildtool/InstrumentationFilterSupport.java
[collector]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/analysis/test/InstrumentedFilesCollector.java
[common]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/analysis/test/CoverageCommon.java
[baseline]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/analysis/test/BaselineCoverageAction.java
[report]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/bazel/coverage/CoverageReportActionBuilder.java
[query]: https://bazel.build/query/language
[cquery]: https://bazel.build/query/cquery
[aspects]: https://bazel.build/extending/aspects
[attributes]: https://bazel.build/reference/be/common-definitions
[go-library]: https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/private/rules/library.bzl
[go-baseline]: https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/private/actions/baseline_coverage.bzl
[rust-provision]: https://github.com/hermeticbuild/rules_rs/blob/f6067c1f0568e24487468a01b7ae7df0b87434cd/rs/rules_rust.bzl
[rustc]: https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/private/rustc.bzl
[kotlin]: https://github.com/bazel-contrib/rules_kotlin/blob/09931b5ce94251d94b803c0d0a6c535896e67d16/kotlin/internal/jvm/compile.bzl
