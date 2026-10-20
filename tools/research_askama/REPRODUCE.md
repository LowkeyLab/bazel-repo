# Reproduce the bounded Askama experiment

Run these commands from the repository root.

```bash
limits=(--config=ci-remote --jobs=8 --local_resources=cpu=2 --local_resources=memory=2048)
nix develop --command bazel --host_jvm_args=-Xmx1536m test //tools/research_askama:native_export_test "${limits[@]}" --test_output=errors
nix develop --command bazel --host_jvm_args=-Xmx1536m test //tools/research_askama:adapter_collection_test "${limits[@]}" --nocache_test_results --test_output=errors
```

The native outputs appear under `bazel-testlogs/tools/research_askama/native_export_test/test.outputs`.
The adapter outputs appear under `bazel-testlogs/tools/research_askama/adapter_collection_test/test.outputs`.
The latter directory includes native and template coverage.
Its `collected` directory contains both merge orders and combined native/template LCOV.
`controls.txt` records actual collector failures after successful Rust assertions.
The transformed templates are build outputs.
The original templates remain unchanged during instrumentation.
