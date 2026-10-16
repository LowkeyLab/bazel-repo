# Bazel dependency restore pilot

The test job restores public dependency archives and eligible repository-rule
contents into `$HOME/.cache/bazel-repo`. Bazel uses its native checksums and
repository-input fingerprints. No output base, action cache, credentials, or local
developer settings belong in this snapshot.

The compatible prefix is `bazel-deps-v1-<OS>-<arch>-`. A run ID and attempt identify
the primary key; prefix matching follows GitHub's normal ref visibility. Lockfiles,
revision, workload, and exact Bazel version do not partition this identity.

Only the test job consumes this pilot. Publication is disabled; authorized saves
and measured pilot acceptance are separate work in #2304. Wider rollout depends on
that acceptance gate (#2305). BuildBuddy, Cachix, and Bazelisk caching are preserved.
The setup action explicitly disables duplicate repository persistence.

Restore uses the standard pinned `actions/cache/restore` action with a two-minute
step timeout and advisory failures. An absent matched-key output is unconfirmed
state even if the action reports success: the action can catch transport errors.
Unconfirmed state is removed before `bazel fetch --config=ci //...`. Genuine fetch
or integrity failures propagate and prevent the normal tests from starting.
Preloading does not guarantee exhaustive or offline coverage; workloads can fetch
additional dependencies lazily.

Preparation prints JSON and a job summary containing the action outcome, selected
snapshot, exact/prefix classification, cleanup, duration, exit status, and disabled
publication. No-match results can mean either a miss or an internally caught
transport error; inspect the restore step's warnings to distinguish them. The
step outcome identifies failures, including deadline cancellation; GitHub's raw
step log retains the timeout diagnostic.

The acceptance workflow exercises the same pinned restore action against isolated
loopback endpoints for misses, HTTP errors, and real stalls. It uses a dummy token,
no service secrets, and no cache publication. Evidence requires an observed request,
actual two-minute cancellation for the stall, incomplete-state cleanup, successful
upstream fetch, and native checksum rejection propagated through preparation.
These fixtures cover transport and integrity behavior, not real GitHub cache
visibility, repository-content trust, useful reuse, or measured savings.
Credential-free evidence is uploaded even on failure. Local process-boundary tests
cover confirmed hits, unconfirmed restores, unrelated-cache preservation,
configuration, and preparation exit status.

To roll back, remove the configure, restore, and prepare steps from the test job;
normal tests then fetch dependencies as before. Do not delete unrelated caches.
For a suspected incompatible or untrusted snapshot, disable consumption first, or
change the shared schema prefix deliberately. Resume only after establishing a
fresh trusted baseline. Native checksums do not establish trust in executable
repository contents.
