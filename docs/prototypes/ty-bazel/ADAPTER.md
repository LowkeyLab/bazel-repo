# Disposable ty adapter feasibility follow-up

This continues [Validate ty through Bazel and Aspect lint](https://github.com/LowkeyLab/bazel-repo/issues/2326) after the maintainer approved the revised adapter boundary and sending prototype inputs and derived artifacts to `remote.buildbuddy.io` in the live decision session. The approval is [recorded on the ticket](https://github.com/LowkeyLab/bazel-repo/issues/2326#issuecomment-6227995296).

The branch is throwaway. No PR or production annotation migration belongs to this experiment. The original [feasibility report](README.md) describes the earlier, unadapted integration. This report describes the follow-up.

Base prototype commit: `dd40aeadba3121de5631f0938cbd874dc56ab086`, originally based on `de1b8f8507d476d9c44ef608a1c298bef1b6c055`. The checker remains checksum-pinned multitool ty **0.0.85**. No result here claims execution of 0.0.89.

## Changes exercised

- [rules_lint patch](../../../tools/lint/prototype_ty.patch): add the current target's `PyInfo` imports and transitive sources; read the repository's configured Python selector through `BuildSettingInfo`; pass that value to both ty actions. The same disposable patch includes explicit sandbox witnesses for the fixture's wheel typing metadata and stub file. These fixture-specific assertions are instrumentation, not a proposed production interface.
- The rules_lint patch also adds one warning-header pattern to the SARIF converter. Without it, a warning-only checker failure can produce an empty machine report. The patch preserves warning severity; it does not choose the eventual strictness policy.
- [Aspect CLI patch](aspect-ty-unconditional.patch): use unfiltered ty diagnostics in the normal accumulated reports and fail on ty error diagnostics or a nonzero ty exit. Other linters retain the selected strategy. This prototype deliberately makes ty unconditional even under `--strategy=soft`.
- [Runner](aspect.sh): initialize a private `ASPECT_CLI_CACHE`, patch the extracted AXL builtins, and invoke the pinned installed CLI. This exercises the changed AXL with the existing binary. It does **not** prove that a rebuilt Nix package embeds the patch; production packaging remains implementation work. Shared installed tooling and shared runtime caches are untouched.
- A disposable Bazel executable copies the pinned checker and appends inert bytes to its ELF image. It exists only to prove checker-content invalidation; it is not a checker upgrade.

## Evidence

Paths below are relative to `evidence/adapter/`.

| Probe                                | Executed observation                                                                                                                                                                                                                               | Evidence                                                                                                                                                      |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Own and dependency imports           | The earlier own-import-root failure is gone. The fixture checks without findings using its own helper, the sample library, the packaging wheel, and the `.pyi` dependency.                                                                         | `own-imports.log`, `clean-imports.log`                                                                                                                        |
| Wheel typing metadata                | Both sandboxed checker actions assert that the unpacked wheel contains `packaging/py.typed` and distribution `METADATA`, and that the stub file is present.                                                                                        | `metadata-sandbox.log`                                                                                                                                        |
| Python selector analysis             | Selecting 3.15 versus 3.14 changes both checker argument lists and both action keys. This row is action-analysis evidence.                                                                                                                         | `selector-315.json`, `selector-314.json`                                                                                                                      |
| Python selector execution            | The same source using a lazy import and `typing.TypeForm` passes under 3.15 and fails under 3.14 with both an unavailable symbol and unsupported-syntax diagnostic.                                                                                | `version-source.txt`, `version-315.log`, `version-314.report.json`                                                                                            |
| Unconditional ty enforcement         | Default, hard, and soft strategies each return 1 and report all ten existing baseline findings, including duplicates through multiple targets.                                                                                                     | `patched-baseline.log`, `patched-baseline-hard.log`, `patched-baseline-soft.log`                                                                              |
| Other linter strategy                | The same deliberate Ruff unused-import finding returns 0 under soft and 1 under hard with the patched CLI. The temporary import was removed.                                                                                                       | `ruff-soft.log`, `ruff-hard.log`                                                                                                                              |
| Library, binary, test, hidden script | Deliberate invalid assignments in all four target kinds appear in normal reports and fail lint.                                                                                                                                                    | `injected-four-kinds.diff`, `injected-four-kinds.log`                                                                                                         |
| Source invalidation                  | Adding an invalid assignment changes the consuming report and returns 1.                                                                                                                                                                           | `mutation-source.report.json`                                                                                                                                 |
| Dependency invalidation              | Changing an imported function's return annotation produces the consuming fixture's incompatible return diagnostic.                                                                                                                                 | `mutation-dependency.report.json`                                                                                                                             |
| Stub invalidation                    | Changing the stub's return type produces an incompatible assignment in the consumer.                                                                                                                                                               | `mutation-stub.report.json`                                                                                                                                   |
| Root configuration invalidation      | Changing the root diagnostic setting from error to warning changes the raw diagnostic. The original parser drops it; the patched parser retains it as a warning in SARIF and normal reports. ty 0.0.85 still returns 1 in this warning-only probe. | `config-warning-repeat.raw.txt`, `config-warning-repeat.report.json`, `warning-parser.raw.txt`, `warning-parser.report.json`, `warning-parser.checker-status` |
| Checker binary invalidation          | Two checker contents have different SHA256 digests; both corresponding human/machine checks execute in `linux-sandbox`, with `cacheHit=false` and exit 0.                                                                                          | `binary-E.sha256`, `binary-F.sha256`, `binary-E-ty-spawns.json`, `binary-F-ty-spawns.json`                                                                    |

The source/dependency/stub/configuration mutations were restored. No production Python file is annotated or changed by this follow-up. The final own-root fixture may retain a harmless annotated value used to force a fresh remote check.

The earlier future-target discovery and exclusion audit remain in the original report and evidence: Gazelle-generated targets over authored sources were discovered without a target allowlist, including a hidden-directory package. This differs from generated Python source files, which rules_lint skips unless `lint-genfiles` is set. Existing first-party Python targets have no `no-lint` tags. `--force-exclude` remains active, so future exclusions and `no-lint` escape hatches require an explicit coverage policy and acceptance guard.

## Reproduction

Enter the development shell with `nix develop`. The runner defaults to a new private runtime cache; set `TY_PROTOTYPE_ASPECT_CACHE` to a task-owned path to reuse it.

```sh
docs/prototypes/ty-bazel/aspect.sh lint \
  --aspect=//tools/lint:linters.bzl%ty \
  --remote_executor= \
  --bazel-flag=--strategy=AspectRulesLintTy=sandboxed \
  //python_sample/ty_prototype:ty_prototype
```

The same command over all first-party Python intentionally fails on the existing annotation baseline. It is not a green migration result.

For JSON execution evidence with Bazel 9.3, pass `--bazel-flag=--execution_log_compact_file=` before `--bazel-flag=--execution_log_json_file=/absolute/path`. The formats are mutually exclusive. The pinned [Bazel 9.3 option source](https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/exec/ExecutionOptions.java) uses an empty-to-null path converter: the string `false` is not the disabling syntax for that release.

The early binary/remote log-capture attempts retained both formats or used `false`; those attempts returned a build failure despite completed checker actions. They are preserved as failed attempts and do not establish a passing gate. The `binary-E`/`binary-F` and final verified remote records use the corrected empty-path override. The first E/F attempts failed during application of the multi-file patch; their distinct `patch-header-attempt` logs are also retained. Complete large execution logs are retained privately in the pool receipt directory; the branch retains focused ty action records with relevant input digests, runner, exit code, and cache-hit status. Remote records select first-party inputs, root policy, the checker, and wheel typing metadata; complete input records remain in the private logs.

## Configured CI remote execution

The final remote probes use the repository's `ci` and `ci-lint` configurations, downloaded lint reports, the remote GNU/LLVM execution platform, the selected Python 3.15 interpreter, `--strategy=AspectRulesLintTy=remote`, and `--noremote_local_fallback`.

- [Deliberate-error invocation](https://app.buildbuddy.io/invocation/ea9a27a9-ef7d-487f-bc24-2122dece10c6): all eight human/machine ty actions for the library, binary, pytest test, and hidden script have `runner=remote` and `cacheHit=false`. The Bazel wrapper actions complete successfully, ty exit codes travel through normal lint output files, four error findings appear, and the lint command exits 1. See `remote-injected-verified.log`, its `.diff`, and `remote-injected-verified-ty-spawns.json`.
- [Clean invocation](https://app.buildbuddy.io/invocation/7ebf4b4f-b585-4a80-8987-b9bfa57319c0): the sample targets and hidden future-target fixture pass with exit 0. Both own-root/typed-wheel/stub fixture actions have `runner=remote`, `cacheHit=false`, and exit 0, including successful metadata assertions. See `remote-clean-verified.log` and `remote-clean-verified-ty-spawns.json`.

These are executed checks from the local session using the configured CI remote path. They are not a hosted GitHub Actions run or a claim that the current annotation baseline passes repository-wide CI. Credential-bearing rc header values are redacted from published evidence.

## Review boundary

The experiment supports retaining ty and multitool provisioning with the provider/version, selective CLI-enforcement, and warning-reporting patches. It does not establish a strict preset or annotation completeness. Missing annotations and explicit Any remained silent under the earlier representative probes. Exact diagnostic severity, annotation rules, unknown handling, exclusions, migration scope, and permitted exceptions still belong to [Define strict Python checking policy and implementation acceptance](https://github.com/LowkeyLab/bazel-repo/issues/2323).

The earlier Ruff/Python selector mismatch is still relevant: a temporary lazy-import probe accepted by ty under 3.15 was rejected by Ruff inferring 3.14 from project metadata. Production acceptance must align syntax-sensitive tooling with the selected interpreter and verify compatibility when it changes. That observation is not a reason to weaken the supported Python policy.

Human review must accept the prototype evidence before its ticket is resolved. This report does not close the ticket or unblock the strictness decision on its own.

## Repository checks

- Full repository formatting passed. The final pass includes the staged evidence and runner.
- `aspect build //...` passed with remote execution, local-result uploads, and build-event uploads disabled, using read-only BuildBuddy cache access: 1,496 remote cache hits and 18 sandboxed actions. See `final-build-cache-read-only.log`. The preceding run without cache access failed in the unmodified Python manifest generator because the local environment lacked `grep` and `cut`; see `final-build-local.log`. Automatic approval review rejected the full remote-execution build because approval covered only the prototype payload. The successful cache-read-only alternative required no broader source upload authorization.
- All six Python test targets executed and passed under the configured CI path with test-result reuse disabled. See `final-python-tests.log`. The recursive query discovers 16 Python targets, including three disposable library fixtures, without a target allowlist; see `final-python-targets.txt`.
- The new runner passes Bazel-managed ShellCheck. See `runner-shellcheck.log`.
- Full lint is **not green**. The existing aspect list fails Java runtime toolchain resolution on the WASM target. The same failure reproduces with ty removed from the complete aspect list; see `full-lint-without-ty.log`. Checkstyle alone produces no lint output for the Rust target and does not reproduce the combined-aspect failure.
- Full lint with keep-going also reports ten existing ty errors and advisory prose findings in the prototype reports. The complete Python lint run reports exactly those ten ty errors. See `final-all-lint-keep-going.log` and `final-python-lint.log`. The experiment intentionally leaves existing annotations for the later migration.

These checks support the bounded integration experiment. They do not establish a passing hosted CI run or production readiness. Final implementation acceptance must include a green complete lint path, the annotation migration, and packaged adapter delivery.
