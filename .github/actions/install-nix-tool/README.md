# Repository Nix tools in CI

`install-nix-tool` installs `aspect` or `lcov` from this checkout's locked
flake. It returns the exact store `path` and adds its `bin` directory to
`GITHUB_PATH`. It accepts public `cache-name` and `public-key` inputs only;
readers receive no Cachix upload credential. Missing public configuration
warns and permits a source build. Partial or malformed configuration fails
with a configuration error. Nix signature verification remains enabled.

`prepare-nix-tools.yml` builds the selected package first, without the upload
token or unrelated CI credentials. Only its publication step receives
`CACHIX_AUTH_TOKEN`. Main pushes and PRs whose head and base both belong to
this repository may publish, including Renovate PRs. Forks only read.
Configure repository variables `CACHIX_CACHE_NAME` and `CACHIX_PUBLIC_KEY`
with the real public cache name and its independently verified signing key,
and the scoped repository secret `CACHIX_AUTH_TOKEN` for uploads.

After publication, the helper enumerates the local runtime closure and
checks each path's remote NAR hash and references against Cachix or
cache.nixos.org. Aspect's root must exist on Cachix itself. Cachix omits
paths already on the upstream cache, so checking only Cachix recursively
would incorrectly reject some complete closures. The helper writes the
chosen source, signatures, hash and references into
`$RUNNER_TEMP/nix-TOOL-closure.json`. This metadata check is not the fresh
signed download acceptance test. Ordinary build success never asserts that
a tool was substituted.

An HTTP 404 is diagnosed as a cache miss. Connection errors, other HTTP
errors, failed downloads with present metadata and failed uploads are
reported separately. Ordinary cache/publication failure warns and permits
source fallback; an evaluation or tool-build failure exits unsuccessfully.
Preparation must finish before consumers fan out, including on a degraded
cache run. Degraded runs explicitly do not claim a verified warm cache.

Configured readers use a 5 second connection timeout, 30 second stalled
download timeout and two download attempts. Diagnostic HTTP probes use a
5 second connection timeout, 30 second maximum per attempt and one retry.
These settings limit individual transfers, not the whole closure or source
build. Preparation has a 90 minute Actions job limit; hitting that limit
cancels the job instead of guaranteeing fallback. Uploads use a 15 minute
process limit. Actual outage elapsed times and fresh Actions substitution
remain pilot acceptance evidence to collect after provisioning.

## Strict pilot acceptance (ticket #2244)

The manual **Nix cache acceptance** workflow uses a fresh hosted runner per
package and the same installer with `install-only: 'true'`. This installs Nix
without building the requested package. No store cache is restored and no
upload, Aspect-service, BuildBuddy or Docker secret is supplied. It never
invokes coverage or production deployment.

[GitHub requires the manual workflow to exist on the default branch](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)
before `workflow_dispatch` is available. An unmerged PR adding this file does
not by itself provide a dispatchable hosted check. Once the file exists on
main, `gh workflow run nix-cache-acceptance.yml --repo LowkeyLab/bazel-repo
--ref REVISION` selects the intended branch revision. Until that integration
boundary is authorized and completed, retain the executable helper and
structural checks as preparation only; report hosted acceptance as blocked.
Do not imply that draft PR publication bypasses workflow discovery.

The command `python3 .github/scripts/nix_tool.py strict aspect` (or `lcov`)
requires real public cache variables, evaluates the exact locked output and
rejects a preexisting output rather than deleting it. It realizes only that
raw non-derivation store path with local jobs and remote builders disabled,
source fallback disabled and signature verification required. Nix's raw-path
realization contract fetches its runtime closure; it does not build a known
deriver. Ordinary consumers retain their existing source fallback behavior.

Artifacts retain Nix version, effective configuration, exact evaluation,
initial valid paths, full realization stdout/stderr, command statuses and
elapsed times, closure metadata, remote availability and executable smoke
checks. The provenance gate reads Nix internal JSON substitution activity
records (type 108, path and source fields), rejects build activities (105),
and fails on missing, malformed or ambiguous source evidence. A substitution
start alone is an attempt: acceptance also requires command success, output
validity, an initially absent output and successful executable checks. All
attempts for a newly downloaded path must name the same source. Aspect's root
must name the intended Cachix URL; lcov and other closure paths may accurately
name cache.nixos.org. Dependencies already present with the installer are
recorded as preexisting, never claimed as downloaded. Remote metadata query
sources are stored separately and are not download provenance. A changed
Nix logging schema requires investigation; absent evidence does not pass.

Do not run the strict command in a credential-bearing environment. It rejects
Cachix credentials and clears Nix access-token, netrc and signing-key providers
for read-only evidence collection. Reusing a developer store can intentionally
fail the absence gate; the hosted workflow is the acceptance seam.

### Provisioning and evidence still required

A maintainer must create a public cache under their own Cachix account using
the free allowance and managed signing, confirm the actual available name and
public key, and provide one write token restricted to that cache. Set Actions
variables `CACHIX_CACHE_NAME` and `CACHIX_PUBLIC_KEY` and Actions secret
`CACHIX_AUTH_TOKEN` in LowkeyLab/bazel-repo. Never put the token in chat,
tracker text, logs or a public variable. Public metadata cannot prove ownership;
the maintainer must compare it against their account. No purchase, paid trial,
custom retention, private signing key or production deployment is required.
These steps are a setup handoff, not a claim that provisioning occurred.

After authorized provisioning, collect this matrix with revision, exact paths,
run URL, attempt, timestamps and result. Every row remains **pending live
execution** until linked evidence exists:

| Evidence                                   | Required observation                                                                                  |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| Main preparation                           | Successful upload and recursive remote availability before consumers                                  |
| Fresh download                             | Both strict matrix jobs pass; Aspect root from intended Cachix; actual lcov/closure sources           |
| Warm fan-out, twice                        | Concurrent format/lint/test consumers, same warmed output, no Aspect compilation or ResourceExhausted |
| Changed-output same-repository PR          | Changed exact path published and remotely available before consumers                                  |
| Renovate same-repository PR                | Same publication behavior observed for actual bot event                                               |
| Fork public reader                         | No upload token or upload attempt; signed public download succeeds                                    |
| Coverage installation                      | Exact repository lcov, genhtml and geninfo run; separate workflow retained                            |
| Deployment installation                    | Exact repository Aspect runs through installer; no production deployment                              |
| Missing output and unavailable substituter | Strict failure; ordinary fallback diagnostics and measured elapsed time                               |
| Failed publication/build                   | Upload failure reported distinctly; genuine build failure remains fatal                               |

The manual workflow supplies the fresh-download row only. It neither warms a
cache nor proves event guards, preparation ordering or the broader live matrix.
For coverage/deployment installation, exercise just the shared action with the
appropriate tool on a hosted runner; do not dispatch a production deployment
workflow merely to gather this evidence. Keep #2244 open until the complete
live matrix is met. Record compressed occupancy and actual available download
and eviction observations separately under #2245; uncompressed closure sizes
and client transfers are not account-wide storage or traffic measurements.
