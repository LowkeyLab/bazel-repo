# Test stable Rust WebAssembly profiling through wasm-bindgen

Date: 2026-10-20.
Research base: `39075c1280f357d58b06a9a471e617cd73949bdf`.
Question: [Test stable Rust WebAssembly profiling through wasm-bindgen][question].
Earlier experiment: [Determine a compatible WebAssembly coverage runtime and export path][prior].

## Result and remaining decision

The selected stable application compiler works with a separately built profiling runtime in this bounded fixture.
The runtime wrapper requires the internal `profiler_runtime` attribute.
Its build uses `RUSTC_BOOTSTRAP=profiler_builtins` for that crate only.
The application build uses stable Rust `1.99.1` without bootstrap or `-Z` flags.
The maintainer must approve this unstable runtime-build condition before production adoption.
This report approves no compiler change or collector exception. [Compiler evidence](stable-wasm-2479/evidence/rustc-vV.txt), [runtime action](../../tools/research_wasm/probe.bzl)

Chromium executes the actual wasm-bindgen `--target web` module and transfers 800 raw profile bytes through HTTP.
LLVM exports measured original Rust lines from the retained pre-transform Wasm and the matching IR-derived object.
Both routes also export executable zero-hit baseline records without browser execution.
LLVM still rejects the transformed module as a mapping object.
These are separate export results. [Browser observation](stable-wasm-2479/evidence/browser.json), [export status](stable-wasm-2479/evidence/summary.json)

The candidate mechanism retains the exact pre-transform object with each transformed module.
It transfers the profile from that module's browser instance.
It exports native and Wasm observations separately, then merges canonical original-source LCOV.
The IR-derived object is a tested alternative mapping input.
Automatic source ownership and a portable remote browser remain separate implementation questions.
The [collector decision][decision] remains a human decision.

## Exact inputs

| Component                                | Selected input                                                                  |
| ---------------------------------------- | ------------------------------------------------------------------------------- |
| Bazel                                    | `9.3.0`                                                                         |
| Application and runtime-wrapper compiler | Rust `1.99.1`, commit `58cf8f9c35b355199727627d07ae34a4e2846d67`, LLVM `23.1.1` |
| Wasm linker                              | LLD `23.1.1`, LLVM commit `1b9c0d5ff9bbe7634aead059efe6b11a7eeba145`            |
| Wasm target                              | `wasm32-unknown-unknown`, edition `2024`                                        |
| Runtime C sources and capture wrapper    | minicov `0.3.9`, source commit `03886b886ce16b9309ddd38e4824970e70fc0588`       |
| Runtime source archive SHA-256           | `c3aa3aa12b448ac225b3102217d1ac5cc717908f02722926524b0599c933c7a0`              |
| Clang, llvm-ar, llvm-cov, llvm-profdata  | `23.1.2`                                                                        |
| wasm-bindgen crate and CLI               | `0.2.129`                                                                       |
| Playwright                               | `1.64.1`                                                                        |
| Chromium Headless Shell                  | `156.0.8078.4`, revision `1248`                                                 |
| Browser archive SHA-256                  | `c3e50596aef52c58f8fda3c6b3411c33d34c88b177b3ed1c2243359fd3b8ba20`              |
| Node                                     | `24.15.0`                                                                       |

Current main retains the earlier Rust, LLVM, wasm-bindgen, and browser pins.
The experiment reuses the earlier fixture and changes its compiler selection to the registered stable toolchain.
It adds a runtime metadata wrapper, a disconnected source, and checks for both merge orders.
It does not register another production toolchain. [Base pins][pins], [compiler and tool digests](stable-wasm-2479/evidence/compiler-tool-digests.sha256), [module digests](stable-wasm-2479/evidence/module-digests.json), [linker version](stable-wasm-2479/evidence/linker-version.txt)

## Stable compiler and runtime metadata

The pinned compiler injects a profiler runtime when coverage instrumentation is active.
It then checks the runtime crate's `is_profiler_runtime` metadata.
A C archive alone cannot satisfy that Rust metadata check. [Pinned compiler source][injection]

The separate Rust wrapper contains:

```rust
#![feature(profiler_runtime)]
#![profiler_runtime]
#![no_std]
```

The wrapper contains no application functions.
A normal stable build rejects its feature attribute with `E0554`. [Runtime gate](stable-wasm-2479/evidence/runtime-gate.json)
The runtime-only bootstrap build succeeds with the same selected compiler.
The compiler declares `profiler_runtime` as an internal feature. [Pinned feature declaration][feature]

The runtime action compiles minicov's ten C sources for bare-metal Wasm with Clang `23.1.2`.
It also compiles minicov's capture wrapper with stable Rust.
Only the metadata wrapper receives `RUSTC_BOOTSTRAP=profiler_builtins`.
The subsequent application command links that wrapper and the C runtime without bootstrap.
The Rust manual documents the crate-scoped override and its unstable compatibility contract. [Runtime procedure][runtime-build], [bootstrap contract][bootstrap]

The application flags are:

```text
--target wasm32-unknown-unknown --edition=2024 --crate-type cdylib
-Cpanic=abort -Copt-level=0 -Cinstrument-coverage
--extern profiler_builtins=<separately-built-runtime-wrapper.rlib>
-Clink-arg=--no-gc-sections -Clink-arg=--emit-relocs --emit=link,llvm-ir
```

The normal Rust rule's profiler-builtins gate remains unchanged.
This research uses an explicit Bazel compiler action to test the substitute runtime.
A production adapter would need an approved runtime-build policy and an explicit integration with that gate.
Stable application compilation does not make the runtime wrapper's internal attribute stable.

## Browser execution and mapping identity

The browser imports the generated `fixture.js` and initializes its transformed `fixture_bg.wasm`.
Its functional result is 83.
The browser calls the dump export and reads bytes from that same instance's linear memory.
It posts those bytes to the test's HTTP server.
Node writes the received bytes into the test outputs. [Browser driver](../../tools/research_wasm/browser.mjs), [browser records](stable-wasm-2479/evidence/browser.json)

The runtime action emits the linked Wasm and LLVM IR in one compiler invocation.
The wasm-bindgen action transforms that exact linked Wasm.
The IR mapping object derives from that same invocation's IR.
The driver hashes both Wasm modules, the IR, the mapping object, and the runtime archive.
This pairing avoids using an independently rebuilt object's mappings. [Action definitions](../../tools/research_wasm/probe.bzl), [module digests](stable-wasm-2479/evidence/module-digests.json)

Compatibility checks exceed equality of raw-profile version constants.
The indexed profile preserves compiler-generated function names, function hashes, counter counts, and measured counts.
Both executed subject functions have matching hashes, one counter, and measured function count one.
The uncalled function retains a mapping and has no measured profile entry.
LLVM exports its original executable lines with zero counts. [Compiler/profile correspondence](stable-wasm-2479/evidence/profile-correspondence.json), [indexed counters](stable-wasm-2479/evidence/profile-functions.stdout)

The raw profile version is 11.
Successful decoding, identity checks, original-source exports, and both merge orders establish compatibility for this fixture.
They do not establish compatibility for every instrumentation mode or future compiler release.

Both modules retain coverage custom sections and named profiler data segments.
The section decoder records binary structure only.
LLVM supplies executable-line mappings. [Before transformation](stable-wasm-2479/evidence/wasm-sections-raw.json), [after transformation](stable-wasm-2479/evidence/wasm-sections-transformed.json)

| Mapping input                        | Empty baseline | Measured export |
| ------------------------------------ | -------------- | --------------- |
| Exact pre-transform Wasm             | Pass           | Pass            |
| Transformed Wasm                     | Fail           | Fail            |
| ELF object from the same compiler IR | Pass           | Pass            |

LLVM reports `out of order section type: 0` for the transformed module.
The browser executes that module successfully.
The failure is an observed LLVM mapping-reader incompatibility with this transformed artifact.
It does not invalidate export through the retained original object.
This investigation does not isolate the role of `--emit-relocs`. [LLVM reader][reader], [observed status](stable-wasm-2479/evidence/summary.json)

## Original lines, disconnected sources, and merging

The driver compares original-source records with explicit expected counts.
It checks a shared executed body, an uncalled body, and separate runtime-only bodies.
It also checks a compiled source file that neither runtime calls.
The compiler supplies real executable zero-hit lines for that disconnected source.
This check covers a compiled source within the fixture closure.
It does not establish discovery outside that closure.
No source scanner invents executable records. [Expected records](stable-wasm-2479/evidence/expected.json), [disconnected records](stable-wasm-2479/evidence/disconnected.json)

| Source and line                   | Empty Wasm baseline | Browser | Native | LCOV merge |
| --------------------------------- | ------------------- | ------- | ------ | ---------- |
| `subject.rs:3`, shared body       | 0                   | 1       | 1      | 2          |
| `subject.rs:8`, uncalled body     | 0                   | 0       | 0      | 0          |
| `subject.rs:15`, Wasm-only body   | 0                   | 1       | Absent | 1          |
| `subject.rs:19`, native-only body | Absent              | Absent  | 1      | 1          |
| `disconnected.rs:2-4`             | 0 each              | 0 each  | 0 each | 0 each     |

Both pre-transform and IR-derived exports preserve these Wasm records.
The adapter canonicalizes both sources below `tools/research_wasm/`.
The actual Bazel coverage merger preserves one canonical subject record in both input orders.
Shared counts equal two, runtime-only counts equal one, and uncalled counts remain zero.
Both merge orders retain the disconnected source's executable zero records.
Declared coverage and combined LCOV each retain one disconnected record with `DA:2,0`, `DA:3,0`, and `DA:4,0`.
[Declared coverage](stable-wasm-2479/evidence/coverage.dat), [combined LCOV](stable-wasm-2479/evidence/combined.dat) [Wasm export](stable-wasm-2479/evidence/measured-raw.stdout), [IR export](stable-wasm-2479/evidence/measured-ir.stdout), [both LCOV orders](stable-wasm-2479/evidence/lcov-merges.json)

LLVM also merges native and Wasm raw profiles in both orders.
Exporting each matching object from either merged profile preserves its original runtime counts.
This fixture uses distinct compiler-generated identities for the two binaries.
The result does not establish identity handling for every cross-target crate arrangement.
The recommended collector still exports per matching object before canonical LCOV aggregation. [Merge status](stable-wasm-2479/evidence/summary.json)

The driver repeats four negative controls after functional success.
Missing profile, corrupt profile, missing mapping, and suppressed browser transfer each cause collection failure.
The suppressed-transfer browser instance still returns the functional result 83.
Explicit expected inputs enforce missing-data failures.
This does not establish automatic discovery of every process or source owner. [Control status](stable-wasm-2479/evidence/summary.json), [transfer control](stable-wasm-2479/evidence/browser.json)

## Execution and validation

BuildBuddy executes fixture build actions.
Chromium and coverage collection execute locally inside a fresh Bazel test.
The successful check reports 2622 action-cache hits, 425 remote-cache hits, and one executed remote action.
It also reports two sandboxed actions and one local action.
The browser test passes in 2.4 seconds.
These counts distinguish fresh execution from cached build results. [Successful check](https://app.buildbuddy.io/invocation/fd31a095-25ae-4a85-94cf-13b8050d4a09)

The earlier stable fixture build executed 163 remote actions and reached real browser execution.
Its driver stopped on a hash string comparison that incorrectly treated leading zeroes as different hashes.
The corrected driver compares numeric hashes.
A fixture launcher correction also sets the Java merger's runfiles root explicitly. [Earlier build](https://app.buildbuddy.io/invocation/f3e7979f-3cbe-4c8b-8d3c-45a7b6692406)

The browser uses the pinned archive and explicit Nix host libraries.
This check establishes no remote browser execution or portable remote browser image.
No hosted workflow or Codecov upload ran.
The Python architecture, combined LCOV policy, thresholds, and Codecov policy remain unchanged.
The corrected fixture also preserves disconnected executable zeros through the declared and combined outputs.
Its fresh browser check executes locally and reports two executed remote build actions.
[Propagation check](https://app.buildbuddy.io/invocation/de87acdc-2d4e-49dc-bc0f-ac01559e3a6a)

Final repository checks appear in the [validation record](stable-wasm-2479/evidence/validation.json).
The [reproduction commands](stable-wasm-2479/REPRODUCE.md) use the pinned Nix environment.

Raw logs and compiled artifacts remain in private runtime storage.
The branch contains inspected fixture records, versions, digests, and sanitized reproduction commands.
It contains no environment dump, pool context, or pool receipt.

[question]: https://github.com/LowkeyLab/bazel-repo/issues/2479
[prior]: https://github.com/LowkeyLab/bazel-repo/blob/3c51334f2ffc3d62ea5e5e15f139fe84b5ef1b34/docs/research/2026-10-20-wasm-coverage-runtime.md
[decision]: https://github.com/LowkeyLab/bazel-repo/issues/2445#issuecomment-6287451818
[pins]: https://github.com/LowkeyLab/bazel-repo/blob/39075c1280f357d58b06a9a471e617cd73949bdf/MODULE.bazel
[injection]: https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_metadata/src/creader.rs
[feature]: https://github.com/rust-lang/rust/blob/58cf8f9c35b355199727627d07ae34a4e2846d67/compiler/rustc_feature/src/unstable.rs
[runtime-build]: https://github.com/Amanieu/minicov/blob/03886b886ce16b9309ddd38e4824970e70fc0588/minicov/build.rs
[bootstrap]: https://doc.rust-lang.org/nightly/unstable-book/compiler-environment-variables/RUSTC_BOOTSTRAP.html
[reader]: https://github.com/llvm/llvm-project/blob/llvmorg-23.1.2/llvm/lib/ProfileData/Coverage/CoverageMappingReader.cpp
