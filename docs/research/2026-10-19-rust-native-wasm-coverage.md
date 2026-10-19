# Determine Rust coverage guarantees across native and WebAssembly execution

Date: 2026-10-19.
Research base: `151e15cd495b5d63155753c0acf09f50102f0cbc`.
Question: [Determine Rust coverage guarantees across native and WebAssembly execution][question].

## Result and evidence boundary

Native Rust has a compiler-based candidate for real zero-hit baselines. LLVM 23.1.2 exports those baselines without executing a binary.
The current Rust rules do not connect that capability to Bazel baselines.
Separate child executables also need explicit export objects and collection checks.
WebAssembly has candidate mechanisms, but the current repository does not demonstrate their end-to-end operation. [LLVM export][llvm-cov], [Rust provider][rustc], [collector][collector], [Wasm runtime][wasm-runtime]

Use the native compiler mappings and LLVM exporter before considering a new Rust parser.
Require one bounded WebAssembly feasibility investigation before choosing its runtime and export mechanism.
Approve no exception. An unproven case does not establish lack of support.
Preserve the accepted membership policy and investigation policy. [Membership decision][membership], [investigation decision][investigation]

This report inspects pinned source code. It executes no coverage fixture, remote coverage action, or hosted upload.
Source capability, current integration, and demonstrated execution remain separate evidence categories.
The previous reports identify the missing Rust baseline and child-object evidence. This report investigates those gaps. [Mechanism report][prior-mechanisms], [evidence report][prior-evidence]

## Pins and repository boundaries

| Component                  | Current pin                                | Change from the previous research base                       |
| -------------------------- | ------------------------------------------ | ------------------------------------------------------------ |
| Bazel                      | `9.3.0`                                    | No version change.                                           |
| `rules_rs`                 | `f6067c1f0568e24487468a01b7ae7df0b87434cd` | No change.                                                   |
| Underlying `rules_rust`    | `fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3` | No change.                                                   |
| Stable Rust                | `1.99.1`, edition `2024`                   | No change.                                                   |
| Registered nightly         | `nightly/2026-01-11`                       | No change. Registration does not select it for every action. |
| Hermetic LLVM wrapper      | `06ec2b02c2ba8fd8564f83661b14562cb74a9101` | Replaces `3b04c07ed7a95d5c9c1c4a8930efbdf5bac186da`.         |
| LLVM tools                 | `23.1.2`                                   | No version change. The wrapper commit changed.               |
| `wasm-bindgen` and its CLI | `0.2.129`                                  | The report checks this current Cargo lock pin.               |

Sources: [current modules][modules], [previous modules][old-modules], [Rust archive provisioning][provision], [Cargo lock][cargo-lock], [Bazel version][bazel-version].
The Rust toolchain declarations select `@llvm//tools:llvm-cov` and `@llvm//tools:llvm-profdata`. [Toolchain declarations][rs-declare]

Rust `1.99.1` pins its LLVM submodule to `1b9c0d5ff9bbe7634aead059efe6b11a7eeba145`.
That submodule identifies LLVM `23.1.1` and raw profile version 11.
The separately selected export tools use LLVM `23.1.2`.
These pins establish source versions. Check the actual compiler with `rustc -vV` through the selected Bazel toolchain. [Rust submodule][rust-llvm-link], [compiler LLVM version][rust-llvm-version], [compiler profile format][rust-llvm-format]

The repository applies two patches to underlying Rust rules.
The coverage patch disables a working-directory scan during coverage compilation. It does not add collection or baselines.
The other patch concerns Rust Analyzer execution dependencies. [Module patches][modules], [coverage patch][coverage-patch]

Coverage currently enables `--instrument_test_targets` and a manual instrumentation filter.
The positive `python-coverage` build filter restricts additional non-test roots.
Neither setting independently selects disconnected Rust crates. [Coverage configuration][rc], [previous mechanism analysis][prior-mechanisms]

`pinyin-composer/wasm` declares native library tests and a separate `wasm32-unknown-unknown` shared library.
Its genrule invokes the pinned wasm-bindgen CLI with `--target web`.
The native tests do not execute that WebAssembly artifact.
The package declares no wasm-bindgen coverage runtime or runner. [Pinyin targets][pinyin]

## Capability matrix

“Source capability” means the inspected implementation supplies the mechanism. It does not mean this investigation observed a successful run.
“Gap” identifies missing integration or evidence. It does not approve a source exclusion.

| Case                                     | Pinned source capability                                                                                                                                                    | Current gap and required observation                                                                                                   |
| ---------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Native original `.rs` lines              | Rust emits source regions. LLVM exports `DA` records. [Compiler][rust-mapgen], [exporter][llvm-lcov]                                                                        | Observe known executed lines under repository paths.                                                                                   |
| Uncalled functions in a compiled crate   | Rust emits mappings for unused eligible functions, including dummy generic instances. [Unused mappings][rust-unused]                                                        | Check retained mappings under actual optimization and link settings.                                                                   |
| Disconnected native library              | LLVM accepts archives and exports an empty profile. [Reader][llvm-reader], [export command][llvm-cov]                                                                       | Select the configured owner and attach real baseline LCOV. Check known zero-hit lines in its actual `.rlib`.                           |
| Disconnected native executable           | LLVM exports compiled mappings without executing the executable. [Empty profile][llvm-tool]                                                                                 | Add a baseline action and check that unused executable lines remain present.                                                           |
| Independent test crate                   | Rust compiles independent test sources with `is_test = True`. Explicit test instrumentation permits their coverage. [Test rule][rust-rule], [compiler flags][rustc]         | Build its instrumented test binary for baseline export without running its bodies. Check positive records when selected for execution. |
| `rust_test(crate = ...)`                 | The compiler also checks instrumentation of the underlying crate. [Compiler flags][rustc]                                                                                   | Preserve this special case and test-target instrumentation. Check test-only functions separately.                                      |
| Executable helper in `deps`              | Instrumented helper code can contribute mappings when the test links it. [Provider][rustc], [Rust mappings][rust-mapgen]                                                    | Discover test-only owners independently. Check helper lines without excluding `testonly` sources.                                      |
| Helper executable in `data` or a package | Bazel can pass the profile destination to children. LLVM accepts additional objects. [Bazel runner][bazel-runner], [LLVM objects][llvm-cov]                                 | The current collector exports only the test binary. Declare each child mapping object and check child-only hits.                       |
| Missing all raw profiles                 | The collector deliberately returns an empty report. [Collector][collector]                                                                                                  | Require a collection failure when an instrumented execution expects a profile.                                                         |
| Missing one child profile                | The collector merges the profiles that it finds. [Collector][collector]                                                                                                     | An existing parent profile cannot prove child completion. Require per-child evidence.                                                  |
| Corrupt raw profile                      | The collector propagates a failed `llvm-profdata merge` exit. [Collector][collector]                                                                                        | Check corrupt and incompatible data with a passing functional test.                                                                    |
| Native branch records                    | LLVM exports supplied branch regions. Rust defaults to block coverage. [Exporter][llvm-lcov], [Rust configuration][rust-config]                                             | No current Rust branch-record guarantee. Branch instrumentation requires a separate nightly choice and actual `BRDA` evidence.         |
| WebAssembly runtime hits                 | The matching wasm-bindgen release contains a minicov dump and browser/Node transfer code. [Dump][wasm-runtime], [browser transfer][wasm-server], [Node transfer][wasm-node] | This runtime is not wired into this repository. Check compatible instrumentation, transfer, mappings, and child completion.            |
| WebAssembly zero-hit lines               | LLVM supplies empty-profile export. Its reader contains explicit Wasm support. [Export][llvm-cov], [reader][llvm-reader]                                                    | Check retained sections before and after wasm-bindgen. Direct export feasibility remains unproven.                                     |
| Original Rust identity after merging     | Rust supplies filename tables. Bazel merges exact source names. [Compiler][rust-mapgen], [merger][bazel-coverage]                                                           | Canonicalize baseline and execution paths before merging. Check one identity and preserved hits.                                       |

## Native baselines and test membership

The Rust compiler uses `--codegen=instrument-coverage` when the toolchain and Bazel instrumentation conditions permit it.
The toolchain requires LLVM tools and `has_profiler_builtins`.
The provider declares `.rs` sources through `srcs`, with `deps` and `crate` traversal.
The inspected provider supplies no `baseline_coverage_files` argument. Bazel's default baseline writes filenames without executable lines. [Compiler and provider][rustc], [toolchain capability][rust-toolchain], [generic baseline][bazel-baseline]

Rust's default instrumentation includes unused functions.
Its compiler explicitly adds mappings for eligible functions that do not participate in code generation.
These mappings include unused generic functions through dummy instances.
The compiler documentation also warns that optimization can invalidate coverage assumptions. [Compiler documentation][rust-doc], [unused mappings][rust-unused]

LLVM 23.1.2 accepts `-empty-profile` instead of `-instr-profile`.
The mapping loader assigns zero counters when no profile reader exists.
The LCOV exporter derives executable lines from mapped regions. It does not invent a line for every nonempty source line.
Thus, the native baseline candidate requires no test execution and no source-text heuristic. [CLI implementation][llvm-tool], [counter loading][llvm-mapping], [LCOV implementation][llvm-lcov]

The binary reader traverses archive members. A compiled `.rlib` is therefore a candidate mapping input.
An instrumented independent test binary is another candidate. Its baseline export does not execute test bodies.
Build that binary with its actual test configuration so that `cfg(test)` code contributes mappings.
A production library build alone does not establish membership of test-only functions. [Archive reader][llvm-reader], [test compilation][rust-rule]

Proposed adapter:

1. Discover configured source owners from the independent inventory.
2. Select only compilation outputs needed for coverage mappings.
3. Instrument production, test, and helper owners under the approved policy.
4. Export each required mapping set with `llvm-cov export -format=lcov -empty-profile`.
5. Normalize original source identities.
6. Supply baseline artifacts through Bazel's baseline API or the checked combined-report stage.
7. Check executable fixture records and measured hits after merging.

This adapter is a proposal. Source evidence supports its mechanism.
Checking actual `.rlib` members, linking, hidden roots, and test-only mappings belongs to implementation acceptance.
Do not select deployment images or execute disconnected tests merely to create a baseline.
Keep sources without executable mappings distinct from unexpected empty baselines. [Membership decision][membership]

## Separate executables and missing data

Bazel sets `LLVM_PROFILE_FILE` to `${COVERAGE_DIR}/%h-%p-%m.profraw` before test execution.
An instrumented native child that retains this environment can write a distinct profile there.
The Rust collector reads immediate `.profraw` children of `COVERAGE_DIR` and merges them with `--sparse`.
It does not recursively discover nested profile destinations. [Bazel runner][bazel-runner], [Rust collector][collector]

The collector supplies only `TEST_BINARY` to `llvm-cov export`.
A profile contains execution counts, while an object supplies the source mappings.
A child-only source absent from the parent binary cannot gain mappings from its profile alone.
Supply the unstripped child executable as an additional `-object` input.
Do the same for independently loaded shared libraries when their mappings are absent from the parent. [Collector invocation][collector], [multiple-object interface][llvm-cov], [Rust multiple-binary example][rust-doc]

A package or container test needs the instrumented executable from the matching configuration and revision.
Its export action must receive the declared mapping object and transferred profiles.
A native Rust collector attached only to `rust_test` does not automatically cover an unrelated shell or browser driver.
An adapter must connect those drivers to collection explicitly. [Rust test environment][rust-rule], [collector inputs][collector]

The collector accepts two absent-data cases:

- No `.profraw` files produce an empty report.
- An export error containing `no coverage data found` also produces an empty report.

Other merge and export failures propagate their exit status. [Collector failure handling][collector]

Those successful empty reports cannot enforce the approved missing-data contract.
The wrapper checks report existence, not per-process completeness. [Coverage wrapper][wrapper]

Proposed checks must distinguish a planned zero baseline from an executed unit's missing profile.
Record expected instrumented process launches and completion evidence.
Require child profiles, mapping objects, successful merge, and expected executed-line observations before collection succeeds.
Do not let a parent's valid profile satisfy a child's expectation.
Remove one child profile while retaining the parent's profile to check this distinction.
A successful functional exit does not establish a successful profile flush. [Investigation requirement][investigation], [runtime termination behavior][rust-doc]

## Branch coverage boundary

Rust 1.99.1 defines `CoverageLevel::Block` as its default.
`Branch` and `Condition` belong to `-Zcoverage-options`.
The current Rust rule adds `--codegen=instrument-coverage`, without a branch option.
LLVM's LCOV exporter emits `BRDA` only from available branch regions. [Rust defaults][rust-config], [unstable option parser][rust-options], [rule flags][rustc], [LCOV branches][llvm-lcov]

This establishes a current configuration limit: ordinary native Rust instrumentation does not request dedicated branch regions.
It does not establish that Rust or LLVM cannot support branch coverage.
Do not infer branch support from line regions, conditional code, or `BRF:0` summaries.
Preserve Python's separate branch requirement. [Membership decision][membership]

A later nightly branch choice needs a pinned-toolchain experiment with a true path and an unexecuted false path.
Require original-source `BRDA` records for both outcomes.
The registered dated nightly is not equivalent to stable Rust 1.99.1.
Check its compiler options and LLVM compatibility before selecting it.
This report proposes no nightly migration. [Toolchain registrations][modules]

## WebAssembly runtime and original-source mappings

The current `wasm32-unknown-unknown` target has no filesystem implementation that can provide ordinary profile-file output.
Its target documentation states that `std::fs` operations return errors.
Native process-exit collection therefore does not establish browser coverage. [Rust target documentation][rust-target]

The wasm-bindgen `0.2.129` source contains an experimental test-coverage route:

1. Compile with coverage instrumentation and `-Zno-profiler-runtime`.
2. Enable `cfg(wasm_bindgen_unstable_test_coverage)`.
3. Use minicov as the WebAssembly profiling runtime.
4. Call `__wbgtest_cov_dump` to capture raw profile bytes.
5. Transfer those bytes from the actual WebAssembly instance to the host.
6. Index the profile and export original Rust mappings with compatible LLVM tools.

The guide requires nightly and illustrates `--no-gc-sections`. It also discusses LLVM IR as a mapping-object substitute. [Pinned guide][wasm-guide]
The dump implementation uses `minicov::capture_coverage`.
Node writes the returned bytes through `fs.writeFile`.
The browser sends them to `/__wasm_bindgen/coverage`, where the host writes the profile. [Runtime dump][wasm-runtime], [Node driver][wasm-node], [browser driver][wasm-server]

This is source capability in the matching upstream release.
The ordinary Rust rule gates instrumentation on `has_profiler_builtins`.
A substitute runtime therefore needs explicit compiler wiring, beyond adding the runtime dependency. [Instrumentation gate][rustc], [toolchain gate][rust-toolchain]
The repository lock contains neither `wasm-bindgen-test` nor minicov.
Its current production genrule does not use that test runner.
An adapter must collect the actual application instance when browser tests execute the production WebAssembly module. [Cargo lock][cargo-lock], [Pinyin targets][pinyin]

There are two specific compatibility questions:

- The release's test crate pins minicov `0.3.8` for LLVM 22 instrumentation. Its source declares raw format version 10.
  Minicov `0.3.9` declares raw format version 11, matching LLVM 23.1.2's version constant.
  Equal format constants alone do not establish full runtime ABI compatibility. [Test dependencies][wasm-deps], [minicov 0.3.8 source][minicov-eight], [minicov 0.3.9 source][minicov-nine], [LLVM format][llvm-format]
- The pinned guide says LLVM cannot extract profiling symbols from `.wasm` files.
  However, LLVM 23.1.2 explicitly reads Wasm profile names from named data segments and handles coverage-section alignment.
  That code weakens the guide's general limitation. It does not prove that wasm-bindgen preserves the required segments and sections.
  Check direct export before choosing the guide's LLVM-IR substitute. [Pinned guide][wasm-guide], [current reader][llvm-reader]

### Required bounded feasibility investigation

Question: Which pinned WebAssembly instrumentation, runtime, transfer, and mapping-object combination produces valid original Rust LCOV through Bazel?

Use one small fixture through the repository's wasm-bindgen `--target web` transformation.
Give it one executed function, one uncalled function, and one WebAssembly-only executable location.
Run it in the browser. A Node run alone does not prove browser transfer.

Required observations:

1. Record the exact compiler, LLVM version, runtime version, flags, target, and module digest.
   Capture `rustc -vV` from the selected Bazel toolchain and both LLVM tool versions.
2. Identify the compatible profiler runtime without silently changing the production toolchain.
3. Preserve original and transformed `.wasm` mapping sections and named profile segments for inspection.
4. Capture a nonempty profile from the actual browser instance.
5. Transfer that profile into declared Bazel coverage outputs.
6. Run LLVM 23.1.2 merge and direct Wasm export first.
7. If direct export fails, test the guide's LLVM-IR mapping object with compiler-matched tools.
8. Require a positive `DA` count for the WebAssembly-only line and a zero count for the uncalled function.
9. Export an empty profile without browser execution and check real zero-hit lines.
10. Remove the profile, corrupt it, and remove a required mapping object separately. Require visible collection failures.
11. Suppress the browser transfer while its functional test succeeds. Require a collection failure.
12. Merge with a native observation and require one canonical `.rs` identity with preserved measured hits.

The investigation must report direct-Wasm and LLVM-IR results separately.
If the proposed mechanism needs nightly, record the exact compatible nightly instead of assuming the existing registration suffices.
Only this feasibility result can choose between direct Wasm export, LLVM-IR substitution, or a different runtime adapter.
It must not approve an exception. It must not implement repository-wide coverage.

## Canonical paths and baseline merging

Rust rules remap output-base, working-directory, and execution-root paths.
They also remove generated staging prefixes in coverage mode.
The collector uses `-path-equivalence`, then performs string replacements on exported text. [Compiler remapping][rustc], [collector normalization][collector]

These operations do not prove the approved canonical path contract.
The collector's `/tmp/.+` exclusion also needs care when a raw mapping contains an absolute temporary source path.
Check whether an intended original source survives before treating an empty report as a declaration-only file.
Bazel merges records by exact source name. Aliases can create different records or fail source filtering. [Collector filters][collector], [Bazel source merge][bazel-coverage]

Normalize original sources to repository-relative paths before merging baselines and measured reports.
Use declared source mappings for staged copies. Reject unexplained aliases and paths outside the expected inventory.
Exclude generated and external sources without excluding maintained originals.
Do not merge different revisions or incompatible executable-line interpretations. [Membership contract][membership]

The Bazel merger adds observations for matching identities.
A zero baseline must preserve positive measured counts.
Check a repeated source from two native tests and a native/WebAssembly pair separately.
Record the runtime and configuration before aggregation, so a native hit cannot substitute for missing WebAssembly collection. [Bazel merge implementation][bazel-source]

## Follow-up and scope

The native baseline adapter, explicit child objects, and stricter missing-data checks have primary-source mechanisms.
They need implementation acceptance under actual Bazel configurations.
The WebAssembly mechanism still needs the bounded feasibility investigation above before the later human mechanism choice.
A dedicated branch experiment is conditional on a proposal to enable unstable Rust branch instrumentation.
Current source evidence already establishes the default block-only configuration.

No affected source or rule receives an exception from this research.
Keep maintained test code, helpers, hidden packages, and disconnected crates in the expected inventory.
Preserve the Python architecture, remaining Python acceptance work, combined LCOV workflow, thresholds, and Codecov policy. [Accepted policy][membership]

## Validation

The investigation reads repository configuration and immutable upstream sources.
It does not run coverage, compile feasibility fixtures, execute remote coverage actions, or upload reports.
The research result records repository formatting, build, lint, and prose checks.
Those checks check this Markdown change. They do not establish coverage capability at runtime.

[question]: https://github.com/LowkeyLab/bazel-repo/issues/2440
[membership]: https://github.com/LowkeyLab/bazel-repo/issues/2417#issuecomment-6277250438
[investigation]: https://github.com/LowkeyLab/bazel-repo/issues/2438#issuecomment-6277423566
[prior-mechanisms]: https://github.com/LowkeyLab/bazel-repo/blob/c277c511697f5d93b0fb7bcd1f3dcae3d573c848/docs/research/2026-10-18-coverage-inclusion-mechanisms.md
[prior-evidence]: https://github.com/LowkeyLab/bazel-repo/blob/2b0cfa271ebfbe1dcc1701574883f61e62afb702/docs/research/2026-10-18-coverage-completeness-evidence.md
[modules]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/MODULE.bazel#L169
[old-modules]: https://github.com/LowkeyLab/bazel-repo/blob/e9040f81672138d94a60e4c996a190f929394da6/MODULE.bazel#L169
[provision]: https://github.com/hermeticbuild/rules_rs/blob/f6067c1f0568e24487468a01b7ae7df0b87434cd/rs/rules_rust.bzl
[cargo-lock]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/Cargo.lock
[bazel-version]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/.bazelversion
[rc]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/.bazelrc#L88
[coverage-patch]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/3rdparty/rules_rs/skip_working_directory_scan_during_coverage.patch
[pinyin]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/pinyin-composer/wasm/BUILD.bazel
[wrapper]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/coverage/coverage.sh
[rs-declare]: https://github.com/hermeticbuild/rules_rs/blob/f6067c1f0568e24487468a01b7ae7df0b87434cd/rs/toolchains/declare_rustc_toolchains.bzl#L150
[rustc]: https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/private/rustc.bzl#L1599
[rust-rule]: https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/private/rust.bzl#L374
[rust-toolchain]: https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/private/toolchain.bzl#L649
[collector]: https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/coverage/collect_rust_coverage.rs
[rust-doc]: https://github.com/rust-lang/rust/blob/1.99.1/src/doc/rustc/src/instrument-coverage.md
[rust-config]: https://github.com/rust-lang/rust/blob/1.99.1/compiler/rustc_session/src/config.rs#L148
[rust-options]: https://github.com/rust-lang/rust/blob/1.99.1/compiler/rustc_session/src/options.rs#L1642
[rust-mapgen]: https://github.com/rust-lang/rust/blob/1.99.1/compiler/rustc_codegen_llvm/src/coverageinfo/mapgen.rs
[rust-unused]: https://github.com/rust-lang/rust/blob/1.99.1/compiler/rustc_codegen_llvm/src/coverageinfo/mapgen/unused.rs
[rust-target]: https://github.com/rust-lang/rust/blob/1.99.1/src/doc/rustc/src/platform-support/wasm32-unknown-unknown.md
[llvm-cov]: https://github.com/llvm/llvm-project/blob/llvmorg-23.1.2/llvm/docs/CommandGuide/llvm-cov.rst#L493
[llvm-tool]: https://github.com/llvm/llvm-project/blob/llvmorg-23.1.2/llvm/tools/llvm-cov/CodeCoverage.cpp#L678
[llvm-reader]: https://github.com/llvm/llvm-project/blob/llvmorg-23.1.2/llvm/lib/ProfileData/Coverage/CoverageMappingReader.cpp#L1094
[llvm-mapping]: https://github.com/llvm/llvm-project/blob/llvmorg-23.1.2/llvm/lib/ProfileData/Coverage/CoverageMapping.cpp#L867
[llvm-lcov]: https://github.com/llvm/llvm-project/blob/llvmorg-23.1.2/llvm/tools/llvm-cov/CoverageExporterLcov.cpp
[llvm-format]: https://github.com/llvm/llvm-project/blob/llvmorg-23.1.2/llvm/include/llvm/ProfileData/InstrProfData.inc#L776
[bazel-runner]: https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/collect_coverage.sh#L105
[bazel-baseline]: https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/analysis/test/BaselineCoverageAction.java
[bazel-coverage]: https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator/Coverage.java#L33
[bazel-source]: https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/CoverageOutputGenerator/java/com/google/devtools/coverageoutputgenerator/SourceFileCoverage.java#L92
[wasm-guide]: https://github.com/wasm-bindgen/wasm-bindgen/blob/0.2.129/guide/src/wasm-bindgen-test/coverage.md
[wasm-deps]: https://github.com/wasm-bindgen/wasm-bindgen/blob/0.2.129/crates/test/Cargo.toml
[wasm-runtime]: https://github.com/wasm-bindgen/wasm-bindgen/blob/0.2.129/crates/test/src/coverage.rs
[wasm-node]: https://github.com/wasm-bindgen/wasm-bindgen/blob/0.2.129/crates/cli/src/wasm_bindgen_test_runner/node.rs#L62
[wasm-server]: https://github.com/wasm-bindgen/wasm-bindgen/blob/0.2.129/crates/cli/src/wasm_bindgen_test_runner/server.rs#L338
[minicov-eight]: https://docs.rs/crate/minicov/0.3.8/source/c/profile/InstrProfData.inc
[minicov-nine]: https://docs.rs/crate/minicov/0.3.9/source/c/profile/InstrProfData.inc
[rust-llvm-link]: https://github.com/rust-lang/rust/tree/1.99.1/src/llvm-project
[rust-llvm-version]: https://github.com/rust-lang/llvm-project/blob/1b9c0d5ff9bbe7634aead059efe6b11a7eeba145/cmake/Modules/LLVMVersion.cmake
[rust-llvm-format]: https://github.com/rust-lang/llvm-project/blob/1b9c0d5ff9bbe7634aead059efe6b11a7eeba145/compiler-rt/include/profile/InstrProfData.inc
