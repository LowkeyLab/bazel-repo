# Coverage membership gaps

## Scope and evidence

This report investigates [Identify coverage membership gaps across repository projects](https://github.com/LowkeyLab/bazel-repo/issues/2414).
The inspected `origin/main` commit is `e9040f81672138d94a60e4c996a190f929394da6`, acquired on 2026-10-18.
The repository pins Bazel 9.3.0. [Bazel version](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelversion)

The report distinguishes three evidence types:

- **Source:** checked-in configuration or the exact pinned rule implementation.
- **Graph:** a successful Bazel query against the inspected commit.
- **Runtime:** records from an actual combined coverage report.

This investigation collected source and graph evidence. It did not execute coverage or inspect a hosted coverage artifact.
Thus, a configured path means a supported route, not proof of an uploaded record.

## Main finding

Automatic package selection alone cannot establish complete source membership.
The current command selects all ordinary tests, but it selects extra production targets only when they carry `python-coverage`.
A separate manual instrumentation filter names project directories.
The Python wrappers address sources outside test dependencies. Other languages need their own membership checks.
[Coverage flags](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelrc#L100),
[Python wrappers](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/defs.bzl)

The pinned Go rules already support zero-hit baselines for untested packages.
The pinned JavaScript rules already support zero-hit records for files in a test's coverage manifest.
A design must use these capabilities before adding another collector.
[Go baseline action](https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/private/actions/baseline_coverage.bzl),
[JavaScript collector](https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/coverage/coverage.js)

## Stages where membership can change

| Stage             | Current route                                                                                                                     | Gap or limit                                                                                                                         | Evidence                                                                                                                                                                                                                                                                                                                                                                         |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Discovery         | CI calls `bazel run //tools/coverage -- //...`. The wrapper calls `bazel coverage`.                                               | Ignored directories and `manual` targets do not enter wildcard selection automatically.                                              | [Workflow](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.github/workflows/coverage.yml), [wrapper](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/coverage/coverage.sh), [ignore list](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelignore) |
| Target selection  | `--nobuild_tests_only` permits production targets. `--build_tag_filters=python-coverage` selects tagged production rules.         | This build tag filter does not restrict the selected tests. It does restrict extra non-test roots.                                   | [Flags](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelrc#L100), local `bazel help coverage --long`                                                                                                                                                                                                                                 |
| CI test selection | Remote CI applies test tag filters. Coverage excludes `no-coverage`.                                                              | Angular Pinyin Composer has this tag. Multiple `--test_tag_filters` settings require verification of effective flags.                | [Flags](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelrc), [Pinyin target](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/angular/projects/pinyin-composer/BUILD.bazel#L76)                                                                                                                 |
| Instrumentation   | The filter lists current package prefixes and `.python_coverage` metadata labels. The configuration enables test instrumentation. | New non-Python package prefixes need manual maintenance. Custom collectors can apply additional source selection.                    | [Flags](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelrc#L104)                                                                                                                                                                                                                                                                     |
| Collection        | Rules produce test LCOV. Python and Go can supply production baselines.                                                           | Instrumentation does not guarantee collection from a subprocess or another runtime.                                                  | [Rust collector](https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/coverage/collect_rust_coverage.rs), [Go baseline](https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/private/actions/baseline_coverage.bzl)                                                                              |
| Aggregation       | Bazel merges LCOV locally. CI downloads top-level outputs. The preset fetches coverage outputs.                                   | These settings cannot create records for omitted sources. This investigation did not compare cached and executed report equivalence. | [Flags](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelrc), [preset](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/preset.bazelrc#L52)                                                                                                                                                |
| Upload            | The wrapper copies the combined report to `coverage-report.lcov`. Codecov uploads that file with upload errors fatal.             | The workflow does not compare uploaded source membership with a production inventory.                                                | [Wrapper](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/coverage/coverage.sh), [workflow](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.github/workflows/coverage.yml#L50)                                                                                                                |

## Project and language matrix

Every row below has source evidence. Graph findings refer to the queries in the next section.
No row claims runtime confirmation.

| Projects or area                                 | Language and route                                                                        | Membership risk or boundary                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Nicknamer, Nicknamer 2                           | Rust through `rules_rs` and its pinned `rules_rust`. Both prefixes match the filter.      | Tests can measure linked code. Graph evidence identifies server entry points, configuration, authentication, and GraphQL roots outside all test dependencies. [Nicknamer targets](https://github.com/LowkeyLab/bazel-repo/tree/e9040f81672138d94a60e4c996a190f929394da6/nicknamer), [Nicknamer 2 targets](https://github.com/LowkeyLab/bazel-repo/tree/e9040f81672138d94a60e4c996a190f929394da6/nicknamer2)                                                                                                                                                                                                                                                |
| Prediction Bot                                   | Rust. Its prefix matches the filter.                                                      | Preserve integration-test instrumentation. A matching prefix does not establish complete production membership. [Targets](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/prediction_bot/lib/BUILD.bazel)                                                                                                                                                                                                                                                                                                                                                                                                            |
| Book Smartz                                      | Rust plus browser JavaScript. Its prefix matches the filter.                              | Tests start a packaged server. The Rust collector exports the test binary's mapping, not every executable in `data`. Check packaged-server records separately. `storage/usage/lib.rs` lies outside the queried test source closure. [Web targets](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/book_smartz/web/BUILD.bazel), [server start](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/book_smartz/web/test_support.rs), [collector](https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/coverage/collect_rust_coverage.rs) |
| Hearthstone Simulator, Kafka Calculator          | Rust. Both prefixes match the filter.                                                     | Hearthstone's app entry point lies outside the queried test source closure. The Rust provider supplies no line-bearing production baseline. [Application target](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/hearthstone_simulator/app/BUILD.bazel), [Rust provider](https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/private/rustc.bzl)                                                                                                                                                                                                                           |
| Pinyin Composer backend                          | Rust native tests and a separate WebAssembly shared library.                              | `pinyin-composer` does not match the filter. Native tests do not establish WebAssembly execution coverage. The generated bindings need an explicit source policy. [Targets](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/pinyin-composer/wasm/BUILD.bazel)                                                                                                                                                                                                                                                                                                                                                        |
| `src`                                            | Standalone Rust binary.                                                                   | No matching filter prefix. `main.rs` lies outside the queried test source closure. [Target](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/src/BUILD.bazel)                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Predix                                           | Go. Its prefix matches the filter.                                                        | Tested libraries have a collector route. `predix/app/main.go` lies outside the queried test source closure. Extra production roots currently require the Python tag. [Application](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/predix/app/BUILD.bazel), [Go provider](https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/private/rules/library.bzl)                                                                                                                                                                                                                      |
| Cowsay                                           | Go. Its prefix matches the filter.                                                        | The query finds no Cowsay test. `main.go` lies outside every test source closure. The Go baseline capability needs a selected production root. [Target](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/cowsay/cmd/hello/BUILD.bazel), [baseline](https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/private/actions/baseline_coverage.bzl)                                                                                                                                                                                                                                  |
| Mindreadr                                        | Kotlin/JVM with JaCoCo support in the pinned Kotlin rules. Its prefix matches the filter. | `Application.kt` lies outside the queried test source closure. A selected test cannot establish records for every production JVM root. [Targets](https://github.com/LowkeyLab/bazel-repo/tree/e9040f81672138d94a60e4c996a190f929394da6/mindreadr), [Kotlin provider](https://github.com/bazel-contrib/rules_kotlin/blob/09931b5ce94251d94b803c0d0a6c535896e67d16/kotlin/internal/jvm/compile.bzl)                                                                                                                                                                                                                                                          |
| Ktor Tutorial                                    | Kotlin/JVM. An app test exists.                                                           | Its prefix does not match the filter. Check instrumentation before collector behavior. [Targets](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/ktor_tutorial/BUILD.bazel)                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Angular applications                             | Six `js_test` targets run Angular/Vitest. The `angular` prefix matches the filter.        | Each app configures coverage and an LCOV copy adapter. CI coverage excludes Pinyin Composer. Missing Vitest output prints an error message but returns successfully. [Angular configuration](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/angular/angular.json), [adapter](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/angular/projects/nicknamer/vitest-global-setup.ts), [test wrapper](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/angular/bzl/ng.bzl)                                                                     |
| Personal Website                                 | Astro, TypeScript, JavaScript, Go, and shell tests.                                       | Its prefix does not match the filter. Generic `js_test` supports Node coverage, including manifest baselines. This does not measure Chromium execution or establish Astro template coverage. [Browser tests](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/personal_website/tests/browser/BUILD.bazel), [Node collector](https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/js_binary.bzl)                                                                                                                                                                          |
| Clojure Sample                                   | Clojure libraries plus a `java_test` wrapper.                                             | Its prefix does not match the filter. The Clojure library implementation returns `JavaInfo`, without `InstrumentedFilesInfo`. Treat Clojure source coverage as unresolved support. [Sample](https://github.com/LowkeyLab/bazel-repo/tree/e9040f81672138d94a60e4c996a190f929394da6/clojure_sample), [rule](https://github.com/griffinbank/rules_clojure/blob/c0322b0251489b5d9f883e55ec093e90cb0dbdd9/rules.bzl), [provider](https://github.com/griffinbank/rules_clojure/blob/c0322b0251489b5d9f883e55ec093e90cb0dbdd9/rules/jar.bzl)                                                                                                                      |
| Python Sample, `tools/python`, `.github/scripts` | Shared `rules_py` wrappers.                                                               | Tagged production roots supply zero-hit baselines outside test dependencies. Metadata labels match the suffix filter, including future package paths. [Wrappers](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/defs.bzl), [acceptance test](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/coverage_membership_test.py)                                                                                                                                                                                                                           |
| Tools, test images, and vendored Gazelle Rust    | Rust, Go, shell, and build tools.                                                         | `tools/topcoat` and `test_images` lack matching filter prefixes. Vendored `3rdparty/gazelle_rust` explicitly matches. Ownership and denominator policy need separate decisions. [Filter](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelrc#L108), [Topcoat](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/topcoat/BUILD.bazel)                                                                                                                                                                                                                                  |

## Graph evidence

The following commands completed successfully through `nix develop --command`:

```bash
bazel query 'kind(".*_test rule", //...)' --output=label_kind
bazel query 'kind("(rust_library|rust_binary|go_library|kt_jvm_library|clojure_library|py_library|py_binary) rule", //...) except deps(tests(//...))' --output=label_kind
bazel query 'labels(srcs, kind("(rust_library|rust_binary|go_library|kt_jvm_library|py_library|py_binary) rule", //...)) except labels(srcs, deps(tests(//...)))' --output=label
```

The third query returned these source labels:

```text
//book_smartz/storage:usage/lib.rs
//cowsay/cmd/hello:main.go
//hearthstone_simulator/app:main.rs
//mindreadr/src/main/io/lowkeylab/mindreadr/app:Application.kt
//nicknamer/migration/bin:src/main.rs
//nicknamer/server/bin:src/main.rs
//nicknamer2/src:lib.rs
//nicknamer2/src/auth:lib.rs
//nicknamer2/src/bin:main.rs
//nicknamer2/src/config:lib.rs
//nicknamer2/src/graphql:lib.rs
//predix/app:main.go
//src:main.rs
//tools/python:coverage_membership_test.py
```

This is an unconfigured graph result, not a complete production inventory.
The query checks `srcs`, not `main`, `resources`, `compile_data`, generated output contents, or runtime file access.
It also includes test support and tools until a production policy excludes them.
It does not apply the CI test tag filter.
A source can exist in a runtime package without an ordinary `srcs` dependency path.
[Bazel query semantics](https://bazel.build/query/language)

## Python comparison and exclusions

Python already provides the intended example for automatic membership.
The root Gazelle mappings route generated Python libraries, binaries, and tests through shared wrappers.
Each eligible library or binary receives a production tag and a metadata target.
The metadata target declares a real zero-hit LCOV action.
[Root mappings](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/BUILD.bazel#L91),
[wrappers](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/defs.bzl),
[baseline collector](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/coverage_baseline.py)

The wrapper excludes `testonly` rules, external repositories, `3rdparty/` packages, generated files, and non-Python extensions from production baselines.
Its metadata targets carry `manual`, but their production parents depend on them in coverage mode.
Thus, their `manual` tag does not prevent the declared parent route.
The same distinction applies to other generated internal targets.
[Wrapper conditions](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/defs.bzl#L11)

The manual Python membership test creates ordinary and hidden packages outside the original directory list.
It checks imported execution, zero-hit sources outside test dependencies, exclusions, merged records, and a baseline failure.
This report inspected that test but did not execute it.
Preserve the accepted `rules_py` decision and this source-discovery behavior.
[Acceptance test](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/tools/python/coverage_membership_test.py),
[accepted ADR](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/docs/adr/0002-use-rules-py-for-first-party-python.md)

There is no universal rule that all hidden directories are absent.
The successful target query includes `.github/scripts` tests.
The ignore list excludes `.claude`, `.direnv`, `.local`, `.worktrees`, `node_modules`, and `target`.
Gazelle exclusions are separate from Bazel target discovery.
[Ignore list](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/.bazelignore),
[Gazelle exclusions](https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/BUILD.bazel#L98)

## Decisions this evidence supports

1. Define production membership independently of existing tests and current project directory names.
2. Preserve the Python baseline mechanism and its explicit exclusions.
3. Consider native Go baselines before designing another Go collector.
4. Separate Node, Chromium, JVM, Rust native, and WebAssembly evidence requirements.
5. Require source-record checks for a new package and a file outside all test dependencies.
6. Decide whether tools, test support, generated sources, and vendored code belong in each denominator.
7. Preserve existing thresholds. Resolve missing-source behavior before comparing percentages.

The next decision must specify the expected source set for each supported rule family.
Then compare that set with normalized `SF:` records in the combined report.
Require line-bearing records for a source with executable statements.
A filename with `LF:0` does not prove a useful zero-hit baseline.
[Go baseline rationale](https://github.com/bazel-contrib/rules_go/blob/7d3f58b093ddab21ecb4b429d8eefafaf64999f1/go/private/actions/baseline_coverage.bzl)

## Validation limits

The queries inspect the current source graph without implementation changes.
They do not prove collection, aggregation, fresh remote execution, cache correctness, Codecov membership, or threshold behavior.
No full build or implementation test is necessary for this Markdown-only report.

Full repository formatting passed. The strict English checker reported no violations.
The repository prose test failed with 17 existing errors in `docs/adr/0001-extract-worktree-pool.md`.
It reported no errors or warnings in this report.
[Prose test invocation](https://app.buildbuddy.io/invocation/3ce0e0ea-d18a-4c85-a53e-315896d49a7b)
