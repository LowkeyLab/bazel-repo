# Disposable ty / Bazel feasibility experiment

This branch answers the Wayfinder ticket [Validate ty through Bazel and Aspect lint](https://github.com/LowkeyLab/bazel-repo/issues/2326). It is a throwaway prototype. Do not merge it or use its checker arguments as production policy.

Base: `de1b8f8507d476d9c44ef608a1c298bef1b6c055`. Tested executable: ty **0.0.85**, resolved by `@multitool//tools/ty`. The upstream rules_lint lock pins the Linux x86_64 musl archive with SHA256 `52da86894541610d757293c4e9f1c03ec4a1d888ce445e4e373510e4d96595cb`.

The experiment adds ty to the existing Aspect lint configuration and uses root `pyproject.toml` as its declared configuration. The temporary Python version argument is literal `3.15`; it deliberately does not pretend to implement selector derivation.

## Findings established so far

| Probe                  | Observation                                                                                                                                                                                                             | Evidence                                                           |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Provisioning           | Bazel resolved and ran ty 0.0.85 from the shared multitool hub.                                                                                                                                                         | `evidence/version.log`                                             |
| Existing coverage      | Library, binary, pytest test and hidden-script deliberate invalid assignments reached ty reports; the hard lint command exited 1.                                                                                       | `evidence/injected-errors.log`, `evidence/injected-source.diff`    |
| Future targets         | Gazelle generated two new Python libraries, including a hidden-directory package. Recursive lint found their deliberate errors without changing a target allowlist.                                                     | `evidence/future-gazelle.log`, `evidence/local-future.log`         |
| Default failure policy | Default hold-the-line reported two findings on the unchanged baseline; hard reported ten, including duplicate source findings through multiple targets. Default is not sufficient for unconditional strict enforcement. | `evidence/baseline.log`, `evidence/baseline-hard.log`              |
| Annotation probes      | Missing function annotations and explicit Any did not produce ty diagnostics under the tested settings. Exact ty/Ruff annotation policy remains undecided.                                                              | `python_sample/ty_prototype/probe.py`, `evidence/local-future.log` |
| Existing migration     | Baseline diagnostics include invalid HTTP handler overrides and attributes added dynamically to server objects. No production annotations were migrated.                                                                | `evidence/baseline-hard.log`                                       |

The original source files were restored byte-for-byte after the injected-error run. New prototype files are disposable fixtures. The own-import-root/stub fixture has a hand-authored BUILD after the initial future-target generation probe; that later fixture is not evidence of automatic generation.

## Coverage boundaries

The pinned aspect skips `no-lint` targets. The lint config also filters `no-lint` targets. The inspected first-party Python BUILD files have no such tags. `filter_srcs` skips generated source files unless a target has `lint-genfiles`. A Gazelle-generated target over authored Python is distinct from a generated Python source file. The requirement here covers authored first-party sources.

The aspect invokes `ty check --force-exclude`. Existing hidden scripts and a newly generated hidden package were checked in the executed probes; future user-configured exclusions still need an explicit policy and coverage guard.

The aspect reads dependency PyInfo imports and transitive sources, including installed wheel directory artifacts. It does not collect the current target's own PyInfo imports. Wheel providers expose their unpacked install directory through PyInfo.transitive_sources; this includes package metadata when present. Source inspection alone is not runtime proof for all metadata/stub cases.

## Remaining gates

- Own target import roots and stub behavior: local fixture results recorded below when complete.
- Derive the checked Python version from the Bazel selector rather than a hardcoded checker argument; prove selector changes invalidate checks.
- Verify selected-version syntax/stdlib behavior and source, dependency, stub, config and binary changes.
- Complete CI-configuration remote execution. Automatic permission review rejected that command because it would send source and derived artifacts to BuildBuddy without explicit payload/destination approval. No hosted GitHub Actions run or CI success is claimed.
- Human review must precede resolving this prototype ticket. The strictness-policy decision remains blocked until the feasibility gate is accepted.

## Reproduction

Enter the repository development shell using `nix develop`.

```sh
bazel run @multitool//tools/ty -- --version
aspect lint --aspect=//tools/lint:linters.bzl%ty --strategy=hard \
  --remote_executor= --remote_cache= --bes_backend= \
  --bazel-flag=--strategy=AspectRulesLintTy=sandboxed \
  //python_sample/... //.github/scripts/...
```

The broad source tree has pre-existing type errors; exit 1 is expected until the later migration. Probe-specific reports distinguish those baseline errors from introduced fixture errors.

## Selector experiment

`selector-315.json` and `selector-314.json` were produced by local `bazel aquery` analysis of the same dependency-free hidden library, changing only `--@rules_python//python/config_settings:python_version` from 3.15 to 3.14. Both ty action keys were unchanged, and both command lines still passed `--python-version 3.15`. This demonstrates why passing a literal version to the stock factory is insufficient. It is analysis evidence; aquery does not execute the checker.

The action input graph declares root `pyproject.toml`, the authored source, and the multitool checker executable. Declared inputs support cache invalidation by content, but the complete mutation matrix has not been executed and is not claimed proven.

The selected-version syntax probe follows the official [Python 3.15 release notes](https://docs.python.org/3.15/whatsnew/3.15.html), which document lazy imports and `typing.TypeForm`.

## Local results after import and mutation probes

`local-imports-fixed.log` confirms the own-root failure: `local_helper` is present in the current target's sources and declared through its `imports`, yet ty reports `unresolved-import`. The same sandbox resolves `versions` through a dependency's first-party import root, the packaging wheel, and a `.pyi` dependency. The original hidden-script import root was also exercised by baseline pytest targets without unresolved-import diagnostics.

`mutations-raw.txt` captures the full human diagnostic output, including warnings suppressed from the hard-strategy summary:

- Changing root `pyproject.toml` from error to warning changed invalid-assignment severity.
- Changing the imported `sort_versions` return annotation to `list[int]` changed the consuming probe's return-type diagnostic.
- Changing the stub return type from int to str produced an incompatible-assignment warning in the consuming source.
- The same source contained Python 3.15 `lazy import math` and `typing.TypeForm`; neither produced syntax, unavailable-symbol or typing diagnostics under `--python-version 3.15`. This is a representative compatibility probe, not exhaustive Python 3.15 support proof.

The source/config/stub mutations were removed after their diagnostic snapshots. Original production Python files match the base commit. No checker-binary replacement was executed; that mutation gate remains open. Selected-version cache invalidation failed in action analysis as recorded above.

## Human review recommendation

The feasibility gate is **not passed**. Retain ty and multitool provisioning, but revisit the selected integration approach to permit a small provider/version adapter (or upstream patch) that collects current-target imports and derives the Python version from the actual selector. Unconditional failures also need explicit handling; applying hard strategy globally changes other linters' behavior and should be a conscious decision rather than an incidental rollout.

Continue this prototype with those changes only after agreeing on the revised integration boundary, then complete the mutation matrix and remote check. Keep the strictness-policy ticket blocked. No issue is closed by this report.

## Final preservation checks

The complete local lint run (`evidence/resumed-final-lint.log`) found an additional selector mismatch: Ruff infers Python 3.14 from the root project's `requires-python` and rejects the Python 3.15 lazy-import probe. ty accepted that same probe with its explicit 3.15 flag. The final source fixture uses an ordinary import; the lazy-import source and both diagnostic results remain in the evidence snapshots. The prototype's incidental unsorted import block was also cleaned before preservation.

Full repository formatting passed (`evidence/final-format.log`). No production Python source changed. The lint run's nonzero result is expected evidence of the existing annotation baseline and integration gaps; it is not a green acceptance result. A final cleaned-fixture lint result is recorded separately in `evidence/preservation-lint.log`. No full monorepo build or hosted CI run is claimed.

The initial `evidence/final-lint.log` was interrupted before it recorded an exit status; `evidence/resumed-final-lint.log` is the completed replacement. Intermediate fixture/Gazelle failures are retained rather than represented as successful checks.

The cleaned-fixture verification completed: full formatting passed with no modifications, and local hard lint exited 1 with eleven ty findings (ten baseline findings including duplicates, plus the own-import-root reproduction). There were no Ruff findings in that final run. This is an intentionally failing feasibility artifact, not production-ready integration.
