# Reproduce stable Wasm profiling

Use the research branch and the repository's Nix development environment.
The runtime action contains the exact C and Rust compiler commands.
The browser driver contains the export, identity, merge, and collection checks.

Run Gazelle after source edits:

```bash
nix develop --command bazel --host_jvm_args=-Xmx1536m run //:gazelle \
  --config=ci-remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048
```

Execute a fresh browser coverage check:

```bash
nix develop --impure --file tools/research_wasm/browser-deps.nix --command \
  bazel --host_jvm_args=-Xmx1536m coverage //tools/research_wasm:browser_coverage_test \
  --config=ci-remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048 \
  --build_tag_filters= --instrumentation_filter=//tools/research_wasm \
  --nocache_test_results --test_output=errors
```

The fixture compiles with the selected stable Rust toolchain.
Its metadata wrapper build alone uses `RUSTC_BOOTSTRAP=profiler_builtins`.
The application command uses no bootstrap setting or unstable flags.
The wrapper build also records the expected normal-stable `E0554` failure.
Read `runtime-stable.exit` and `runtime-stable.log` within the runtime action output.

The browser runs locally.
Configured BuildBuddy actions build the runtime, application, and transformed module.
The browser uses the pinned archive with the explicit Nix library closure.
Read the action summary to distinguish executed actions from cache hits.

The test writes these records under its `test.outputs` directory:

- `browser.profraw` contains the bytes from the transformed module's browser instance.
- `summary.json` records export and failure-control statuses.
- `disconnected.json` records zero-hit lines from a compiled but unexecuted source.
- `profile-correspondence.json` compares compiler identity with decoded profile identity.
- `lcov-merges.json` records the actual Bazel merger's counts in both input orders.
- `merged-*.stdout` records exports from both raw-profile merge orders.

The coverage runner also writes declared `coverage.dat` and combined LCOV output.
Private logs and compiled artifacts remain outside the branch.
The report links the inspected technical records needed to assess this fixture.

Run the required repository checks:

```bash
nix develop --command aspect --host_jvm_args=-Xmx1536m format --scope=all \
  --config=ci-remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048 \
  --java_language_version=21 --tool_java_language_version=21 \
  --java_runtime_version=remotejdk_21 --tool_java_runtime_version=remotejdk_21
nix develop --command aspect --host_jvm_args=-Xmx1536m build //... \
  --config=ci-remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048
nix develop --command aspect --host_jvm_args=-Xmx1536m lint \
  --config=ci-remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048
```

The Java formatter selection does not change the selected Rust compiler.
