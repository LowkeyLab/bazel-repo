# Node and Angular runtime feasibility

This report addresses [Validate Node and Angular baselines, mappings, and child collection](https://github.com/LowkeyLab/bazel-repo/issues/2451).
It extends the completed [collector source research](https://github.com/LowkeyLab/bazel-repo/blob/257619b1d73a6bc9ee577927bd5718ce3a5b4ff0/docs/research/2026-10-19-node-angular-collector-guarantees.md).
The refreshed research base is `5e246a47cd7ea0a9c506e915db0c053d7d84d023`.
No production collector changes or coverage exceptions follow from these experiments.

## Retained native experiment

The previous worker ran two native fixtures before the interruption.
Each fixture executed production TypeScript, an executable helper, and an awaited child with child-only TypeScript.
The inventory fixture also included a disconnected TypeScript module in its manifest.
The [fixtures](../../tools/coverage/node_research/BUILD.bazel) preserve these two target definitions.

Functional assertions passed. Parent and child V8 profiles contain positive function counts.
However, the [authoritative combined report](node-angular-runtime/summary/native-combined.lcov) reports zero-hit baselines for the executed TypeScript modules.
Only the native test entry retains positive line counts.
The disconnected module's baseline includes its comment, interface, and blank lines.
Thus, this native configuration does not establish exact executable-line baselines or preserved TypeScript hits.
A successful functional test and positive native profiles do not establish authoritative LCOV coverage.

## Adapter snapshots

The [snapshot manifest](../../tools/coverage/node_research/adapters/sources.json) records the original path and SHA-256 for each adapter.
Each `.input` snapshot contains the exact bytes of its maintained `.ts` source at the research base.
The harness copies each snapshot to `setup.mjs` for direct execution with the pinned Node toolchain.
These experiments execute the snapshot functions. They do not execute the maintained Angular test targets.

The initial matrix stopped during Bazel analysis because two maintained configuration targets have private visibility.
No test action ran in that attempt. Production visibility remains unchanged.
The snapshot matrix supplies isolated report files and writable temporary directories.
It invokes all six teardown functions with absent, empty, and partial LCOV.
A separate directory probe attempts report creation beside read-only configuration files and inside writable temporary storage.

## Evidence boundaries

The source research supplies the pinned implementation references and current configuration contracts.
This report adds executed observations and preserves their inputs and reports.
Initial commands use two jobs. Later commands permit eight remote jobs while retaining two local CPU units and 2048 MB of local action memory.
The Bazel JVM uses a 1536 MB heap limit.
A remote-cache hit establishes artifact reuse. A fresh remote test action establishes execution of that research fixture.
Neither establishes hosted CI, Codecov upload, production acceptance, or every platform's behavior.

## Executed capability matrix

The [retained archive](node-angular-runtime/retained-evidence.tar.gz) preserves inputs, manifests, emitted scripts, maps, profiles, reports, commands, and logs.
Published artifacts replace machine paths with `<HOME>`, `<BAZEL_OUTPUT_BASE>`, `<REMOTE_EXECROOT>`, `<TEST_TMPDIR>`, and `<NIX_TMPDIR>`.
Private runtime storage and ignored working artifacts retain the original bytes.
The [archive manifest](node-angular-runtime/summary/archive-manifest.json) supplies published member hashes.
The archive protects exact executed source layouts from later repository formatting.

| Requirement                     | Executed observation                                                                                                                       | Result                                              |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------- |
| Native original-source manifest | Executed TypeScript retains synthetic zero counts.                                                                                         | Insufficient.                                       |
| Native emitted-script manifest  | Production, helper, test, and child records retain positive hits under original paths.                                                     | Candidate mapping adapter.                          |
| Native exact denominator        | Comments, interfaces, and blank lines enter the line records.                                                                              | Insufficient.                                       |
| Native missing profiles         | Functional test passes. Converter exits zero. Independent expected-hit validation fails.                                                   | Additional validation required.                     |
| Native corrupt profiles         | Functional test passes. Converter exits zero. Independent expected-hit validation fails.                                                   | Additional validation required.                     |
| Missing manifest runfile        | Converter exits zero. Independent existence validation reports the absent source.                                                          | Additional validation required.                     |
| Angular explicit inclusion      | The disconnected module receives zero-hit executable lines and conditional branches. Non-code lines remain absent.                         | Demonstrated for this fixture.                      |
| Angular parent mappings         | Original production and helper records retain hits. The untaken arm retains zero counts.                                                   | Demonstrated for this fixture.                      |
| Angular child defaults          | The child succeeds. Its baseline remains zero despite execution.                                                                           | Child collection is incomplete.                     |
| Angular child attachment        | `autoAttachSubprocess` supplies positive child-only counts in authoritative Vitest LCOV.                                                   | Demonstrated for this fixture.                      |
| Angular test membership         | The spec has no source record despite explicit builder inclusion.                                                                          | Required test-source observation remains missing.   |
| Six adapter teardown paths      | All snapshots return normally for absent, empty, and partial LCOV.                                                                         | No completeness guarantee.                          |
| Read-only report directories    | Privilege-drop probes reject writes beside Predix/Tailwind configurations. Writable temporary storage succeeds.                            | Temporary-directory placement is a candidate.       |
| Hidden owned source             | Bazel ownership discovery includes `.owned/hidden.ts`.                                                                                     | Demonstrated in the bounded subtree.                |
| Undeclared source               | Filesystem discovery finds `undeclared.ts`. Bazel ownership does not. The independent comparison fails.                                    | No central project list required.                   |
| Baseline merge                  | Combined LCOV preserves positive production/helper/child hits under one source identity. The native baseline adds false denominator lines. | Hit preservation succeeds. Exact denominator fails. |

The [native summary](node-angular-runtime/summary/native-adapter-results.json) distinguishes converter exits from independent validation errors.
The [Angular summary](node-angular-runtime/summary/angular-results.json) distinguishes each injected teardown input and directory probe.
The [discovery result](node-angular-runtime/summary/discovery.json) records the ownership mismatch.
The [combined report](node-angular-runtime/summary/authoritative-combined.lcov) exposes both preserved hits and denominator contamination.

## Candidate mechanisms and remaining observations

Use explicit builder inclusion as the Angular baseline candidate. It supplies executable-only zero records for the demonstrated disconnected module.
Use subprocess attachment when child coverage must reach authoritative Vitest output.
The bounded adapter maps copied fixture originals to their repository source identities before Bazel merging.
These mappings apply only to exact fixture copies. They do not establish general source normalization.

An emitted-script manifest is a native mapping candidate. It does not solve the executable-denominator defect.
Do not combine its synthetic baseline with an exact Angular baseline without validation of the resulting line set.
An independent inventory must validate ownership, source existence, canonical identities, and expected executable records.
Each adapter must reject missing, empty, or partial output before successful functional tests can establish collection completeness.
The deliberate profile-loss cases also require independent evidence that expected executions reached the report.

The Angular test-source record remains unproven. A later adapter must demonstrate executable test-source records after builder and provider exclusions.
The native exact-baseline mechanism also remains unproven. It needs original executable lines without comment, interface, or blank-line inflation.
These are missing observations, not approved exceptions or unsupported-source classifications.
The later human decision must consider these gaps before production planning.

## Pins and final artifacts

The [base module configuration](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/MODULE.bazel) selects Node `24.15.0`.
It pins rules_js to `e67442a2ccf329841c9227d66ee013afcf5633c6` and rules_angular to `bfe6e98434982bad38c393f1bd472ec33f16cd32`.
The [lockfile](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/pnpm-lock.yaml) selects Angular build `22.2.3`, TypeScript `6.0.3`, and Vitest/provider `5.0.5`.
The Node runtime and Vitest output report their effective versions. Bazel is `9.3.0` on Linux with an x86-64 toolchain.

The [final artifact archive](node-angular-runtime/final-evidence.tar.gz) preserves the formatted fixture inputs and final execution outputs.
The [final manifest](node-angular-runtime/summary/final-archive-manifest.json) supplies member hashes.
The [commands](node-angular-runtime/summary/commands.txt) retain meaningful flags and resource limits.
The [verification result](node-angular-runtime/summary/verification.json) records exact copied-source bytes, unique canonical sources, and preserved measured hits.

The temporary reporter captures provider profile files before cleanup.
It also captures each remapping input's generated code and source map.
The final attached-child run records one provider profile file and five remapping inputs.
Those inputs include the executed production/helper bundle and the spec bundle.
The spec bundle's presence does not establish a test-source LCOV record.
The pinned [provider lifecycle](https://github.com/vitest-dev/vitest/blob/v5.0.5/packages/vitest/src/node/coverage.ts) removes its temporary files after reporting.
The hook preserves them before that removal without changing the conversion result.

## Final validation

| Operation              | Evidence                                                                                             | Scope                                                                                                                |
| ---------------------- | ---------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Native matrix          | [Fresh remote invocation](https://app.buildbuddy.io/invocation/69e83d3e-5c9f-48dd-8b17-3cab7eff9fff) | One fresh test. Log records 11 remote actions, one remote-cache hit, and 15 action-cache hits.                       |
| Authoritative coverage | [Fresh remote invocation](https://app.buildbuddy.io/invocation/f41f32c3-0cc1-4149-b52a-04e8a068cafb) | Two fresh tests. Log records 11 remote actions, two local actions, one remote-cache hit, and 1250 action-cache hits. |
| Targeted lint          | [Invocation](https://app.buildbuddy.io/invocation/a7f621c0-8dd3-45a1-87fb-eb9ed1c5a880)              | KeepSorted ran with no findings. No ESLint or repository-wide lint result follows.                                   |
| Repository formatting  | Full scope passed.                                                                                   | Formatting changes precede the final executed fixtures.                                                              |
| Evidence validation    | Copied-source hashes and measured-hit comparisons pass.                                              | The archive retains all exact inputs and final reports.                                                              |
| Strict English         | Structural validation passes.                                                                        | Technical literals and exact issue titles retain their original spelling.                                            |

No repository-wide production build, hosted CI, production coverage acceptance, or Codecov upload ran.
