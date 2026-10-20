# Bazel and Aspect original-source coverage

## Result and scope

Bazel 9.3.0 supplies original statement-entry events through its native debugger.
The bounded adapter captured BUILD, MODULE, loading, and analysis locations.
However, all three debugger commands exited 2 despite successful build events.
This experiment does not establish an end-to-end collector that succeeds.

The patched Aspect evaluator contains a coverage API.
The inspected Aspect CLI does not enable or export that API.
Its maintained task passed, but this experiment obtained no native Aspect coverage profile.
A separate adapter remains a candidate, with optimizer and lifecycle limits to resolve.

This report answers [Determine Bazel and Aspect Starlark original-source coverage](https://github.com/LowkeyLab/bazel-repo/issues/2454).
It extends [Classify remaining maintained executable sources for coverage](https://github.com/LowkeyLab/bazel-repo/issues/2444).
It provides evidence for [Choose collector mechanisms and exceptions after feasibility research](https://github.com/LowkeyLab/bazel-repo/issues/2445).
It selects no final mechanism and approves no exception.

## Pins and native hooks

The refreshed repository base was `df390e4205234e7ea4d8b09be436739a23de27a2`.
Bazel was 9.3.0, with executable SHA-256 `d302d22ed77ee5658b87857249baa579db84300ecebe0a1e9a357b5b20f80eb8`.
The native [`Debug.Debugger.before` hook](https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/net/starlark/java/eval/Debug.java) receives statement locations.
[`Eval.exec`](https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/net/starlark/java/eval/Eval.java#L287-L293) invokes it before statement execution.
An entry event does not prove that the statement completed.

The [`StarlarkDebuggerModule`](https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/starlarkdebug/module/StarlarkDebuggerModule.java) installs that hook for build commands.
The [debugger options](https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/starlarkdebug/module/StarlarkDebuggerOptions.java) enable a TCP server and optional analysis invalidation.
The [protocol](https://github.com/bazelbuild/bazel/blob/9.3.0/src/main/java/com/google/devtools/build/lib/starlarkdebug/proto/starlark_debugging.proto) exposes line breakpoints and paused source locations.
The adapter uses unconditional breakpoints at parsed statement starts.
It records every received event and immediately continues each paused thread.
It does not evaluate expressions inside the debugger.

The [flake](https://github.com/LowkeyLab/bazel-repo/blob/df390e4205234e7ea4d8b09be436739a23de27a2/flake.nix) pins Aspect to `66298af52ed870fb6651eedc0d8ee2a0762caab6`.
The effective executable SHA-256 was `677df1df4b911d3aee6453a62f428e8e76cd67e6625434c9619224e8b4c876b3`.
Its version command identifies a development CLI, so the source pin and executable hash identify the tested build.
The [package](https://github.com/LowkeyLab/bazel-repo/blob/df390e4205234e7ea4d8b09be436739a23de27a2/nix/packages/aspect/default.nix) stages patched `starlark` 0.14.2.
The flake also applies `delivery_versioned_flags.patch` and `python_typing.patch`.

The patched crate exposes `ProfileMode::Coverage`, `enable_profile`, and `coverage()`.
The [coverage API](https://docs.rs/starlark/0.14.2/starlark/eval/struct.Evaluator.html#method.coverage) returns source spans.
Its implementation warns that optimizer transformations can remove statements or give incorrect spans.
The archived patched implementation preserves that warning.
No executed experiment here establishes precise Aspect line or branch coverage.

Aspect creates separate evaluators for [file loading](https://github.com/aspect-build/aspect-cli/blob/66298af52ed870fb6651eedc0d8ee2a0762caab6/crates/axl-runtime/src/eval/load.rs) and [config/task execution](https://github.com/aspect-build/aspect-cli/blob/66298af52ed870fb6651eedc0d8ee2a0762caab6/crates/axl-runtime/src/eval/multi_phase.rs).
Inspection found no `enable_profile` or `ProfileMode` call in the pinned CLI crates.
The upstream Starlark patch and local patches do not add profile collection at these sites.
This is an integration gap in the inspected CLI, rather than absence of an evaluator API.

## Executed capability matrix

| Observation          | Bazel                                                                                                        | Patched Aspect CLI                                                                                                     |
| -------------------- | ------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| Maintained function  | `tools/python/defs.bzl`: `py_library` and `_coverage_sources_impl`                                           | `tools/aspect/delivery_flags.axl`: `_impl`                                                                             |
| Functional result    | Shell output contains `RESEARCH_EXECUTION`. BEP reports build success. Process exits 2.                      | Both maintained assertions pass. Process exits 0.                                                                      |
| Original events      | Native statement-entry locations include MODULE, BUILD, `.bzl` loading, and analysis callbacks.              | The CLI exported no coverage profile. Functional prints do not establish line hits.                                    |
| Untaken statements   | Analysis entry reaches lines 15 and 22. Bodies at lines 16–19 and 23–34 have no entry events.                | Failure bodies remain untaken in the successful task. Their hit counts remain unmeasured.                              |
| Unloaded source      | Parser records `unloaded.bzl` locations without evaluating its fatal top-level expression.                   | Parser records `unloaded.axl`. The CLI succeeds with its fatal expression present but unloaded.                        |
| Cached loading       | An identical warm build produces zero events and configures zero packages/targets.                           | Two import paths share one wrapper evaluation. `ASPECT_LOADING_ONCE` appears once.                                     |
| Baseline             | Five source identities and 262 statement-start locations, all zero, before profile application.              | Four source identities and 57 parser locations. All hits remain unmeasured.                                            |
| Branch evidence      | Statement locations expose selected bodies. The protocol supplies no branch identifiers or outcome counters. | The experiment captured no runtime branch profile. The crate API alone does not establish branch support.              |
| Missing/corrupt data | Independent converter rejects missing, empty, and corrupt profiles.                                          | Missing evaluator output must fail collection despite the successful task. No positive profile was available to check. |

The first poison `.axl` fixture sat directly under `.aspect`.
Aspect discovered it and failed startup with exit 1.
The corrected placement keeps the same source under `tools/research_starlark`, outside the discovery directory.
The maintained task then succeeds without evaluating that source.
Both receipts remain in the archive.

The independent inventory uses the Bazel-selected Starlark-Go parser at `492d3672b3f4`.
`bazel mod show_repo` records version `v0.0.0-20250205221240-492d3672b3f4` and its checksum.
The root Go module declares an older indirect version, so that declaration alone does not identify the effective parser.
The archive preserves the selected parser source and its hashes.
This parser establishes statement starts for the accepted fixture syntax.
It does not establish complete grammar compatibility with Bazel or typed Aspect syntax.
The candidate inventory excludes string documentation statements.
The inventory includes function definitions and load statements.
It does not classify every expression line as independently executable.

## Runtime receipts and aggregation

The first debugger build starts after this worker stops its prior Bazel server.
It records 277 events, including 201 MODULE events.
The identical warm build records zero events.
Its console reports zero packages loaded and zero targets configured.
Those absent fresh events do not establish fresh zero coverage.

The first fixture references an empty Python library.
Its disabled coverage select does not retain the maintained analysis helper.
The revised fixture explicitly references `subject.python_coverage`.
The direct Bazelisk reproduction enables analysis invalidation and records 285 events.
It includes helper entries at lines 12, 13, 14, 15, 20, 21, 22, and 35.
The expected original lines were not moved to match events.
Historical fixture snapshots preserve both dependency edges.

All debugger commands retain their actual process exit 2.
Their build events report `overallSuccess: true` and `SUCCESS`.
The adapter exits 1 because the child process failed.
The bounded direct Bazelisk reproduction retains the discrepancy.
Its cause remains unresolved.
The final adapter adds type annotations and bounded cleanup after these observations.
Build and lint results cover those edits, without another debugger experiment.

Each debugger command requests an execution JSON file, but none appears.
The repository also configures compact execution logging.
This observation does not establish the cause of exit 2.
The first console reports one remote process and no remote cache hit.
The action output contains the expected marker.
Together, these provide bounded execution evidence, without a verified per-action JSON receipt.
The revised run reports one action cache hit.
Registering `ctx.actions.run_shell` occurs during analysis.
The resulting shell action does not execute `.bzl` source statements.

The diagnostic LCOV uses the revised run's 285 native events and five original source identities.
It contains 262 locations, with 247 positive locations.
Counts represent debugger statement entries, rather than completed statements or branch outcomes.
The raw observed columns remain in event receipts.
LCOV collapses them to original statement-start lines.
An unknown source or location causes conversion failure.

Missing, empty, and corrupt profile controls each exit 1 after functional output exists.
The valid data conversion exits 0.
This data result does not override failed orchestration.
The adapter does not establish authenticity or completeness for every possible truncated profile.
Aspect baseline zeros stay separate from measured Bazel events.
They must not conceal the missing Aspect collector.

The first merge attempt uses `.lcov` input suffixes.
The merger ignores those inputs, warns about absent coverage, and exits 0 with empty outputs.
That byte comparison establishes no aggregation result.
The corrected check uses `.dat` inputs.
Both orders preserve all five source identities and all 262 location counts.
The combined diagnostic LCOV files contain 2,605 bytes and remain byte-identical.

Both input orders use the [coverage tool pinned by Bazel 9.3.0](https://github.com/bazelbuild/bazel/blob/9.3.0/tools/test/extensions.bzl).
Its release is `coverage_output_generator-v2.9`.
Its README identifies source commit `d49967e8cf23b2f1179eba168766e93afb736c6b` and build Bazel 8.3.0.
The archive records the release checksum and effective JAR checksum.
They retain unloaded zero locations.
These are merger observations, rather than a successful native coverage command or hosted upload.

## Remaining observations and candidates

A Bazel debugger adapter is a candidate for original statement-entry coverage.
Before selection, resolve its process exit and absent execution JSON receipts.
Define evaluator lifecycle receipts and cached-result reuse before treating cached absence as coverage data.
Check grammar compatibility and executable-location policy beyond this specified corpus.
Define branch events separately from statement entry.

An Aspect adapter must enable profiling in each actual evaluator phase and export its data before evaluator disposal.
It must prove original spans after the effective patches and optimizer transformations.
It must preserve frozen-module cache observations and independently discover unloaded source locations.
This experiment did not execute that runtime adapter.
The successful maintained task and single-evaluation wrapper do not supply those missing observations.

The experiment changes no maintained production implementation, coverage policy, threshold, Python architecture, or Codecov workflow.

## Evidence and checks

The [reproduction notes](starlark-original-source-2454/REPRODUCE.md) describe the exact commands and evidence mappings.
The [archive](starlark-original-source-2454/evidence.tar.gz) contains sanitized original observations, source snapshots, native hook sources, baselines, intermediate LCOV, and combined LCOV.
Raw private receipts remain separate.
Sanitization replaces machine path prefixes with documented placeholders.
The wire archive re-encodes protobuf lengths after those replacements.
It preserves event fields and values except those path substitutions.

The [fresh debugger build](https://app.buildbuddy.io/invocation/445a895a-a665-4eba-b49a-737041414f39), [cached build](https://app.buildbuddy.io/invocation/d89c7d33-45a5-4d70-9719-7558b43cc530), and [direct reproduction](https://app.buildbuddy.io/invocation/cbaf251a-1adf-4eb6-b7f0-a9f2cf767422) remain distinct.
The [adapter build](https://app.buildbuddy.io/invocation/26c29730-25a8-4862-8d5c-4dbc3ad7174e) reports six remote processes and 44 action cache hits.
The [final targeted lint](https://app.buildbuddy.io/invocation/8cf3e415-a2d4-4184-a4e0-dfdf0b07440c) passes Ruff, Ty, and KeepSorted.
The [final adapter build](https://app.buildbuddy.io/invocation/5f8c122f-efca-4de9-b808-a1fa16bfc5b4) passes with two remote processes and 1,597 action cache hits.
Full repository formatting passes.
The strict English scan reports no findings.
Final data checks preserve the diagnostic bytes and reject the same three faults.
Both corrected merger orders match every expected location and count.
The task server stops with exit 0.
Archive hash validation and whitespace checks complete the local checks.
