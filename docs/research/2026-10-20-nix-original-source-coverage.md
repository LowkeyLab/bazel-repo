# Nix original-source coverage and lazy evaluation

A fixed evaluator and a bounded parser-backed adapter produced original-source expression-entry coverage for the two research fixtures.
The instrumented result remained `6`. The lazy poison expression and untaken branch remained unforced.
The shared addition evaluated once although the requested result used its binding twice.
This establishes a candidate for the tested grammar. It does not establish a complete collector for maintained Nix programs.

This report addresses [Determine Nix original-source coverage across lazy evaluation](https://github.com/LowkeyLab/bazel-repo/issues/2455).
It extends the [remaining-source investigation](https://github.com/LowkeyLab/bazel-repo/blob/0dd2c8293fe0375af1377c93996bf848dcd3ff80/docs/research/2026-10-19-remaining-source-coverage.md).
The refreshed base is `5e246a47cd7ea0a9c506e915db0c053d7d84d023`.
No production implementation, final collector mechanism, or coverage exception follows from these results.

## Evaluator and source model

The experiment fixes Nix `2.34.9` at `/nix/store/9jw399l4lllgai8p7fd4y61lqhi8qzja-nix-2.34.9`.
The executable SHA-256 is `cac9e1cdced503201d9d175b48732cd29b027b066398a358ac98159c0c9128b1`.
The harness validates that hash and effective version before evaluation.
This is an explicit research pin. The [repository flake](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/flake.nix) does not pin the caller's Nix executable.

The [harness](../../tools/research_nix/experiment.py) parses a restricted expression grammar independently of evaluator events.
Its inventory records node kinds, original offsets, lines, columns, and child relationships.
The grammar covers the fixture's bindings, attribute sets, identifiers, literals, addition, conditionals, parentheses, and `throw` application.
It rejects unsupported syntax instead of supplying an empty baseline.
The fixtures contain ASCII source. General Unicode offsets and full Nix syntax remain outside the demonstrated parser scope.

The adapter wraps each parsed expression with a uniquely identified `builtins.trace` call.
A demanded expression emits its own marker. Inventory construction does not evaluate expressions.
The line model uses expression start lines. Multiple expressions on one line use the largest observed entry count for that line.
An `if` has two branch records, one for each arm's root expression.
These dimensions do not establish condition independence, short-circuit coverage, or every line in a multiline expression span.

The [Nix evaluation manual](https://nix.dev/manual/nix/2.34/language/evaluation.html) describes call-by-need evaluation and reuse of values in bindings.
It also leaves trace ordering unspecified. The adapter therefore aggregates location counts without requiring a particular trace order.
The [trace builtin](https://nix.dev/manual/nix/2.34/language/builtins.html#builtins-trace) supplies the diagnostic marker while returning the wrapped expression.
Runtime comparisons establish laziness only for the selected demands and fixtures.

## Executed results

| Case                       | Observation                                                                                                   | Evidence limit                                                     |
| -------------------------- | ------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Native function tracing    | The original result is `6`. Events identify the outer import call rather than the fixture's arithmetic lines. | No exhaustive expression-line report follows.                      |
| Native call statistics     | `nrFunctionCalls` is zero and the function list is empty.                                                     | Aggregate counters are not executable-line coverage.               |
| Instrumented result        | Output remains `6`. Original expression lines receive positive counts.                                        | The restricted grammar defines the line model.                     |
| Lazy poison                | Original line 8 retains zero counts and raises no exception.                                                  | The adapter does not demand this expression for coverage.          |
| Untaken arm                | Original line 6 and the untaken branch retain zero counts.                                                    | The fixture demonstrates one conditional's arm coverage.           |
| Shared binding             | The addition on line 4 emits once for two uses of its binding.                                                | This demonstrates within-process memoization for this fixture.     |
| Repeated evaluation        | A fresh evaluator process emits identical location counts.                                                    | Persistent flake-evaluation and derivation caches were not tested. |
| Disconnected source        | Parsed executable locations on lines 1, 2, and 4 remain zero.                                                 | The parser inventories the file without evaluation.                |
| Actual builder demand      | Requesting the script records string creation at original line 9.                                             | This is Nix evaluation, not shell execution.                       |
| Combined expression report | Result and builder-demand counts combine with the zero baseline. Existing measured hits remain positive.      | This is fixture LCOV, not the repository's global coverage report. |
| Missing or empty capture   | Collection fails while the preserved functional output remains `6`.                                           | The adapter supplies this validation.                              |
| Unknown location           | A corrupt event fails despite a matching capture digest.                                                      | Event identities must belong to the independent inventory.         |
| Receipt-only capture       | Collection fails because the demanded root expression is absent.                                              | A marker alone cannot establish collection.                        |
| Truncated capture          | Collection fails against the preserved capture digest.                                                        | Stream integrity does not prove general evaluator completeness.    |

The capture manifest records evaluator exit status, stream length, and SHA-256.
The collector also requires a receipt marker, a demanded root event, and known source locations.
The negative cases preserve successful functional output while removing or corrupting collection evidence.
These validations belong to the temporary adapter. Native tracing supplies no equivalent completeness contract in this experiment.

## Builder shell mapping

The script is a separate, actual demand. The harness requests its JSON string value and decodes that value before execution.
This demand does not evaluate the lazy poison expression or the arithmetic binding.
The extracted script removes four indentation characters without inserting or removing source lines.
The harness compares every extracted line with its original string-content line.

| Extracted shell line | Original Nix line | Observed shell operation                                         |
| -------------------- | ----------------- | ---------------------------------------------------------------- |
| 1                    | 10                | `printf` executes.                                               |
| 2                    | 11                | The literal `false` condition executes.                          |
| 3                    | 12                | The untaken `printf` does not execute.                           |
| 4                    | 13                | `fi` closes the conditional. It is not a separate statement hit. |

Bash xtrace supplies these narrow execution observations. The artifact records the effective Bash version, executable path, and SHA-256.
The shell trace remains separate from Nix expression events and LCOV.
The mapping demonstrates only this fixed string without interpolation or indented-string escapes.
A general adapter must represent multiple source segments or reject ambiguous expansions.
A constant offset cannot establish arbitrary multiline interpolation mappings.

[Determine shell and embedded-shell original-source coverage](https://github.com/LowkeyLab/bazel-repo/issues/2453) investigates the separate shell collector and executable baseline.
This report does not infer complete shell statement or branch coverage from Nix events, xtrace, or successful script output.

## Candidate mechanism and remaining observations

A full Nix parser plus explicit expression-entry instrumentation is a candidate.
It must preserve binding laziness and memoization under the actual evaluation demands.
It must supply an independent baseline and original-source identity mapping.
The bounded adapter demonstrates those properties for the selected grammar and fixtures.
It does not justify applying this parser to arbitrary maintained Nix files.

General feasibility still needs full syntax, imports, functions, recursion, string interpolation, path context, and source-location-sensitive behavior.
It also needs evidence under production evaluator configuration and persistent evaluation/build caches.
The fixed host evaluator closure is not provisioned on configured remote workers.
No remote evaluator execution or hosted upload observation follows from the configured Bazel services.
These are precise missing observations. They approve no exception and do not classify untested constructs as unsupported.

## Execution boundaries

The target explicitly requires local execution because it uses the fixed host evaluator closure.
The configured remote test filter initially excluded it. That attempt ran no evaluator.
The named-target command clears that filter while retaining the local execution requirement.
The final probe ran in a local Linux sandbox. Configured remote dependency services do not turn that action into remote evaluation.

The first temporary wrapper failed parsing because a compound trace argument needed parentheses.
The revised wrapper passed the original-value and laziness comparisons.
An initial new-CLI string extraction also rejected an absolute fixture import in pure mode.
The final harness uses JSON output from the same `nix-instantiate --eval --strict` evaluator path as the successful expression probes.
Preserved failed probes explain these adapter revisions. They do not establish native evaluator limitations.

## Preserved artifacts and validation

The [evidence archive](nix-original-source-coverage/evidence.tar.gz) preserves original and instrumented sources, inventories, evaluator streams, capture manifests, command arguments, reports, and failed probes.
The [archive manifest](nix-original-source-coverage/archive-manifest.json) hashes each published member.
Publication replaces machine paths with `<HOME>`, `<BAZEL_OUTPUT_BASE>`, and `<NIX_TMPDIR>`.
Capture sidecars describe the original evaluator bytes. Their hashes apply to private originals when publication changes a path.
Private runtime storage retains those original bytes without deletion.

The [inventory](nix-original-source-coverage/inventory.json) supplies independent expression locations.
The [zero baseline](nix-original-source-coverage/baseline.lcov), [result report](nix-original-source-coverage/measured.lcov), and [builder-demand report](nix-original-source-coverage/builder-evaluation.lcov) preserve the intermediate observations.
The [combined report](nix-original-source-coverage/combined.lcov) includes both actual demands and disconnected zero counts.
The [negative cases](nix-original-source-coverage/negative-cases.json) preserve each collection failure alongside successful functional output.
The [builder map](nix-original-source-coverage/builder-map.json) records the separate shell offsets.

The effective builder shell is Bash `5.3.15` for x86-64 Linux.
Its [pin record](nix-original-source-coverage/bash-pin.json) preserves the resolved store executable and SHA-256.
The [command record](nix-original-source-coverage/commands.txt) preserves resource limits and the cleared test filter.
Bazel is `9.3.0`. Each invocation uses a 1536 MB JVM heap limit and local CPU/memory limits of two units and 2048 MB.
Eight jobs permit configured remote dependency work. They do not change the evaluator's local execution requirement.

| Operation           | Result                                                                                                               | Scope                                                                                                           |
| ------------------- | -------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Final fixture test  | [Fresh invocation](https://app.buildbuddy.io/invocation/8504420c-306d-43b4-b70f-60f4be4329e6) passes in 0.6 seconds. | One fresh local test. The log records four action-cache hits and two Linux-sandbox actions.                     |
| Targeted lint       | [Invocation](https://app.buildbuddy.io/invocation/a6907c81-2171-446b-aa49-aa913582b181) passes.                      | Ruff, Ty, and KeepSorted report no findings.                                                                    |
| Full formatting     | Full-scope formatting passes.                                                                                        | No repository-wide production build or lint result follows.                                                     |
| Artifact validation | Every archive member matches its recorded hash.                                                                      | Publication preserves source identities and exact observation content apart from documented path substitutions. |
| Strict English      | Structural validation reports zero findings.                                                                         | Technical literals retain their original spelling.                                                              |

No configured-remote evaluator test, persistent-cache acceptance, hosted CI, global LCOV integration, or Codecov upload ran.
