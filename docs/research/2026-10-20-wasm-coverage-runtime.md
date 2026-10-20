# Determine a compatible WebAssembly coverage runtime and export path

Date: 2026-10-20.
Research base: `61e9f955bc2d75f7108c6332abae49e232ea21c4`.
Question: [Determine a compatible WebAssembly coverage runtime and export path][question].
Prerequisite: [Rust native and WebAssembly investigation][prior].

## Result

A retained pre-transform Wasm object exports browser-measured original Rust LCOV.
An ELF mapping object from the same compiler's LLVM IR also works.
LLVM 23.1.2 rejects the transformed Wasm with `out of order section type: 0`.
Keep these three results separate. [Export evidence](wasm-2447/evidence/summary.json)

The demonstrated combination uses the research-only `nightly/2026-08-15` compiler, minicov `0.3.9`, and LLVM `23.1.2` tools.
The browser executes the repository's wasm-bindgen `--target web` output.
It transfers 784 raw profile bytes through HTTP from the actual Wasm instance.
Both mapping routes preserve a browser-only positive line and an uncalled zero-hit function.
The real empty baseline needs no browser execution. [Browser evidence](wasm-2447/evidence/browser.json), [raw Wasm export](wasm-2447/evidence/measured-raw.stdout), [IR export](wasm-2447/evidence/measured-ir.stdout)

This result establishes a bounded candidate. The later [collector mechanism decision][decision] remains a human decision.
The production compiler selection remains stable Rust `1.99.1`. This research approves no exception.

## Effective pins

| Component                               | Effective input                                                                                                         |
| --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Bazel                                   | `9.3.0`                                                                                                                 |
| Production Rust                         | `1.99.1`, commit `58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM `23.1.1`                                              |
| Registered dated nightly                | `nightly/2026-01-11`, Rust `1.94.0-nightly`, commit `f57eac1bf98cb5d578e3364b64365ec398c137df`, LLVM `21.1.8`           |
| Research compiler                       | `nightly/2026-08-15`, Rust `1.99.0-nightly`, commit `d453bdd8f092d099bc336f0bda4163f809ad18e0`, LLVM `23.1.0`           |
| Target                                  | `wasm32-unknown-unknown`, edition `2024`                                                                                |
| Runtime                                 | minicov `0.3.9`, source commit `03886b886ce16b9309ddd38e4824970e70fc0588`                                               |
| Runtime archive SHA-256                 | `c3aa3aa12b448ac225b3102217d1ac5cc717908f02722926524b0599c933c7a0`                                                      |
| Clang, llvm-ar, llvm-cov, llvm-profdata | `23.1.2`                                                                                                                |
| wasm-bindgen crate and CLI              | `0.2.129`                                                                                                               |
| Playwright                              | `1.64.1` from the repository lock                                                                                       |
| Chromium Headless Shell                 | `156.0.8078.4`, Playwright revision `1248`                                                                              |
| Browser archive SHA-256                 | `c3e50596aef52c58f8fda3c6b3411c33d34c88b177b3ed1c2243359fd3b8ba20`                                                      |
| Node                                    | `24.15.0`, from the selected Bazel runtime                                                                              |
| Host libraries                          | Nixpkgs `f20cd90e28fe0403873f473b29e42662cd86e8c4`, explicit [library list](../../tools/research_wasm/browser-deps.nix) |

The historical production versions in the ticket still match this base.
The research compiler and the January nightly are distinct experimental inputs.
The Clang/exporter patch version differs from the research compiler's LLVM patch version.
The successful profile merge and exports establish compatibility for this fixture. They do not establish every LLVM configuration.

[Compiler outputs](wasm-2447/evidence/research-rustc-vV.txt), [compiler/tool digests](wasm-2447/evidence/compiler-tool-digests.sha256), [module/runtime/browser digests](wasm-2447/evidence/module-digests.json), [CLI digest](wasm-2447/evidence/bindgen.sha256).
Primary sources: [repository modules][modules], [minicov source][minicov], [runtime build procedure][minicov-build], [browser pin][browser-pin].

## Instrumentation and the profiler gate

The selected stable Wasm toolchain reports `coverage_supported=False`.
Plain compilation succeeds. Explicit `-Cinstrument-coverage` fails with `E0463` because `profiler_builtins` is absent.
Stable Rust also rejects `-Zno-profiler-runtime`.
The January nightly accepts that flag, but its compiler uses LLVM `21.1.8`.
It does not establish compatibility with this LLVM 23 runtime. [Compiler probes](wasm-2447/evidence/compiler-probes.json)

The pinned Rust rule requires `has_profiler_builtins` for normal coverage instrumentation. [Rule gate][rust-rule], [toolchain gate][toolchain-gate]
The temporary [Bazel action](../../tools/research_wasm/probe.bzl) supplies instrumentation explicitly and links the substitute runtime.
It selects the August nightly through a fixture-specific transition.
It adds no production toolchain registration and uses no `RUSTC_BOOTSTRAP` or rustup override.

Automatic Bazel coverage also selected native compiler-rt profiling sources for Wasm.
That compilation failed on missing `errno.h`.
The fixture transition disables automatic coverage within its Wasm dependencies.
The fixture's direct rustc command still supplies `-Cinstrument-coverage -Zno-profiler-runtime` and minicov.
Native compilation retains standard coverage configuration. [Integration failure](wasm-2447/evidence/automatic-wasm-profile-failure.txt)

The runtime action compiles minicov's ten C sources with Clang, following its pinned build script.
It compiles minicov's Rust wrapper without the `alloc` feature.
A bounded static sink receives `minicov::capture_coverage` bytes.
Minicov checks the raw profile version before capture. The observed raw version is 11. [Runtime implementation][minicov], [profile correspondence](wasm-2447/evidence/profile-correspondence.json)

The Rust flags include:

```text
--target wasm32-unknown-unknown --edition=2024 --crate-type cdylib
-Cpanic=abort -Copt-level=0 -Cinstrument-coverage -Zno-profiler-runtime
-Clink-arg=--no-gc-sections -Clink-arg=--emit-relocs --emit=link,llvm-ir
```

The initial raw-export fixture lacked wasm-bindgen metadata. The CLI rejected missing `clone_ref` intrinsics.
The final fixture uses the matching wasm-bindgen crate and its compiler-matched dependencies.
The CLI then applies the repository's ordinary `--target web` transformation. [Transformation](../../tools/research_wasm/probe.bzl), [production command][production-command]

## Browser capture and counter identity

The Bazel shell test launches the SHA-pinned Chromium binary through Playwright.
The browser imports `fixture.js` and initializes its transformed `fixture_bg.wasm`.
The functional result is 83. It requires the shared function and the Wasm-only value 77.
The browser calls the dump export, reads that instance's memory, and posts its bytes to the local test server.
Node receives those browser bytes and writes `browser.profraw`. [Driver](../../tools/research_wasm/browser.mjs), [functional and transfer evidence](wasm-2447/evidence/browser.json)

The indexed profile has the compiler-generated function names, function hashes, and counter counts for both executed subject functions.
Each function has one counter and a measured function count of one.
The uncalled subject function has compiler mappings but no measured profile entry.
LLVM supplies zero counts for its mapped executable lines. [Compiler/profile comparison](wasm-2447/evidence/profile-correspondence.json), [indexed functions](wasm-2447/evidence/profile-functions.stdout)

The driver hashes the actual mapping inputs and browser executable.
Its section decoder records custom sections and named data segments before and after wasm-bindgen.
The decoder inspects binary structure. It does not discover executable source lines.
Both modules retain `__llvm_covfun`, `__llvm_covmap`, and the named `__llvm_prf_names`, `__llvm_prf_cnts`, and `__llvm_prf_data` segments.
LLVM supplies all executable-line mappings. [Before transformation](wasm-2447/evidence/wasm-sections-raw.json), [after transformation](wasm-2447/evidence/wasm-sections-transformed.json)

## Mapping results

| Mapping input                   | Empty-profile export | Browser-measured export | Observation                                                                              |
| ------------------------------- | -------------------- | ----------------------- | ---------------------------------------------------------------------------------------- |
| Exact pre-transform Wasm        | Exit 0, real zeros   | Exit 0                  | Retain this object with the transformed module and captured profile.                     |
| Transformed `--target web` Wasm | Exit 1               | Exit 1                  | LLVM reports `out of order section type: 0`. Chromium executes this module successfully. |
| Textual compiler LLVM IR        | Exit 1               | Not selected            | LLVM does not recognize textual IR as an object file.                                    |
| ELF object derived from that IR | Exit 0, real zeros   | Exit 0                  | Clang converts the same compilation's IR into a mapping object.                          |

The reader contains direct Wasm support. Its guide's older general limitation does not describe the successful pre-transform result.
The transformed-module failure remains an observed parser limit.
This fixture requests `--emit-relocs`. The investigation does not isolate its role in that error. [LLVM reader][llvm-reader], [pinned guide][wasm-guide]

The IR-derived route uses:

```bash
clang --target=x86_64-unknown-linux-gnu -c -x ir fixture.ll -o fixture-ir.o
```

Clang reports a target-triple override warning and exits zero.
The test never executes or links that ELF object. It uses the object only for source mappings.
Both successful measured exports give the same subject `DA` counts.
The direct pre-transform object is the mapping input for the final collection controls.
[Raw export](wasm-2447/evidence/measured-raw.stdout), [IR export](wasm-2447/evidence/measured-ir.stdout), [IR conversion](wasm-2447/evidence/ir-object.stderr), [transformed failure](wasm-2447/evidence/measured-transformed.stderr)

## Original-source observations and merge

The bounded subject is [subject.rs](../../tools/research_wasm/subject.rs).
Its ordinary executed function, uncalled function, and runtime-specific function share one original source identity.
The empty-profile exports run before browser startup.

| Original line | Meaning              | Empty Wasm baseline | Browser observation | Native observation | Bazel merged count |
| ------------- | -------------------- | ------------------- | ------------------- | ------------------ | ------------------ |
| 3             | Shared executed body | 0                   | 1                   | 1                  | 2                  |
| 8             | Uncalled body        | 0                   | 0                   | 0                  | 0                  |
| 15            | Wasm-only body       | 0                   | 1                   | Absent             | 1                  |
| 19            | Native-only body     | Absent              | Absent              | 1                  | 1                  |

The adapter canonicalizes the declared subject to `tools/research_wasm/subject.rs`.
It exports native and browser profiles separately before LCOV aggregation.
Bazel's coverage runner merges the real baseline, native observation, and browser observation.
The declared test `coverage.dat` and combined report retain one subject `SF` record and both runtime-specific hits.
Runtime-specific function names remain distinct `FN` entries. [Declared coverage output](wasm-2447/evidence/coverage.dat), [combined report](wasm-2447/evidence/combined.dat)

The raw exports also retain harness records.
The bounded LCOV comparison selects the declared subject and does not prove repository-wide source-owner discovery.
The generic Bazel baseline still emits filename-only records for other selected harness sources.
This research supplies no complete baseline for every helper, test source, or disconnected crate.
Preserve the approved [membership policy][membership] and [investigation requirements][requirements].

## Failure controls

Each control invokes the collector in a separate process.
The negative probe checks its actual nonzero exit. It does not replace a failure with an empty success report.

| Control                                          | Functional execution | Collection outcome | Enforcing mechanism                                            |
| ------------------------------------------------ | -------------------- | ------------------ | -------------------------------------------------------------- |
| Remove copied browser profile                    | Already passed       | Exit 1             | Adapter requires a nonempty expected profile.                  |
| Corrupt profile                                  | Already passed       | Exit 1             | `llvm-profdata merge` rejects the bytes.                       |
| Remove copied required mapping object            | Already passed       | Exit 1             | Adapter requires the declared mapping object.                  |
| Suppress transfer from a second browser instance | Result 83 passes     | Exit 1             | Adapter requires that instance's expected transferred profile. |

Missing-input checks belong to the temporary adapter. They are not intrinsic minicov completion checks.
The suppress-transfer case retains successful browser behavior and proves a separate collection failure.
This fixture uses explicit expectations. It does not establish automatic process discovery.
[Statuses](wasm-2447/evidence/summary.json), [collector](../../tools/research_wasm/collector.mjs), [control logs](wasm-2447/evidence)

## Execution and validation

The browser, native functional observation, HTTP transfer, profile merge, and exports execute locally inside a Bazel test.
The browser uses explicit pinned Nix host libraries. This result does not establish a portable remote browser image.
Bazel manages the compiler, LLVM tools, CLI, Node, and archived browser inputs.
The host-library closure is an explicit environment input. [Reproduction](wasm-2447/REPRODUCE.md)

Configured-remote builds supplied fixture artifacts.
The initial runtime build reported one executed remote action.
The compiler-matched dependency build reported 1,184 executed remote actions before its fixture-metadata failure.
The successful browser check used cached build artifacts and executed a fresh local test.
Remote configuration alone does not establish remote browser execution.
[Runtime build](https://app.buildbuddy.io/invocation/225bf8d1-7f20-4161-bb78-33de14b8e0b0), [dependency build](https://app.buildbuddy.io/invocation/0f0a0dca-1f0a-4f91-9f45-79fcb431cefe), [first successful browser check](https://app.buildbuddy.io/invocation/061ab08c-519c-40a8-94e3-4286599d5b3d)

Final validation results appear in [validation evidence](wasm-2447/evidence/validation.json).
They include whole-repository formatting, focused lint, a fresh browser coverage run, and formatted-source hash comparison.
Focused lint exits zero with four Clippy advisory warnings.
The final check executes a fresh local test and reports four executed remote build processes.
[Final browser check](https://app.buildbuddy.io/invocation/549c1ded-8bc5-4d61-be49-7d3eab517ebc)
No hosted workflow or Codecov upload ran for this investigation.
The Python architecture, thresholds, and combined coverage policy retain their current behavior.

Public evidence replaces private roots with documented placeholders in [evidence notes](wasm-2447/EVIDENCE.md).
Private originals retain the compiled objects and complete raw logs.

[question]: https://github.com/LowkeyLab/bazel-repo/issues/2447
[prior]: https://github.com/LowkeyLab/bazel-repo/blob/44fe4a5ddb3ff37cc28a49bdd5f8af3ffda2d5ac/docs/research/2026-10-19-rust-native-wasm-coverage.md
[decision]: https://github.com/LowkeyLab/bazel-repo/issues/2445
[membership]: https://github.com/LowkeyLab/bazel-repo/issues/2417#issuecomment-6277250438
[requirements]: https://github.com/LowkeyLab/bazel-repo/issues/2438#issuecomment-6277423566
[modules]: https://github.com/LowkeyLab/bazel-repo/blob/61e9f955bc2d75f7108c6332abae49e232ea21c4/MODULE.bazel
[minicov]: https://github.com/Amanieu/minicov/blob/03886b886ce16b9309ddd38e4824970e70fc0588/minicov/src/lib.rs
[minicov-build]: https://github.com/Amanieu/minicov/blob/03886b886ce16b9309ddd38e4824970e70fc0588/minicov/build.rs
[browser-pin]: https://github.com/microsoft/playwright/blob/v1.64.1/packages/playwright-core/browsers.json
[rust-rule]: https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/private/rustc.bzl
[toolchain-gate]: https://github.com/hermeticbuild/rules_rust/blob/fb6108715c7e1ab6b0c78b37aef2388e5aeb7cc3/rust/private/toolchain.bzl
[production-command]: https://github.com/LowkeyLab/bazel-repo/blob/61e9f955bc2d75f7108c6332abae49e232ea21c4/pinyin-composer/wasm/BUILD.bazel
[llvm-reader]: https://github.com/llvm/llvm-project/blob/llvmorg-23.1.2/llvm/lib/ProfileData/Coverage/CoverageMappingReader.cpp
[wasm-guide]: https://github.com/wasm-bindgen/wasm-bindgen/blob/0.2.129/guide/src/wasm-bindgen-test/coverage.md
