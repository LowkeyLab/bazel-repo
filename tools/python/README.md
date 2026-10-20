# Python coverage

The shared Python wrappers retain `aspect_rules_py` and the hermetic interpreter.
Gazelle uses these wrappers for libraries, binaries, and tests.

Run the existing combined report command:

```bash
nix develop --command bazel run //tools/coverage -- //...
```

Coverage mode also selects production Python targets through a shared wrapper tag.
Bazel retains the existing test selection for all languages. Each eligible Python rule declares a
baseline action with its production sources as inputs. The real collector emits
zero-hit line and branch records. Bazel merges these records with test execution.
Sources outside test dependencies therefore remain in the denominator.

The rules exclude test-only sources, generated sources, and external dependencies.
Each eligible source also travels through Python coverage metadata.
Metadata targets use the `.python_coverage` suffix in the instrumentation filter.
The existing language boundaries remain in place. Python discovery requires no directory list.
Baseline failures fail the coverage command. Ordinary test runs select no baseline
tool. Existing aggregation and Codecov policy remain in use.

## Acceptance checks

Run the collector command tests:

```bash
nix develop --command aspect test //tools/python:coverage_baseline_test
```

Run the complete membership test from a clean, assigned checkout:

```bash
nix develop --command bazel run //tools/python:coverage_membership_test -- /absolute/evidence/directory
```

The manual test creates temporary ordinary and hidden packages. It runs Gazelle
after each source addition. The ordinary package is outside the original instrumentation paths. The test checks imported execution, zero-hit sources, exclusions, and
merged records through `bazel coverage`. The production module with zero hits remains outside every test dependency.

The test then introduces invalid syntax and requires a baseline failure. It removes
its temporary packages. The report and logs remain in the supplied evidence
directory.

The command defaults to local sandbox execution. It disables remote execution,
remote cache access, and the build-event service for its nested commands.
Each test executes again. The report uses the existing coverage helper.
A Go test checks that aggregation preserves another language.

Use the configured remote service for fresh execution:

```bash
nix develop --command bazel run //tools/python:coverage_membership_test -- /absolute/evidence/directory --execution remote
```

The remote mode uses `--config=ci`, `--nocache_test_results`, and
`--remote_accept_cached=false`. It requires remote execution metadata for every
selected test. Fresh remote acceptance requires executed tests.
Bazel supplies the interpreter, collector, production sources, and subprocess
inputs through the declared targets.

Both modes retain `coverage.log`, `coverage.exit`, `coverage.bep.json`,
`test-results.json`, and `combined.lcov`. The build events identify test execution
strategies and cache status. Keep local and remote evidence in separate directories.
Hosted workflow and Codecov upload acceptance require separate evidence.

The command also checks four coverage failures with real collector processes:
missing collector dependencies, child collection errors, corrupt subprocess data,
and report exclusion errors. Each underlying ordinary test must pass first.
The corresponding coverage command must return a nonzero exit status and the
expected error. The invalid-source case requires a baseline failure.

The evidence directory also retains `commands.jsonl`, `revision.txt`,
`working-tree.patch`, dependency-query results, and failure logs with exit status.
Run acceptance alone in the assigned checkout. Temporary invalid sources can
cause concurrent formatting, lint, or build commands to fail.
