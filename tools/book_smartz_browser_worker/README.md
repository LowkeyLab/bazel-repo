# Book Smartz browser worker

This tooling image combines the root Playwright package's Chromium installation
with Docker, `containerd`, `runc`, and `iptables` for BuildBuddy Firecracker workers.
It contains no Book Smartz app or credentials. Publication and browser
acceptance enablement follow in tickets #2372 and #2373.

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
The app browser target and normal fallback policy remain unchanged.

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
