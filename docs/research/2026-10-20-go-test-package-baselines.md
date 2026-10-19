# Go baselines for unexecuted test packages

## Result

The pinned baseline builder supplies canonical executable-line records for internal and external test sources without compiling or executing their test packages.
Mixed package names work in one invocation. Separate package invocations produce the same records for these fixtures.
The experiment therefore needs no different baseline adapter or separate package grouping to obtain these line records.
Production integration still needs an adapter that attaches builder output to test owners. This research does not choose that final mechanism.

This report resolves [Determine whether Go baselines can include unexecuted test packages](https://github.com/LowkeyLab/bazel-repo/issues/2446).
It extends the [previous source investigation](https://github.com/LowkeyLab/bazel-repo/blob/e44f292165c33f3381c3b649a848158c008e50f1/docs/research/2026-10-19-python-go-expanded-coverage.md).
It approves no exception and changes no production collector.

## Revision and configuration

The acquired revision is `5e246a47cd7ea0a9c506e915db0c053d7d84d023`.
Its [module configuration](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/MODULE.bazel) pins rules_go to `20008e8ca02add2d4ae2f3edf514d94afa3ee0be`.
Its [Go module](https://github.com/LowkeyLab/bazel-repo/blob/5e246a47cd7ea0a9c506e915db0c053d7d84d023/go.mod) selects SDK `1.26.4`.
Bazel is `9.3.0`. The experiment uses the repository toolchain for `linux/amd64` with `CGO_ENABLED=1`.
The action environment, arguments, and SDK paths remain in each case's evidence.

The [experiment rule](go-test-baseline-experiment/experiment.bzl) obtains the pinned builder, covdata, SDK tools, and environment from `go_context`.
It invokes the builder's `baselinecoverage` command directly. It creates no test execution action or package compilation action.
The rule captures builder failures as evidence. Thus, successful Bazel orchestration does not mean each builder invocation succeeded.

The previous worker executed this command before the interruption:

```bash
nix develop --command bazel build //docs/research/go-test-baseline-experiment:{mixed,internal,external,declarations,absent_metadata,invalid} --spawn_strategy=local
```

The [build log](go-test-baseline-experiment/evidence/experiment-build.log) records seven local actions and the [BuildBuddy invocation](https://app.buildbuddy.io/invocation/138ea106-3733-45d8-ba85-0f989d0fa30c).
The six baseline actions and one invalid-input copy action account for those local actions. Tool dependencies can use cached artifacts.
This establishes local execution. It establishes no configured-remote coverage run, hosted upload, or combined LCOV acceptance.

## Executed results

Each case retains `arguments.txt`, `stdout.txt`, `stderr.txt`, `exit-status`, and a metadata manifest.
Published evidence replaces machine paths with `<LOCAL_PATH>`, temporary roots with `<TMPDIR>`, and temporary builder directories with `<WORKDIR>`.
These substitutions do not change source identities, meaningful flags, coverage data, or exit statuses.
The metadata manifests describe the published archive contents. Private runtime storage retains the original evidence.
Its `work.tar.gz` preserves the builder's working files, including coverage metadata and instrumented temporary outputs.
These temporary outputs were never compiled. The archive protects generated files from repository formatting.

| Case                                | Builder exit | Observation                                                                                               | Raw evidence                                                                                                                                                           |
| ----------------------------------- | ------------ | --------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Mixed internal and external sources | 0            | Both source paths contain `DA:3,0`, `DA:4,0`, and `DA:5,0`. The declaration has `LF:0`.                   | [Mixed LCOV](go-test-baseline-experiment/evidence/mixed/baseline.lcov)                                                                                                 |
| Internal package separately         | 0            | Its executable records equal the mixed invocation's internal records.                                     | [Internal LCOV](go-test-baseline-experiment/evidence/internal/baseline.lcov)                                                                                           |
| External package separately         | 0            | Its executable records equal the mixed invocation's external records.                                     | [External LCOV](go-test-baseline-experiment/evidence/external/baseline.lcov)                                                                                           |
| Declaration-only source             | 0            | The original path has `LF:0` and no `DA`. Its emitted metadata file exists with zero bytes.               | [LCOV](go-test-baseline-experiment/evidence/declarations/baseline.lcov), [metadata manifest](go-test-baseline-experiment/evidence/declarations/metadata-manifest.json) |
| Build-constraint exclusion          | 0            | `excluded.go` appears in arguments but has no LCOV source record or instrumented output.                  | [Mixed arguments](go-test-baseline-experiment/evidence/mixed/arguments.txt), [metadata manifest](go-test-baseline-experiment/evidence/mixed/metadata-manifest.json)    |
| Invalid syntax                      | 1            | The real cover tool rejects the generated invalid Go input. No baseline exists.                           | [Invalid stderr](go-test-baseline-experiment/evidence/invalid/stderr.txt)                                                                                              |
| Absent metadata                     | 1            | A successful no-op cover tool emits no metadata. The builder rejects the absent file. No baseline exists. | [Absent-metadata stderr](go-test-baseline-experiment/evidence/absent_metadata/stderr.txt)                                                                              |

The [internal fixture](go-test-baseline-experiment/internal_test.go) and [external fixture](go-test-baseline-experiment/external_test.go) each contain one unused function.
Their bodies return constants. Lines 3 through 5 contain the signature, return, and closing brace.
The real Go cover tool supplies those exact block spans. No approximation from source line counts supplies the denominator.
All successful source records retain the `docs/research/go-test-baseline-experiment/` prefix.
They contain no absolute path, generated-output path, or import-path substitute.

The [declaration source](go-test-baseline-experiment/declarations.go) contains only a type and constant.
Its dedicated invocation independently distinguishes valid empty metadata from missing metadata.
The missing-metadata case replaces only the cover executable with a successful no-op. The pinned builder still checks metadata existence.
The invalid-input case uses the real SDK cover executable. These are separate failure cases.

## Source explanation and integration limits

The pinned [baseline implementation](https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/tools/builders/baseline_coverage.go) supplies the observed behavior.
`baselineCoverage` applies build constraints before instrumentation. `coverableLines` sends the sources to cover with `Local: true` and `EmitMetaFile`.
`baselineLCOV` emits the original supplied paths. It writes one zero count for each decoded executable line.
An existing empty metadata file returns a valid empty line set. An absent metadata file returns an error.

The mixed invocation supplies `PkgName` from the first source. The cover tool nevertheless accepts both fixture package names and emits both line sets.
Separate grouping is therefore unnecessary for these baseline observations. This does not establish that mixed package sources can compile as one Go package.

The pinned [test rule](https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/private/rules/test.bzl) still lacks the library rule's custom baseline attachment.
The [existing baseline helper](https://github.com/bazel-contrib/rules_go/blob/20008e8ca02add2d4ae2f3edf514d94afa3ee0be/go/private/actions/baseline_coverage.bzl) remains a candidate integration path.
This experiment tests builder reuse. It does not implement test-owner metadata, root discovery, inventory validation, or measured-report merging.

The result covers the selected SDK, platform, and simple internal/external source groups.
It does not establish every build tag, platform, cgo case, generated-source exclusion, duplicate owner, or embedded-source integration.
Those limits do not invalidate the demonstrated builder capability. No unsupported-source exception follows from them.

## Resume checks

The resumed review checked all six retained evidence sets without executing tests or repeating the experiment.
Exact zero-hit records, relative source paths, builder exit statuses, and every published archive hash passed.
The report passed the strict-English structural check.
Repository formatting passed with two jobs, bounded CPU and memory, and disabled remote executor, cache, downloader, and BES services.
These checks do not establish a production build, lint result, hosted CI result, or collector acceptance.
