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

## Verified execution on 2026-10-20

The acceptance revision is `eef18dd1c2887c644ae5fd7b6126e4767b360c71`. Both execution modes passed.
The main coverage command returned zero in each mode.
Each combined report contains 15 source records.

| Check                                                                 | Local           | Configured remote |
| --------------------------------------------------------------------- | --------------- | ----------------- |
| Selected tests                                                        | 12              | 12                |
| Execution strategy                                                    | `linux-sandbox` | `remote`          |
| Cached test results                                                   | 0               | 0                 |
| Collection, combination, report, and dependency failure exit statuses | 3               | 3                 |
| Invalid-source coverage exit status                                   | 1               | 1                 |

Both reports include imported execution, generated binaries, and child interpreters.
Ordinary and hidden packages include disconnected sources with zero hits.
The Go production file retains 11 executed lines from 13 executable lines.
The ordinary tests pass before each deliberate collector failure.

The remote coverage invocation is
[`d752b483-b811-4bb2-9089-899097216dcd`](https://app.buildbuddy.io/invocation/d752b483-b811-4bb2-9089-899097216dcd).
The local coverage invocation is `6bc51cb7-303b-4e13-b95f-4161fa8a4ac4`.
Local acceptance disables the build-event service.

Private evidence remains under
`/home/tacascer/.local/state/worktree-pool/agent-receipts/implement-2395-acquire-35mi2MIx`.
The `local` and `remote` directories retain exact commands, revisions, combined reports,
exit statuses, and build-event metadata.
The `local-acceptance.log` and `remote-acceptance.log` files record acceptance output.

This acceptance checks Python and one Go package.
The individual failure commands retain no separate execution metadata.
Hosted workflow and Codecov acceptance remain separate work under
[issue #2396](https://github.com/LowkeyLab/bazel-repo/issues/2396).
