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
