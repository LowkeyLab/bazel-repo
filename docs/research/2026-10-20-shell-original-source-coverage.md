# Shell and embedded-shell original-source coverage

This report answers [Determine shell and embedded-shell original-source coverage](https://github.com/LowkeyLab/bazel-repo/issues/2453).
The repository base is `5e246a47cd7ea0a9c506e915db0c053d7d84d023`.
The experiment uses kcov revision `39bda2f4f6fe04529c3479c6494acb6bc78a0857` and GNU Bash `5.3.15`.

The pinned collector produces useful original-line observations for the requested bounded fixture.
It records the sourced helper, child-only execution, disconnected zero-hit lines, and the two lines of the multiline condition.
The temporary adapter maps two extracted programs to their original container lines.
Native kcov does not reject the demonstrated loss of child tracing.
The complete automatic-inclusion contract remains unproven.
This report approves no exception or production mechanism.

## Inputs and execution

The [Bazel package](shell-2453/BUILD.bazel) builds the exact [collector archive](shell-2453/kcov.tar.gz) through a local genrule.
The [dependency environment](shell-2453/build-deps.nix) uses Nixpkgs revision `f20cd90e28fe0403873f473b29e42662cd86e8c4` from the repository lock.
It supplies CMake, GCC, pkg-config, elfutils, curl, OpenSSL, and zlib.
The [input manifest](shell-2453/evidence/inputs.json) preserves source and archive hashes.

Bazel controls compilation. CMake uses two compiler jobs inside that action.
The action uses immutable Nix dependencies through explicit environment variables.
Those dependency paths are not declared as portable remote action inputs.
This establishes a pinned local experiment, not a fully hermetic remote collector package.

The [probe](shell-2453/probe.py) executes the collector locally through `bazel run`.
Its [declared fixtures](shell-2453/fixtures/main.sh) include a sourced helper, a child, and a disconnected script.
A separate no-op driver collects a zero baseline without executing those subject commands.
The native directory parser discovers their candidate executable lines.
The baseline retains real line records rather than filename-only placeholders.

The workflow fixture preserves the original install command from `.github/workflows/coverage.yml:47`.
An exported shell function replaces the external install operation.
The launcher fixture preserves shell statements from `tools/topcoat/format.bzl:11–13`.
A controlled executable replaces the formatter tool.
These controls test shell execution and source attribution. They do not test installation or formatter behavior.

## Known source locations

The [native regular report](shell-2453/evidence/raw/regular.xml) and [native zero baseline](shell-2453/evidence/raw/baseline.xml) preserve the following observations.
The source paths below refer to the four declared fixture files unless a container path appears.
Mapped records cover only the tested fragments. They do not establish completeness for every program in either container file.

| Source location                     | Known role                                       | Native observation                                                               |
| ----------------------------------- | ------------------------------------------------ | -------------------------------------------------------------------------------- |
| `main.sh:1`                         | Shebang text                                     | No line record.                                                                  |
| `main.sh:2–3`                       | Executed option and source commands              | Positive records. Line 3 counts two trace events.                                |
| `main.sh:4–5`                       | Two executed condition commands across two lines | Both lines have positive records.                                                |
| `main.sh:6`                         | Selected print command                           | Positive record.                                                                 |
| `main.sh:8`                         | Untaken print command                            | Zero-hit record.                                                                 |
| `main.sh:7,9`                       | `else` and `fi` syntax                           | No line records.                                                                 |
| `helper.sh:2`                       | Sourced assignment                               | Positive record.                                                                 |
| `helper.sh:4`                       | Uncalled function body                           | Zero-hit record.                                                                 |
| `helper.sh:3`                       | Function declaration header                      | Omitted. This observation does not define a general declaration-location policy. |
| `helper.sh:5`                       | Closing brace                                    | No line record.                                                                  |
| `child.sh:2`                        | Child-only print command                         | Positive record.                                                                 |
| `disconnected.sh:2–4`               | Unexecuted print, condition, and body commands   | Real zero-hit records.                                                           |
| `disconnected.sh:1,5`               | Shebang and `fi` syntax                          | No line records.                                                                 |
| `.github/workflows/coverage.yml:47` | Extracted shell command                          | Positive record after explicit mapping.                                          |
| `tools/topcoat/format.bzl:11–13`    | Extracted option, assignment, and exec commands  | Positive records after explicit mapping.                                         |

Counts represent trace events at a source line.
A command substitution can add another event at the same line.
The experiment uses positive versus zero observations. It does not establish exact logical statement execution counts.

## Capability matrix

| Capability                      | Intrinsic kcov result                                                                      | Temporary adapter result or limit                                                                                             |
| ------------------------------- | ------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| Executed and disconnected lines | The fixture produces expected positive and zero-hit records.                               | The adapter retains those records in canonical LCOV. General grammar completeness remains unproven.                           |
| Sourced helper                  | The assignment receives a hit and the uncalled body remains zero.                          | The declared input association supplies its repository-relative identity.                                                     |
| Child execution                 | The ordinary child print receives a hit.                                                   | The experiment also requires a separate child report for artifact validation.                                                 |
| Child tracing loss              | `set +x` permits functional success while the next child print remains zero. kcov exits 0. | Missing-artifact checks do not detect every valid but incomplete report. A stronger completeness mechanism remains necessary. |
| Branch outcomes                 | The native reports supply line hits, not condition or branch outcomes.                     | The converter emits no invented `BRDA` records. This candidate establishes no branch dimension.                               |
| Workflow environment expansion  | The extracted command executes with a controlled environment value.                        | Extracted line 2 maps to original workflow line 47. GitHub expression insertion inside a script remains untested.             |
| Generated launcher              | Three shell statement lines execute with controlled tool paths.                            | Fixed brace and path substitutions preserve line identities. General multiline interpolation remains unproven.                |
| Missing child XML               | The collector already completed successfully before deletion.                              | Reading the required report raises an error. The probe catches that expected error.                                           |
| Corrupt child XML               | The collector already completed successfully before corruption.                            | XML parsing raises an error. The probe catches that expected error separately.                                                |
| Baseline merging                | The native baseline contains zero-hit executable candidates.                               | Both pinned LCOV merger orders preserve the measured hits and canonical identities.                                           |
| Portable hermetic execution     | Not established by this local Nix-dependent action.                                        | A declared dependency closure and fresh remote collector execution remain necessary evidence.                                 |

See the [observations](shell-2453/evidence/observations.json), [source mappings](shell-2453/evidence/source-maps.json), and [merge checks](shell-2453/evidence/merge-checks.json).
The missing and corrupt report checks belong to the adapter.
They are not intrinsic kcov failure behavior.
The validation probe expects and catches each error, so its successful exit means those controls behaved as required.

## Parser and branch limits

The pinned [Bash engine](https://github.com/SimonKagstrom/kcov/blob/39bda2f4f6fe04529c3479c6494acb6bc78a0857/src/engines/bash-engine.cc) discovers candidate lines through `parseFileFull` or `parseFileBasic`.
These functions inspect source lines and maintain lexical states. They do not consume a complete Bash syntax tree.
The known-line comparison above passes for this fixture.
It does not prove exact executable-location discovery for arbitrary maintained Bash syntax.

The [trace helper](https://github.com/SimonKagstrom/kcov/blob/39bda2f4f6fe04529c3479c6494acb6bc78a0857/src/engines/bash-helper.sh) enables xtrace and places source/line information in its prompt.
The trace-loss control demonstrates that a child can disable that signal while completing its functional work.
The [Cobertura writer](https://github.com/SimonKagstrom/kcov/blob/39bda2f4f6fe04529c3479c6494acb6bc78a0857/src/writers/cobertura-writer.cc) exports the collected line observations.
The native XML reports constant branch-rate fields of `1.0` without condition outcomes. Those summary fields are not branch-outcome evidence.
This is a limit of the investigated candidate, not proof that every Bash collector lacks branch support.

## Other embedded containers

The [remaining-source inventory](https://github.com/LowkeyLab/bazel-repo/blob/0dd2c8293fe0375af1377c93996bf848dcd3ff80/docs/research/2026-10-19-remaining-source-coverage.md) identified the following boundaries.
This check classifies their extraction requirements. It does not claim new runtime coverage for them.

| Container                                             | Required treatment                                                                                                                                 |
| ----------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Workflow and composite-action `run` blocks            | Extract shell ranges with original paths and offsets. Separate expression insertion from runtime environment expansion.                            |
| Bazel `cmd` strings and maintained launcher templates | Preserve maintained statement origins through label and template substitution. Keep generated output identities separate.                          |
| Nix builder/check strings                             | Preserve string segment origins through evaluation and indentation removal. Nix expression events do not establish shell execution.                |
| Dockerfile `RUN` instructions                         | Check the effective interpreter. The current Dockerfile also invokes `sh`; Bash substitution does not automatically preserve that behavior.        |
| `.envrc` and `githooks/pre-commit`                    | Treat their shell programs as direct source inputs. Preserve the hook's quoted child program as its own mapped range.                              |
| Package `scripts` and Renovate command fields         | Identify the configured execution shell and map only executable command strings. Dependency selectors and other metadata are not shell statements. |
| `.pre-commit-config.yaml` and `.vscode/tasks.json`    | The current values declare tool invocations. A command plus an argument list does not by itself create a maintained shell program.                 |
| Setup-action bazelrc values                           | Treat them as Bazel command configuration, not Bash statements.                                                                                    |

A constant line offset works for the two tested extractions.
Inserted multiline source needs segment origins. Ambiguous origins must not become invented original-source hits.
No general interpolation mechanism follows from these two fixed substitutions.

## Candidate mechanisms and remaining evidence

Native kcov supplies useful Bash line observations and disconnected line candidates.
The tested adapter adds explicit input identity, container offsets, expected child artifact checks, LCOV conversion, and baseline merging.
Those primitives do not establish the complete required collector.

A bounded continuation can reuse these sources to compare parser-derived executable locations with the native candidates.
It must also test an independent way to detect truncated but syntactically valid child reports.
A broader container adapter needs source segments through interpolation and the actual interpreter at each boundary.
Branch instrumentation and a portable dependency closure remain separate unproven capabilities.

The later [collector mechanism decision](https://github.com/LowkeyLab/bazel-repo/issues/2445) retains the human choice of mechanisms and exceptions.
The accepted membership policy, Python architecture, combined LCOV workflow, thresholds, and Codecov policy remain unchanged.

## Validation and reproduction

[Reproduction commands](shell-2453/REPRODUCE.md) preserve the bounded build, run, and merge configuration.
The local collector build and fresh local probe ran under Bazel with the repository-configured BuildBuddy services enabled.
Collector compilation used a local action. Collector execution also ran locally.
Remote configuration or cached tools do not establish fresh remote collection.
No hosted workflow execution or Codecov upload forms part of this evidence.

Gazelle, whole-tree formatting, the local collector build, the final probe, both merge orders, and focused probe lint passed.
The final lint run used KeepSorted, Ruff, and Ty and reported no findings.
The [validation record](shell-2453/evidence/validation.json) separates their statuses.
The [Bazel probe invocation](https://app.buildbuddy.io/invocation/2873b4a6-cdb9-4cfa-8f84-d757aa05c78f) records the final local run.
It is not evidence of remote collector execution.

The research branch contains no production change and requires no merge.
