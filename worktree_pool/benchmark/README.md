# Worktree pool benchmark

This experiment applies the acceptance policy in GitHub issues #2064 and #2070.
It measures the selected frozen pool executable. Its source revision and digest
identify the implementation under measurement. Software test results and later
source changes don't establish performance or adoption.

## Boundary and dependencies

The runner owns private local Git clones, worktrees, output bases, and catalog.
It uses real Git, the pool executable, `redb`, the filesystem, Nix, and Bazel.
BuildBuddy, archive caches, the kernel page cache, network, and host resources
form shared dependencies. Disclose their conditions and attribution limits.
Preserve preexisting caches, worktrees, processes, and refs. Ignore inherited
`WORKTREE_POOL_*` and `GIT_*` controls. Pool Git and disposable Git use the same
intended policy without global or system Git configuration. Pool authority uses
only the experiment's private data, state, and configuration directories.

The report evaluator accepts measurements as values and returns the gate
assessment. Worked examples protect that decision without timing or mocking
the host. Concrete orchestration exercises the real dependencies. Neither
module needs a new production interface.

## Protocol

Use Linux x86_64, the pinned Nix shell, and `//nicknamer/server/lib:lib`.
Set `RUN_ROOT` to a new private directory, `SOURCE` to a read-only checkout,
`POOL_BINARY` to the selected retained executable, and `TOOL_COMMIT` to its
source commit. Record the executable digest. Both arms refresh a private local
origin with fixed `main` on every acquisition. Local transport excludes GitHub
latency. Each pair must resolve matching commits.

Use the same tracked Bazel configuration, BuildBuddy remote cache, archive
cache, and native Nix/Aspect commands. Both arms turn off the fetched-directory
cache with `--repo_contents_cache=` to bound experiment disk growth. Each pool
slot retains an independent output base. Each disposable caller gets a fresh
checkout and output base. Run disposable cleanup and server shutdown after
measurement. Product release preserves retained state.

Warm all four pool slots at the base revision. Warm every slot through a small
source-edit cycle, then warm one disposable cycle. Record warmup durations and
resource growth. For every workload, run two batches of ten pairs and alternate
which arm runs first. Retain every valid sample and record failures separately.
Failed attempts never count as successful fast cycles.

Measure monotonic intervals for acquisition, preparation, build, total, and
release. Total starts before refresh/acquisition and ends at build completion.
Report caller commits before release, release, disposable cleanup, setup, and
warmup separately.

Cover unchanged repeats, small source edits, revision changes, revision returns,
and two simultaneous callers. Edit the same literal in
`nicknamer/server/lib/src/lib.rs` in both arms. Immediately run Gazelle, then
full-scope formatting, then build. Commit edited work before pool release.
Before paired sampling, acquire, build, and release one pooled slot at base.
Record this setup cycle and its resource growth separately: warmup edits leave
caller commits, so returning to base must occur before an unchanged measurement.
For every unchanged pooled cycle, verify the selected pre-acquisition `HEAD`
is already the requested base. A mismatch fails the run and can't count as a
successful unchanged pair. For revision-change/return requests, prepare the
opposite revision before each measured cycle. Record all preparation cycles
outside paired timing.
For two callers, use a start barrier and separate output bases. Retain both
caller intervals and wall time until both builds finish. Defer release until
both builds finish.

## Assessment

Both primary workloads require strict pooled wins in at least eight of ten
pairs in each batch. Ties aren't wins. For a secondary workload, eight strict
pooled slowdowns in both batches establish a regression. Assess both caller
positions and total completion time for two callers.

Batch disagreement or incomplete/invalid evidence means **hold**, taking
precedence over a performance **no-go**. Consistent failure of either primary,
a secondary regression, or rejected costs means **no-go**. Complete favorable
measurements remain **hold** until the maintainer explicitly accepts savings
and resource costs. This consistency policy doesn't establish statistical
confidence. Packaging requires a separate satisfied adoption gate.

Report medians, nearest-rank p95, minimum/maximum, sample standard deviation,
and absolute and relative paired savings. At four slots, report allocated and
apparent checkout/output bytes, idle server resident memory, catalog file size
and event-count growth, and disk/RAM headroom. Separate preexisting shared cache
and Nix-store usage from owned state. Resident memory includes shared physical
pages, so summing it doesn't establish unique physical usage. Keep missing
measurements unknown.

## Reproduction

Obtain a coordinator-approved quiet window, then run:

```bash
nix develop --command bazel run //worktree_pool/benchmark:run -- \
  --phase warmup \
  --root "$RUN_ROOT" \
  --source "$SOURCE" \
  --pool-binary "$POOL_BINARY" \
  --fixture-commit c0dd770d87a1392bb19f8970d5f783c3631981db \
  --tool-commit "$TOOL_COMMIT" \
  --quiet-window "$COORDINATOR_WINDOW"
```

Warmup exits after recording four-slot and first-disposable costs, before any
paired samples. Review these observations and actual headroom with the
coordinator. Choose documented reserves, then repeat with `--phase sample`
and separate sampling authorization. Sampling requires the same complete,
unused warmup and executable/fixture provenance. Warmup refuses an existing
directory. Sampling validates the protocol version and runner source digest,
as well as executable and fixture provenance, before writing sample metadata.
It refuses old or incompatible warmup proof and any previously started sequence.

The runner writes metadata, command records, compressed sanitized logs, headroom,
warmup, setup cycles, every successful cycle, completed pairs, separate failures,
resource observations, and the report. It replaces private paths with
`$RUN_ROOT`, `$SOURCE`, `$POOL_BINARY`, and `$USER_HOME`. It omits private pool
response path byte arrays and records configuration digests and runtime versions.

Retain each requested revision, selected worktree ID, and pre/post-acquisition
`HEAD`. Read existing-slot tips before timing. A revision-change/return request
can select another slot already at the target because production ranks target
matches first. Distinguish requested sequences from actual checkout changes.

Disposable callers share a private repository context and serialize refreshes
to avoid racing Git ref updates. Include waiting time in acquisition. Pooled
callers use the tool's real repository coordination. Rank the two caller
positions by elapsed completion duration as faster and slower. Retain both
clocks and their overall wall interval.

Experiment guards default to eight GiB free disk and two GiB available RAM.
Before creating state, reserve the largest observed private checkout/output
footprint and idle server resident memory per new caller. Record observations,
selected guards, and any resulting hold. These guards don't establish product
quotas or resource bounds. Never discard successful samples, silently change
settings, or clean user caches to produce a winner.

## Adapter regression checks

Run `nix develop --command aspect test //worktree_pool/benchmark:adapter_test`
for private Git/catalog orchestration checks using the production command-line
tool as native test data. Test support omits native build and server-resource
effects. Its intervals aren't performance measurements. These checks cover isolated authority
and Git policy, actual retained-tip selection, invalid unchanged preconditions,
and incompatible warmup proof. The gate tests protect assessment semantics.
