# Compatibility evidence

The bounded experiment passes the configured Linux compatibility gate for [Verify the Topcoat Bazel compatibility gate](https://github.com/LowkeyLab/bazel-repo/issues/2271).

## Inputs

The experiment starts from commit `74cdfa35ee413ca8dfc66928350719cc139b2f36`. It uses published Topcoat 0.10.0, the existing shared Rust 1.99.1 compiler, and Bazel 9.3.0. The npm lock selects Tailwind 4.3.3, Roboto 5.3.0, and Feather JSON 1.2.1. The font and icon serve as fixtures.

[Resolved features and checksums](resolved-features.json) record all 22 framework crates. The root `Cargo.lock` and `pnpm-lock.yaml` preserve the complete dependency graphs and integrity data. The dependency sources remain registry releases.

## Observed outcomes

| Evidence              | Observed result                                                                                                                                                                                                                      | Capture                                                                                                                  |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------ |
| Compiler and macros   | The app and independent library compile with real view, route, procedure, shard, asset, font, and icon declarations. The app links and uses the existing domain and storage crates.                                                  | [Full build](build.txt)                                                                                                  |
| Package               | The archive contains the executable and adjacent assets. Its inventory and file hashes identify the delivered bytes.                                                                                                                 | [Artifact inventory](artifact.json), [asset manifest](manifest.toml)                                                     |
| Offline generation    | Tailwind and asset packaging declare `dockerNetwork=off`. Packaging also declares `block-network=1`, rejects remote assets, and uses an empty download-cache directory.                                                              | [Action configuration](offline-actions.json)                                                                             |
| Protected requests    | The page, procedure, and shard succeed with a stored session. They deny absent credentials, malformed credentials, and a valid cookie whose session row no longer exists.                                                            | [Test output](test-results.txt)                                                                                          |
| Durable session       | Real isolated PostgreSQL retains the session. After restarting with a different configured fixture identity, the same browser credential still restores the original stored reader.                                                  | [Test invocation](test.txt)                                                                                              |
| Delivered output      | HTTP requests return the generated CSS, local font, library asset, runtime script, and rendered icon. The test extracts only the archive into a temporary directory and starts it from another directory in an isolated test worker. | [Two passing Rust tests](test-results.txt)                                                                               |
| Input changes         | Changing the library asset to version 2 and its Tailwind class to `text-emerald-800` changes the asset manifest and passes HTTP assertions for both new values. The final source restores version 1 and `text-emerald-700`.          | [Variant tests](variant-test.txt), [variant results](variant-test-results.txt), [source change](style-input-change.diff) |
| Formatting            | The composed formatter rejects source that needs formatting, formats ordinary Rust and a view macro, and converges on a second run. Full-scope formatting also succeeds.                                                             | [Formatter test](test-results.txt), [full-scope format](format.txt)                                                      |
| Repository validation | Gazelle, the focused integration target, and the full repository build succeed. One Bazel test target executes two Rust tests with no filters.                                                                                       | [Gazelle](gazelle.txt), [tests](test.txt), [build](build.txt), [exit statuses](command-status.json)                      |

The final [lint capture](lint.txt) accompanies this report. Readable log copies omit trailing whitespace and terminal control sequences. Compressed originals retain every captured byte. The full build also compiles `//tools/topcoat/compatibility:run`. Runtime evidence comes from the integration suite extracting the same archive. This session didn't start a separate interactive `bazel run` session.

## Rebuild comparison

| Archive                             | Digest                                                             |
| ----------------------------------- | ------------------------------------------------------------------ |
| Baseline and restored final package | `236d2dea6185778a7a6e8b61cbf909f8e8fa6b39412e1b847cbbe40b5286a29b` |
| Changed asset and style             | `6aff3791cb75d5693c1406c19ea241740db92e6681ad8b69223ba4c3af44d086` |

[Baseline](baseline-manifest.toml) and [variant](variant-manifest.toml) manifests preserve the changed content hashes. The restored archive matches the baseline archive hash.

## Integration findings

The initial dependency build failed because Topcoat's asset metadata retained an absolute `CARGO_MANIFEST_DIR`. The repository's Rust wrapper correctly rejected that artifact. A normal location expansion also reintroduced an absolute path. A relative dependency annotation and declared caller environment files fix the metadata while retaining the Rust guard. [Original diagnostic](initial-path-failure.txt).

The first shared-source CSS action conflicted with Rust's source staging. A separate declared source directory fixes that conflict. Tailwind's explicit source directives restrict scanning to the app and library inputs. [Original staging diagnostic](initial-staging-failure.txt).

These fixes retain the agreed public tooling form, release, compiler, and registry dependency workflow. The experiment doesn't change production dependencies or deploy a service.

## Limits

This evidence covers the configured Linux profile and selected view/font formatting profile. It doesn't certify renamed dependencies, other platforms, `font-fontsource` or mail formatting, live identity or catalog providers, browser cookie transport policy, the complete expiry/revocation/isolation suite, deployment, or production readiness. The larger app contract and product implementation remain separate work.
