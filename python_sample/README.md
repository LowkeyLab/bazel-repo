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

Renovate maintains the selected stable Python minor in `MODULE.bazel` and
the matching toolchain selection in `.bazelrc` together with the standalone
interpreter release date. Interpreter updates refresh build metadata and must pass
CI. The runtime test compares the running interpreter with `MODULE.bazel`, so
future minor upgrades exercise the same acceptance checks.

## Coverage for imported modules

The shared test wrapper supplies `coverage.py` through the Bazel dependency hub.
The pinned runner patch enables branch collection and fails if the collector is
unavailable. Collector and report errors propagate to the coverage command.

Run coverage for imported sample and CI modules:

```shell
bazel coverage //python_sample/... //.github/scripts:all
```

Read the combined report at `bazel-out/_coverage/_coverage_report.dat`.
`SF:` records identify source files. `DA:` records contain line hits.
`BRDA:` records contain branch hits. The existing coverage helper and hosted
workflow consume the same report.

This stage covers imports. Subprocess collection, complete production-source
membership, and final hosted upload checks remain in the dependent tickets
under [#2391](https://github.com/LowkeyLab/bazel-repo/issues/2391).

For a collector failure test, use this command:

```shell
bazel coverage //python_sample:versions_test --nocache_test_results --test_env=COVERAGE_CORE=invalid
```

The coverage command must fail. An ordinary `bazel test` invocation with the
same environment must pass. For a report failure test, supply a coverage
configuration with `[report]` and `omit = *` through `COVERAGE_RCFILE`.
Run that command locally so the test can read the configuration file.
The tests must pass before report generation fails with `NoDataError`.
