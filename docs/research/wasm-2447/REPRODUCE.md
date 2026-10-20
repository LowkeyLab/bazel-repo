# Reproduce the bounded Wasm experiment

Use the repository's Nix development environment and Bazel-managed inputs.
The research compiler has a separate declaration. Production registration remains unchanged.

Run Gazelle after source edits:

```bash
nix develop --command bazel --host_jvm_args=-Xmx1536m run //:gazelle \
  --config=ci-remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048
```

Run the actual browser and all bounded controls:

```bash
nix develop --impure --file tools/research_wasm/browser-deps.nix --command \
  bazel --host_jvm_args=-Xmx1536m coverage //tools/research_wasm:browser_coverage_test \
  --config=ci-remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048 \
  --build_tag_filters= --instrumentation_filter=//tools/research_wasm \
  --nocache_test_results --test_output=errors
```

The test runs locally. Its compiler and transformation actions can run remotely.
The driver writes technical artifacts under the test's `test.outputs` directory.
The coverage runner produces the declared `coverage.dat` and the combined `_coverage_report.dat`.
The driver copies transferred profile bytes and normalized LCOV into `COVERAGE_DIR`.

The runtime action contains the exact C and Rust compiler commands.
It compiles the fixture and LLVM IR in one rustc invocation.
It converts that IR into an ELF mapping object with Clang.
The web action uses the pinned production CLI and `--target web`.

Run the three compiler probes separately. Preserve each output before changing configuration.

```bash
nix develop --command bazel --host_jvm_args=-Xmx1536m build \
  //tools/research_wasm:compiler_probe --config=ci-remote \
  --platforms=@rules_rs//rs/platforms:wasm32-unknown-unknown \
  --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048
```

Add this flag for the registered January nightly:

```text
--@rules_rust//rust/toolchain/channel=nightly
```

Add both flags for the separate August compiler:

```text
--@rules_rust//rust/toolchain/channel=nightly
--extra_toolchains=@research_wasm_toolchains//...
```

The compiler probe deliberately records failed compilation commands.
A successful probe action does not mean every contained compilation succeeded.
Read each `.exit`, `.log`, and `rustc-vV.txt` file.

Whole-repository formatting uses the existing Java 21 formatter compatibility selection:

```bash
nix develop --command aspect --host_jvm_args=-Xmx1536m format --scope=all \
  --config=ci-remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048 \
  --java_language_version=21 --tool_java_language_version=21 \
  --java_runtime_version=remotejdk_21 --tool_java_runtime_version=remotejdk_21
```

This formatter selection does not select the Rust compiler.
