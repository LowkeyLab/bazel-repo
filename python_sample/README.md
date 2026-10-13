# Python reference project

This example sorts package version strings. It exercises the repository's
Python rules, local imports, third-party dependencies, command output, and
test execution.

Build and run the executable through Bazel:

```shell
bazel run //python_sample:python_sample_bin -- 10.0 2.0 1.0rc1 1.0
aspect test //python_sample/... //:gazelle_python_manifest_test
```

Declare shared Python dependencies in the root `pyproject.toml` default group.
Update the lockfile and import map before regenerating `BUILD.bazel` files:

```shell
bazel run //tools/python:lock
bazel run //:gazelle_python_manifest.update
bazel run //:gazelle
```

The lock updater uses Bazel-managed `uv` and Python. Pass `-- --upgrade` to update
all locked packages. Generated libraries and executables load from
`aspect_rules_py`. Generated tests use the shared `pytest` wrapper.
Keep the library's explicit `imports = ["."]` root so isolated execution can
resolve this project's modules without relying on the entry point's directory.

Renovate maintains the selected stable Python minor and standalone interpreter
release date together. Interpreter updates refresh build metadata and must pass
CI. The runtime test compares the running interpreter with `MODULE.bazel`, so
future minor upgrades exercise the same acceptance checks.
