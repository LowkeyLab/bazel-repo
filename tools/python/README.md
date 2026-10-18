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
directory. The configured remote profile can use cached test results.
