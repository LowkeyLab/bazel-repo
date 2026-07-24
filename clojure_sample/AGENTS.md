# AGENTS.md (Clojure Sample)

This package demonstrates `griffinbank/rules_clojure` integration with Bazel.

## Commands

Run all commands from the repository root.

```bash
# Regenerate Clojure BUILD files after changing Clojure namespaces or deps.edn
bazel run //:clojure_gen_build_files

# Build the sample
aspect build //clojure_sample/...

# Run the sample binary
bazel run //clojure_sample:hello

# Run the sample test
bazel test //clojure_sample/test/lowkeylab/clojure_sample:core_test.test --test_output=errors
```

## Workflow

- Edit `deps.edn` for Clojure dependencies and source paths.
- Edit source files under `clojure_sample/src/` and tests under `clojure_sample/test/`.
- Do not manually edit generated `BUILD.bazel` files below `clojure_sample/src/` or `clojure_sample/test/`.
- After changing Clojure namespaces, `deps.edn`, or namespace dependencies, run `bazel run //:clojure_gen_build_files`.
- Follow repository workflow: run `bazel run //:gazelle` after source edits, then `aspect format --scope=all`, then targeted build/test checks.

## Notes

- `//clojure_sample:hello` is a Java binary that launches `clojure.main` with `lowkeylab.clojure-sample.core`.
- The sample uses generated `clojure_library` and `clojure_test` targets from `rules_clojure`.
