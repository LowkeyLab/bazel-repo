# Python coverage

The shared Python wrappers retain `aspect_rules_py` and the hermetic interpreter.
Gazelle uses these wrappers for libraries, binaries, and tests.

Run the existing combined report command:

```bash
nix develop --command bazel run //tools/coverage -- //...
```

Coverage mode selects Python baselines through automatic Bazel owner discovery.
Bazel retains the existing test selection for all languages. Each eligible Python rule declares a
baseline action with its maintained sources as inputs. The real collector emits
zero-hit line and branch records. Bazel merges these records with test execution.
Sources outside test dependencies therefore remain in the denominator.

The rules include maintained tests and executable helpers. They exclude generated
sources and external dependencies. Test-only roots select baseline metadata, so
execution filters still control which tests run.
Each eligible source also travels through Python coverage metadata.
Metadata targets use the `.python_coverage` suffix. Broad instrumentation includes
their sources.
Python discovery requires no directory list.
Baseline failures fail the coverage command. Ordinary test runs select no baseline
tool. Existing aggregation and Codecov policy remain in use.

## Acceptance checks

Run the collector command tests:

```bash
nix develop --command aspect test //tools/python:coverage_baseline_test
```

Run the complete membership test from a clean, assigned checkout.
Use a new evidence directory for each run.
The command rejects existing paths and preserves their files.

```bash
nix develop --command bazel run //tools/python:coverage_membership_test -- /absolute/evidence/directory
```

The manual test creates temporary ordinary and hidden packages. It runs Gazelle
after each source addition. The ordinary package is outside the original instrumentation paths. The test checks test and helper membership, imported execution, zero-hit sources, exclusions, and
merged records through `bazel coverage`. The production module with zero hits remains outside every test dependency.

The test then introduces invalid syntax and requires a baseline failure. It removes
its temporary packages. The report and logs remain in the supplied evidence
directory.

The command defaults to local sandbox execution. It disables remote execution,
remote cache access, and the build-event service for its nested commands.
Each test executes again. The report uses the existing coverage helper.
Endpoint overrides apply to every nested command.
The helper also applies these overrides when it queries the report location.
A Go test checks that aggregation preserves another language.

Use the configured remote service for fresh execution:

```bash
nix develop --command bazel run //tools/python:coverage_membership_test -- /absolute/evidence/directory --execution remote
```

The remote mode uses `--config=ci`, `--nocache_test_results`, and
`--remote_accept_cached=false`. It requires remote execution metadata for every
selected test. Fresh remote acceptance requires executed tests.
Bazel supplies the interpreter, collector, maintained sources, and subprocess
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

The acceptance revision is `e66c2386018423ef7aabaa255c455c2863bed485`. Both execution modes passed.
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
[`37c5fc55-a946-428c-8a5d-68f87ae1b713`](https://app.buildbuddy.io/invocation/37c5fc55-a946-428c-8a5d-68f87ae1b713).
The local coverage invocation is `38f5b792-d690-4086-8c4e-84d879cae47a`.
Local acceptance disables the build-event service.

Private evidence remains under
`/home/tacascer/.local/state/worktree-pool/agent-receipts/deliver-2395-acquire-g0t47X3n`.
The `local` and `remote` directories retain exact commands, revisions, combined reports,
exit statuses, and build-event metadata.
The `local-acceptance.log` and `remote-acceptance.log` files record acceptance output.

This acceptance checks Python and one Go package.
The individual failure commands retain no separate execution metadata.
Hosted workflow and Codecov acceptance remain separate work under
[issue #2396](https://github.com/LowkeyLab/bazel-repo/issues/2396).

## Hosted evidence

The existing Coverage workflow also runs the Python membership acceptance command.
It uses the Bazel-managed collector to measure that command and merges its real
hits with the full combined report. The original inventory and test results remain
with that report. Existing thresholds and Codecov policy remain in use.

The workflow retains a `coverage-evidence-<run>-<attempt>` artifact for 30 days.
It contains the combined report, `coverage-report.lcov`, and the Bazel test results, `coverage-test-results.jsonl`.
The result file retains test results, test summaries, and the build result.
It excludes command-line and environment details.
The `coverage-revision.txt` file identifies the tested checkout commit.
For pull requests, this commit is the GitHub test merge commit.
Compare its parents with the final pull request head.

Download the artifact from the Coverage workflow run.
Check Python `SF`, `DA`, and `BRDA` records in the report.
Check imported modules, generated binary execution, child interpreters, and zero-hit sources.
Check that Python records include maintained tests and executable helpers.
Check that generated sources and external dependencies remain excluded.
Use `testResult` events to distinguish executed tests from cached results.
The artifact can contain coverage from cached test results.

The existing Codecov step uploads only the combined `coverage-report.lcov` file.
It excludes evidence metadata from coverage-file discovery.
Check its log for the commit, upload result, and Codecov report link.
Record the workflow run, final pull request head, artifact, and upload evidence together.
Keep this hosted evidence separate from local and configured remote acceptance.

## Go report preservation

Python acceptance compares the native Go test report with both combined reports.
It requires known function-body hits and preserves every native executable record.
The selected Go SDK defines the executable-line set. Go 1.27 omits function-declaration
lines that Go 1.26 reports. Acceptance checks native counts instead of fixed totals.
Failed acceptance retains the native, combined, and processed reports in CI.
