# Node and Angular collector guarantees

Date: 2026-10-19.
Repository base: `151e15cd495b5d63155753c0acf09f50102f0cbc`.
Question: [Determine Node and Angular collector guarantees][ticket].

## Result and evidence limits

The native Node collector can convert manifest sources and V8 profiles to LCOV.
Angular supplies a separate Vitest report through six maintained adapters.
A nonempty adapter report takes precedence over native Node reporting.
Thus, native collector capability does not prove Angular report completeness. [Bootstrap][bootstrap], [Angular rule][ng-test]

This investigation inspected pinned sources. It ran no coverage fixture, remote collection, or hosted upload.
The experiments below remain necessary before a runtime guarantee.
No exception receives approval in this report.
Browser execution and Astro templates remain separate investigations. [Question][ticket], [Investigation decision][investigation]

The approved policy includes maintained production sources, test sources, and executable test helpers.
It requires disconnected zero-hit lines, canonical original identities, supported branches, and first-party child-process execution.
It requires failure on unexplained missing data. Preserve the accepted Python architecture, combined LCOV, thresholds, and Codecov policy. [Membership decision][membership], [Investigation decision][investigation]

## Starting evidence and current pins

The previous [mechanism report][mechanisms] established that instrumentation cannot discover disconnected roots.
The previous [evidence report][evidence] identified Node manifest inclusion and one Angular missing-output path.
Neither report supplied current runtime fixtures for this ticket.
This report extends that evidence to every maintained adapter, report precedence, conversion failures, and current Vitest baseline conditions.

The refreshed repository pins these dependencies. [Bazel dependencies][module], [Node dependencies][lock]

| Dependency                       | Pin                                                               |
| -------------------------------- | ----------------------------------------------------------------- |
| Bazel                            | `9.3.0`                                                           |
| `aspect_rules_js`                | `e67442a2ccf329841c9227d66ee013afcf5633c6`                        |
| `aspect_rules_ts`                | `7f76b72dcb5d2022d8b3db4c70ae9ae676d72969`                        |
| `rules_nodejs`                   | `5fd53b1684fc3fc92d6f480a9704e795e5039d55`                        |
| Node                             | `24.15.0`                                                         |
| `rules_angular`                  | `bfe6e98434982bad38c393f1bd472ec33f16cd32`, plus repository patch |
| Angular build                    | `22.2.3`                                                          |
| Vitest and `@vitest/coverage-v8` | `5.0.5`                                                           |

The Angular Git tag lookup returned 404.
Instead, this investigation inspected the exact published [Angular package][angular-package].
Its SHA-512 integrity matched `pnpm-lock.yaml`:

```text
sha512-EtNEuPBUCyIuzdXrQ/nINvgFzW8bUI5AkAZtJiSn7nsFOlvaygaKN0kENNTO3zRl3T9B4GDog+Ks5Nec3FBkhg==
```

The Angular source references below name files inside that archive.
This avoids attributing current package behavior to an unverified Git revision. [Package metadata][angular-metadata], [Lockfile][lock]

## Native Node collector

### Membership and zero-hit records

`js_library` propagates instrumentation metadata through `srcs` and `deps`.
Test-only `js_binary` rules propagate it through `data`.
Both restrict extensions to the collector list. [Library metadata][library], [Binary metadata][binary]

The report generator reads `COVERAGE_MANIFEST` and resolves entries beneath the workspace runfiles directory.
Its replacement exclusion class checks exact resolved membership.
It accepts `.js`, `.ts`, `.jsx`, `.tsx`, `.mjs`, `.cjs`, `.mts`, and `.cts`.
It does not apply the usual test-name exclusions from `test-exclude`. [Manifest generator][generator]

Consequently, test code and executable helpers are candidates when their declarations propagate into the manifest.
A filename suffix alone does not exclude them at this layer.
This is a source-level conclusion, not demonstrated coverage for every repository rule. [Manifest generator][generator], [Binary metadata][binary]

`all: true` creates synthetic zero-count ranges for existing manifest files without measured profiles.
However, `globSync` silently removes manifest entries whose runfiles do not exist.
The generator cannot discover a disconnected project absent from the manifest. [Manifest generator][generator]

The bundled c8 code converts those ranges through source maps and `v8-to-istanbul`.
Its uncovered range spans the file size.
This establishes a candidate baseline, not exact executable-line classification for arbitrary unexecuted TypeScript.
Compare known executable and non-executable lines before accepting the denominator. [Bundled conversion][conversion]

Selecting a disconnected library is insufficient by itself to demonstrate native reporting.
The report runs within a test action where sources and profiles coexist.
A candidate adapter must supply inventory sources to a reporting action with all required originals, emitted JavaScript, and maps.
Determine that action's inputs from Bazel ownership, rather than another project list. [Bootstrap][bootstrap], [Membership decision][membership]

### Original mappings, branches, and merging

The bootstrap captures source-map cache entries for runtime transpilers.
The reporter also reads on-disk source maps.
A missing recoverable map can leave a script unmapped. [Bootstrap][bootstrap], [Bundled conversion][conversion]

The bundled reporter merges V8 process data and then Istanbul file coverage.
Its LCOV writer emits line and branch records.
These are source-level capabilities for supplied data.
They do not establish exact original TypeScript lines, branch semantics, or canonical paths after Bazel aggregation. [Bundled conversion][conversion]

The reporter resolves manifest paths against the runfiles workspace.
Source maps can introduce different original paths.
Require one repository-relative identity after native conversion, Angular adaptation, and Bazel merging.
Compare measured hits before and after adding zero baselines. [Manifest generator][generator], [Bundled conversion][conversion], [Membership decision][membership]

### Processes and silent losses

The bootstrap starts precise inspector coverage and enables `NODE_V8_COVERAGE` for child Node processes.
Only the reporting root runs the reporter.
Collection therefore depends on child environment inheritance and profile completion before reporting. [Bootstrap][bootstrap]

A normal awaited child is a candidate supported case.
A child that clears the environment, outlives its parent, or uses an unrelated launcher needs separate evidence.
Non-Node children need their own language collector.
No child completion inventory or required-profile receipt appears in this bootstrap. [Bootstrap][bootstrap]

These source paths can permit incomplete output without a functional test failure:

- The bootstrap logs and returns when inspector collection is unavailable.
- The bootstrap logs and returns when the report generator runfile is absent.
- The manifest enumeration omits missing source runfiles.
- Bundled c8 skips invalid profile JSON after a debug log.
- Bundled c8 catches individual conversion failures and continues.
- A nonempty `COVERAGE_OUTPUT_FILE` suppresses native reporting, except with split post-processing.

Flush failures and report-process failures do set a failing exit code.
Thus, some collection failures propagate, but this is not a complete missing-data guarantee. [Bootstrap omission paths][bootstrap-loss], [Manifest generator][generator], [Conversion omission paths][conversion-loss]

## All maintained Angular adapters

`angular.json` enables coverage and `lcovonly` for six projects.
The local macro calls the pinned `ng_test`, which calls `js_test` with Angular CLI as its entry point.
The CLI runs `test <project> --no-watch`.
That Node driver is distinct from the Vitest report that the adapter copies. [Angular configuration][angular-config], [Local macro][ng-local], [Pinned rule][ng-test]

Each row below covers both configuration and teardown source files.

| Project         | Report directory                                                    | Missing LCOV   | Identity conversion                      | Source                                           |
| --------------- | ------------------------------------------------------------------- | -------------- | ---------------------------------------- | ------------------------------------------------ |
| Mindreadr       | `TEST_TMPDIR` fallback to config directory, then `.vitest-coverage` | Log and return | Prefix `angular/` unless already present | [Setup][mindreadr], [config][mindreadr-config]   |
| Nicknamer       | Same temporary-directory rule                                       | Log and return | Same prefix rule                         | [Setup][nicknamer], [config][nicknamer-config]   |
| Nicknamer2 Web  | Same temporary-directory rule                                       | Log and return | Same prefix rule                         | [Setup][nicknamer2], [config][nicknamer2-config] |
| Pinyin Composer | Same temporary-directory rule                                       | Log and return | Same prefix rule                         | [Setup][pinyin], [config][pinyin-config]         |
| Predix          | Config directory, then `.vitest-coverage`                           | Log and return | Same prefix rule                         | [Setup][predix], [config][predix-config]         |
| Tailwind Sample | Config directory, then `.vitest-coverage`                           | Log and return | Same prefix rule                         | [Setup][tailwind], [config][tailwind-config]     |

All six return when `COVERAGE_OUTPUT_FILE` is absent.
All six copy existing LCOV without checking required sources, line records, branch records, or syntax.
A present empty file therefore passes the adapter itself.
Each row links the inspected `vitest-base.config.ts` and `vitest-global-setup.ts`.

Predix and Tailwind can require writes beneath the configuration directory.
That differs from the other four adapters' temporary-directory support.
A read-only runfiles failure is a concrete risk, not a demonstrated failure from this session. [Predix files][predix], [Tailwind files][tailwind]

The prefix operation is not full normalization.
For example, `/tmp/a.ts` becomes `angular//tmp/a.ts`, and `../a.ts` becomes `angular/../a.ts`.
The adapters neither reject these identities nor compare them with declared originals. [Predix files][predix], [Pinyin files][pinyin]

Missing Vitest output does not force failure in any adapter.
The native Node fallback can then attempt a different report.
That fallback does not prove Angular worker source coverage.
Conversely, nonempty partial Vitest output can suppress native reporting entirely. [All adapter files][angular-projects], [Bootstrap][bootstrap]

### Angular and Vitest membership boundary

The exact Angular archive's `src/builders/unit-test/runners/vitest/plugins.js` sets `excludeAfterRemap: true`.
It adds `spec-*.js` and `chunk-*.js` when the caller supplies `coverageInclude`.
It supplies source maps for bundled output and adjusts original paths.
Builder-internal chunks can receive `virtual:builder` identities. [Angular package][angular-package]

The archive's `options.js` passes `coverageInclude` and `coverageExclude` from builder options.
Its `build-options.js` enables script source maps.
None of the six repository configurations sets `coverageInclude` or a Vitest `coverage.include`. [Angular package][angular-package], [Angular configuration][angular-config], [All adapter files][angular-projects]

Vitest's pinned provider generates untested coverage only when `options.include` is non-null.
It merges transformed original-source records and filters missing originals.
Parse failures can return empty coverage after logging an error.
These paths require expected-inventory validation. [Vitest provider][vitest-provider]

Vitest defaults set `autoAttachSubprocess: false`.
The pinned provider contains an optional subprocess path, but the repository adapters do not enable it.
Native Node environment inheritance does not prove that Vitest consumes child profiles into its self-reported LCOV. [Vitest defaults][vitest-defaults], [Vitest provider][vitest-provider], [All adapter files][angular-projects]

The Angular adapter is a transport adapter, not a membership adapter.
It cannot add disconnected original sources or repair lost mappings.
Candidate work must test explicit include patterns against Angular's bundling and remapping boundary.
Test code and helpers require separate observations from production modules. [Angular package][angular-package], [Vitest provider][vitest-provider], [All adapter files][angular-projects]

## Capability matrix

“Source capability” means inspected code supports a mechanism.
“Unproven” means this investigation lacks the required execution evidence.
No row asserts a demonstrated lack of support.

| Required case           | Native Node                          | Angular/Vitest                               | Remaining evidence                             |
| ----------------------- | ------------------------------------ | -------------------------------------------- | ---------------------------------------------- |
| Maintained production   | Source capability within manifest    | Source capability within supplied maps       | Exact original `SF` and positive `DA`          |
| Test and helper sources | No native suffix exclusion           | Subject to builder and provider membership   | Separate test/helper `SF` and executable `DA`  |
| Disconnected source     | No discovery outside manifest        | No explicit include in current configuration | Owned root to real zero-hit executable lines   |
| Missing source runfile  | Silently omitted during enumeration  | Provider filters missing original files      | Independent inventory failure                  |
| Original TypeScript     | Source-map mechanism                 | Bundled maps and post-remap filtering        | Exact known lines and canonical identities     |
| Zero-hit denominator    | Synthetic ranges within manifest     | Untested path needs explicit include         | Executable lines without false non-code lines  |
| Supported branches      | Bundled converter and LCOV support   | V8 conversion mechanism                      | Known conditional `BRDA`, taken and untaken    |
| Child-only execution    | Inherited Node environment mechanism | Optional provider path, disabled by default  | Child-only positive line in authoritative LCOV |
| Missing or corrupt data | Several silent omission paths        | Missing LCOV returns normally                | Failure despite passing functional tests       |
| Baseline merge          | Process and file merges              | Provider file merge, then copy               | Positive hits retained under one original `SF` |

The preceding sections supply sources for each matrix row.
The repository wrapper checks only that combined LCOV exists.
That check cannot establish these per-family guarantees. [Coverage wrapper][wrapper]

## Smallest feasibility experiments

Run these experiments on separate temporary research fixtures.
Use Bazel targets and the pinned Node toolchain.
Preserve manifests, runfiles identities, raw profiles, source maps, per-test LCOV, combined LCOV, and command exit status.
Do not infer runtime evidence from the matrix.

| Experiment                     | Smallest setup                                                                                    | Required observations                                                                                                   |
| ------------------------------ | ------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Native membership and baseline | One JS test, one production function, one executed helper, one unimported TS file in its manifest | Test/helper records exist. Known executable untouched lines equal zero. Non-code lines do not inflate the denominator.  |
| Disconnected root              | One declared TS library outside all test closures                                                 | Record graph separation. Test an inventory-derived reporting root. Require original zero-hit `DA`, not merely `SF`.     |
| Hidden and undeclared sources  | One eligible hidden package and one maintained source without an owner                            | Hidden owned source enters discovery. Undeclared source causes a visible failure.                                       |
| Mappings and branches          | One transpiled TS conditional with a known positive line and an untouched arm                     | Require original path and line numbers. Require supported taken and untaken branch records.                             |
| Child-only execution           | One awaited child Node process that executes its own first-party module                           | Require child profile and positive child-only line. Repeat with a cleared environment and require detection.            |
| Native data failure            | Remove one expected profile, corrupt another, and remove one manifest runfile in separate runs    | Functional assertions still pass. Completeness validation fails for each deliberate loss.                               |
| Adapter failure                | Invoke each maintained teardown with no LCOV, empty LCOV, then partial LCOV                       | Preserve normal teardown returns as observations. Require later validation to reject each incomplete report.            |
| Angular inclusion              | One tiny Angular project with a spec, helper, production module, and disconnected TS module       | Try builder `coverageInclude`. Require original records for all four roles and real disconnected zero-hit lines.        |
| Angular child                  | One Node child from the Angular test, using a child-only first-party module                       | Compare default and `autoAttachSubprocess` results. Inspect authoritative Vitest LCOV rather than only native profiles. |
| Identity and merge             | Two test targets execute different lines of one original source                                   | Merge an independent baseline. Require one repository-relative `SF` and retained hits for both lines.                   |
| Adapter directories            | Execute Predix and Tailwind with read-only runfiles and writable `TEST_TMPDIR`                    | Record whether report production fails. Compare one temporary-directory adapter under equivalent conditions.            |

These experiments test the approved contract. They introduce no percentage threshold. [Membership decision][membership]

For each successful local case, repeat the relevant action with fresh configured-remote execution.
Record actual execution, cache reuse, worker identity, fallback, output transfer, and combined report separately.
Check hosted upload against the same report and revision only after collection acceptance.
A cached report or successful upload cannot replace missing source evidence. [Membership decision][membership], [Previous evidence report][evidence]

## Candidate mechanisms and next decision

Retain native manifest conversion where experiments establish exact original records.
Use independent Bazel ownership and selected reporting roots to supply disconnected sources.
Add a language baseline adapter only when existing conversion cannot satisfy executable-line requirements. [Membership decision][membership], [Manifest generator][generator]

For Angular, investigate explicit builder inclusion before adding a second baseline engine.
Require an adapter failure for missing expected LCOV and check its contents against the independent inventory.
Normalize original identities before merging.
Require child-only evidence from the report that actually reaches combined LCOV. [Angular package][angular-package], [Bootstrap][bootstrap], [Membership decision][membership]

No specific source or rule receives a proposed exception yet.
The unresolved cases need the bounded experiments above.
The later human decision can select mechanisms after those observations.
Do not classify these cases as unsupported from source inspection alone. [Investigation decision][investigation]

## Report checks

This report changes one Markdown file.
Validation results accompany its research commit and tracker resolution.
No production source changed, so Gazelle does not apply.
No coverage acceptance result follows from formatting or prose checks.

[ticket]: https://github.com/LowkeyLab/bazel-repo/issues/2442
[membership]: https://github.com/LowkeyLab/bazel-repo/issues/2417#issuecomment-6277250438
[investigation]: https://github.com/LowkeyLab/bazel-repo/issues/2438#issuecomment-6277423566
[mechanisms]: https://github.com/LowkeyLab/bazel-repo/blob/c277c511697f5d93b0fb7bcd1f3dcae3d573c848/docs/research/2026-10-18-coverage-inclusion-mechanisms.md
[evidence]: https://github.com/LowkeyLab/bazel-repo/blob/2b0cfa271ebfbe1dcc1701574883f61e62afb702/docs/research/2026-10-18-coverage-completeness-evidence.md
[module]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/MODULE.bazel
[lock]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/pnpm-lock.yaml
[bootstrap]: https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/node-bootstrap/coverage.cjs
[generator]: https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/coverage/coverage.js#L16084
[conversion]: https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/coverage/coverage.js
[library]: https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/js_library.bzl#L254
[binary]: https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/js_binary.bzl#L906
[ng-test]: https://github.com/devversion/rules_angular/blob/bfe6e98434982bad38c393f1bd472ec33f16cd32/src/architect/ng_test.bzl
[ng-local]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/bzl/ng.bzl
[angular-config]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/angular.json
[angular-projects]: https://github.com/LowkeyLab/bazel-repo/tree/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects
[mindreadr]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/mindreadr/vitest-global-setup.ts#L14
[nicknamer]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/nicknamer/vitest-global-setup.ts#L14
[nicknamer2]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/nicknamer2-web/vitest-global-setup.ts#L14
[pinyin]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/pinyin-composer/vitest-global-setup.ts#L14
[predix]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/predix/vitest-global-setup.ts#L14
[tailwind]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/tailwind-sample/vitest-global-setup.ts#L14
[angular-package]: https://registry.npmjs.org/@angular/build/-/build-22.2.3.tgz
[angular-metadata]: https://registry.npmjs.org/@angular/build/22.2.3
[vitest-provider]: https://github.com/vitest-dev/vitest/blob/v5.0.5/packages/coverage-v8/src/provider.ts
[vitest-defaults]: https://github.com/vitest-dev/vitest/blob/v5.0.5/packages/vitest/src/defaults.ts
[wrapper]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/tools/coverage/coverage.sh
[bootstrap-loss]: https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/node-bootstrap/coverage.cjs#L56
[conversion-loss]: https://github.com/aspect-build/rules_js/blob/e67442a2ccf329841c9227d66ee013afcf5633c6/js/private/coverage/coverage.js#L15750
[mindreadr-config]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/mindreadr/vitest-base.config.ts
[nicknamer-config]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/nicknamer/vitest-base.config.ts
[nicknamer2-config]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/nicknamer2-web/vitest-base.config.ts
[pinyin-config]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/pinyin-composer/vitest-base.config.ts
[predix-config]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/predix/vitest-base.config.ts
[tailwind-config]: https://github.com/LowkeyLab/bazel-repo/blob/151e15cd495b5d63155753c0acf09f50102f0cbc/angular/projects/tailwind-sample/vitest-base.config.ts
