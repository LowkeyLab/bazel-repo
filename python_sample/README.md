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

## Coverage for imports and child processes

The shared test wrapper supplies `coverage.py` through the Bazel dependency hub.
The pinned runner patch enables branch collection and fails if the collector is
unavailable. Collector and report errors propagate to the coverage command.

Run coverage for sample modules and CI modules:

```shell
bazel coverage //python_sample/... //.github/scripts:all
```

Read the combined report at `bazel-out/_coverage/_coverage_report.dat`.
`SF:` records identify source files. `DA:` records contain line hits.
`BRDA:` records contain branch hits. The existing coverage helper and hosted
workflow consume the same report.

The runner installs a coverage startup hook in each private test environment.
Generated Python launchers copy the same hook into their private environments.
Scripts that tests start through `sys.executable` use the test environment hook.
Children use the Bazel-managed collector from the parent test action.
Each child saves a separate data file and a completion record.
The runner checks completion and data files before combination and LCOV reporting.
Incomplete child collection and corrupt data fail the coverage run.

The sample CLI test measures the generated executable. The dependency preparation
test measures its script through `sys.executable`. Repeated observations use one
workspace-relative source identity. Ordinary tests do not enable the hook.
The integration checks execute the real Bazel-built test commands:

```shell
aspect test //tools/python:coverage_runtime_test
```

The checks use the same coverage environment and LCOV output as Bazel test actions.
They check child execution, branch results, collection failure, corrupt process
data, and ordinary execution. The fault fixture permits its underlying test to
pass so the checks can detect a separate coverage failure.

Children must retain the coverage environment and finish before their parent test.
Python processes that disable `site` with `-S` do not execute startup hooks.
Complete production-source membership and final hosted upload checks remain in
the dependent tickets under [#2391](https://github.com/LowkeyLab/bazel-repo/issues/2391).

For a collector failure test, use this command:

```shell
bazel coverage //python_sample:versions_test --nocache_test_results --test_env=COVERAGE_CORE=invalid
```

The coverage command must fail. An ordinary `bazel test` invocation with the
same environment must pass. For a report failure test, supply a coverage
configuration with `[report]` and `omit = *` through `COVERAGE_RCFILE`.
Run that command locally so the test can read the configuration file.
The tests must pass before report generation fails with `NoDataError`.
