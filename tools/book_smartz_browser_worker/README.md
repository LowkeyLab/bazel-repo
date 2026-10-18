# Book Smartz browser worker

This tooling image combines the root Playwright package's Chromium installation
with Docker, `containerd`, `runc`, and `iptables` for BuildBuddy Firecracker workers.
It contains no Book Smartz app or credentials. The [browser acceptance target](../../book_smartz/web/README.md)
pins the anonymously verified digest and uses the bundled browser on Firecracker.

## Build and validate

Use the repository Nix shell. GnuPG verifies the Ubuntu snapshot's signed package
indexes during Bazel module resolution. Restart a pre-existing Bazel server after
entering the updated shell if it can't find `gpgv`.

```bash
nix develop --command aspect build //tools/book_smartz_browser_worker:image
nix develop --command aspect test //tools/book_smartz_browser_worker:runtime_test --test_output=errors
nix develop --command bazel build //tools/book_smartz_browser_worker:image.digest
cat bazel-bin/tools/book_smartz_browser_worker/image.json.sha256
```

The explicit runtime target needs a Linux Docker host that permits privileged
containers. It's manual because generic remote executors can't launch its
outer privileged container. Failures are errors, including unavailable Docker.
The test starts a fresh nested daemon with the `vfs` storage driver, loads the
existing `//tools/test_images:postgres_18_tar` archive, starts PostgreSQL without
pulling, and checks a SQL result. While PostgreSQL runs, the repository's Bazel
Playwright package launches the image's bundled Chromium and checks page content.
The outer container has no network access or local browser override. A test-only
derived image adds the Bazel-managed Node runtime and probe. The worker image
itself contains only tooling. Docker removes the outer container after execution.

This validates image prerequisites. It doesn't establish remote execution,
anonymous GitHub Container Registry access, the full app journey, or production readiness.
The app browser target uses this worker. Normal fallback policy remains unchanged.

## Runtime evidence

The validation passed with Docker 29.1.3, `containerd` 2.2.1, `runc` 1.3.4, and
Chromium `156.0.8078.4` at `/ms-playwright/chromium-1248/chrome-linux64/chrome`.
PostgreSQL ran from the declared archive and returned `42` for `SELECT 6 * 7`.
The first runtime attempt failed because extracting the `iptables` package omits
its alternatives links. The image now provides those links explicitly.

Worker manifest digest:
`sha256:6fdbf236c09d41b4bc82d140ea6f5510eb27de20d422f4fa88b64cf59413990f`.
[Passing prerequisite run](https://app.buildbuddy.io/invocation/4cc13977-a613-46b0-9eab-7ba8baac82f0)
executed the runtime test locally. Its image build used remote actions.

## Pinned inputs and updates

- Playwright package: `1.64.1`, from the root pnpm catalog and lockfile.
- Browser base: `mcr.microsoft.com/playwright:v1.64.1-noble`, pinned to
  `sha256:7852dc5a7d72f5fe458a597d690f6c42fbc8e9e26ddfa95f8e6ef552558d9bbf`.
- Architecture: Linux amd64.
- Docker packages: the signed Ubuntu Noble snapshot at
  `https://snapshot.ubuntu.com/ubuntu/20261017T000000Z`, including updates and
  security, with main and universe components. Snapshot indexes and package
  digests fix the transitive package closure; `MODULE.bazel.lock` records the
  resolved versions, URLs, and `SHA-256` checksums.
- Ubuntu archive signing keyring: `SHA-256`
  `80a36b0a6de2f69f49d2df75ef473ccde121e9e190b9ea01d20a4f63778d5c31`.

When the Playwright catalog changes, resolve the matching official Noble image
digest and update its `MODULE.bazel` pull. When updating Docker, choose a signed
immutable Ubuntu snapshot, resolve the package closure, and refresh `MODULE.bazel.lock`. Run the runtime target again and record
the resulting worker digest before publication. Never enable the app
target against an unavailable worker digest.

[BuildBuddy custom images](https://www.buildbuddy.io/docs/rbe-microvms/#using-custom-images)
require installed Docker before daemon initialization.
[Playwright Docker guidance](https://playwright.dev/docs/docker) requires a matching
package and browser image.

## Publish independently

The manual `Publish Book Smartz browser worker` workflow publishes only
`ghcr.io/lowkeylab/book_smartz_browser_worker`. It has no app deployment or
browser-journey dependency. Aspect app delivery excludes its `push_image`
target, so the ordinary deployment gate can't publish it incidentally.

First merge the workflow and publisher onto `main`, with repository checks and
review satisfied. A new dispatch workflow must exist on the default branch before
its first dispatch. Then run:

```bash
gh workflow run publish-browser-worker.yml --repo LowkeyLab/bazel-repo --ref main
gh run list --repo LowkeyLab/bazel-repo --workflow publish-browser-worker.yml
```

Publication uses the workflow's short-lived `GITHUB_TOKEN` with `packages: write`,
through the existing `oci_push` rule. It needs no new personal access token or registry secret.
Hosted publication uses the runner’s Bazel setup and installed `gpgv`, rather than
the developer shell’s Linux namespace wrapper. The publisher reruns the isolated
Docker/PostgreSQL/Chromium runtime test, builds
the worker digest, and tags the artifact `sha-<publishing-commit>`. Its summary
records the publishing commit and validated digest. Consumers must use the digest,
not the source tag. The runtime probe image is never published.

[GitHub Container Registry creates new packages as private](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry).
Check visibility after first publication. If the package is private, an administrator
must open the
[worker package settings](https://github.com/orgs/LowkeyLab/packages/container/book_smartz_browser_worker/settings)
and set its visibility to Public. Keep the package linked to this repository and
allow its Actions workflows to write. An authenticated push doesn't establish
anonymous access. If initial anonymous verification fails because the package is
private, change visibility and rerun the workflow. Don't enable the consumer while
verification fails.

The separate anonymous-access job has a fresh runner, no registry login, no persisted
checkout credential, and an empty Docker config. Bazel-managed `crane` retrieves
the exact digest and all image content into a fresh image layout in the Open Container Initiative format. The job checks
that the requested manifest exists and verifies every downloaded blob against its
`SHA-256` filename, including the manifest, configuration, and layers. This layout
preserves uncompressed layer media types. The pinned `crane validate` command
assumes `gzip` layers and can't validate this worker. The summary records the commit, digest, and downloaded bytes.
Keep the successful workflow link alongside the consumer's digest. A manifest
lookup alone, a publisher credential, or a cached Docker image isn't sufficient.

After an input update, rerun local runtime validation, merge the reviewed inputs,
dispatch publication on `main`, and require full anonymous verification before
updating the consumer digest. Keep the root Playwright catalog and lockfile,
`MODULE.bazel` browser image, signed Docker snapshot and `MODULE.bazel.lock`, and
consumer digest aligned in the pinned-input instructions. Record the new publishing commit,
runtime result, immutable digest, and anonymous verification run together.

Publication validates worker prerequisites and anonymous availability. The
[focused remote and local smoke commands](../../book_smartz/web/README.md) exercise
the full app journey. Use the hosted procedure below to establish execution from CI.

## Combined worker decision and hosted verification

Use one combined Chromium and Docker worker on Firecracker. The browser journey,
packaged app, controlled providers, and real PostgreSQL fixture stay together.
The consumer pins the public manifest digest in the runtime evidence. It doesn't inherit
host browser paths. Keep normal CI caching and fallback. The fresh probe below
is an explicit acceptance operation, rather than a requirement for every run.
This decision implements [the approved worker specification](https://github.com/LowkeyLab/bazel-repo/issues/2370).

The ordinary [hosted run at the consumer rollout commit](https://github.com/LowkeyLab/bazel-repo/actions/runs/39096595875)
passed all 155 tests from cache at `d40dac386ac7e33298a5d6d92173ec3e1c0c0839`.
It establishes CI compatibility, but doesn't establish fresh browser execution.
For hosted execution evidence, merge the reviewed workflow onto the default
branch, then dispatch the opt-in probe at the delivered ref:

```bash
gh workflow run run-tests.yml --repo LowkeyLab/bazel-repo --ref main -f browser-acceptance=true
gh run list --repo LowkeyLab/bazel-repo --workflow run-tests.yml --event workflow_dispatch
```

The test job first runs the ordinary suite with its existing policy, then runs
the complete browser journey with test caching, remote action-cache reuse, and
local fallback turned off. The overrides apply only to the focused command.
The job summary records the actual checkout commit, hosted run, and a dedicated
BuildBuddy invocation. Missing worker prerequisites or failed assertions fail
the probe. An invocation link alone is insufficient evidence.

Open that invocation's execution list and select the browser test action for
`//book_smartz/web:browser_test`: mnemonic `TestRunner`, command ending in
`test-setup.sh book_smartz/web/browser_test`. Ignore the separate XML-generation
action. Require exit code zero, `runner=remote`, `cacheHit=false`, the exact
public `container-image` digest, `workload-isolation-type=firecracker`, and
`init-dockerd=true`. Inspect the completed execution's worker identifier and
execution timestamps. The execution request must have `skipCacheLookup=true`.
Record the action/execution link alongside the invocation and hosted run in the
delivery ticket, with the tested commit and worker digest. If the ordinary run
used cached results or fell back, its green result can't replace this probe.

Keep these evidence boundaries separate:

| Evidence                                                                                                | Establishes                                                                                                                   |
| ------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Worker runtime target                                                                                   | Fresh nested Docker, declared PostgreSQL archive, and bundled Chromium work together.                                         |
| [Publication and anonymous full pull](https://github.com/LowkeyLab/bazel-repo/actions/runs/39092168629) | Public digest availability and integrity of every image blob at publishing commit `6f6a80e3e0862535a7aa8f9c782c4a91525bce39`. |
| [Focused remote journey](https://app.buildbuddy.io/invocation/2add1ffc-8353-4415-81bf-4d606992d949)     | The full app journey executed remotely at source `07ba33931e7032f2ca65c7078de2bd48c1d4b464`.                                  |
| [Explicit local Nix smoke](https://app.buildbuddy.io/invocation/83a23410-0546-4bfc-8e1e-0728d3de98d7)   | Local browser compatibility for the consumer rollout.                                                                         |
| Hosted probe and browser execution metadata                                                             | The delivered checkout executed the full journey remotely from GitHub Actions.                                                |

After an input update, repeat publication, anonymous access, local smoke,
focused remote acceptance, and hosted execution verification for the new digest.
The controlled journey doesn't establish live provider integration or production
launch readiness.
