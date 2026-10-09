# Contributing to worktree pool

[README.md](README.md) covers installation, commands, and caller safety obligations.
This guide covers repository development, implementation, and verification.
Read the repository-root `AGENTS.md` before changing this monorepo. Development
commands below run from the repository root, including for source-package contributors.

## Development workflow

Use the repository's Nix shell and Bazel tools for development:

```sh
nix develop --command bazel run //:gazelle
nix develop --command aspect format --scope=all
nix develop --command aspect test //worktree_pool:worktree_pool_lib_test //worktree_pool:cli_test
nix develop --command aspect build //...
nix develop --command aspect lint
```

Run Gazelle immediately after each source edit and before formatting or manual
`BUILD.bazel` changes. The command-line integration target requires the development
shell's real Git on PATH. Fixtures own private catalogs, repositories, and remotes.
Use the installation acceptance target below to verify the standalone package.
Cargo commands in the README are installation instructions. Repository development
uses Bazel.

Implementation reference docs include [command schemas](WIRE_SCHEMA.md) and
[dependency features](DEPENDENCIES.md). Root Cargo.lock is the Bazel dependency
authority. The standalone manifest and lockfile define source distribution.

## Distribution and adoption scope

Development uses Nix and Bazel. The standalone Cargo package and executable use the
name `worktree-pool`, version 0.1.0, with exact dependencies and Rust 1.96+ source
installation. Runtime support is Linux x86-64 with Git 2.36+ and ordinary local
linked-worktree repositories. See [installation](README.md#standalone-source-installation),
[command schemas](WIRE_SCHEMA.md) and [dependency features](DEPENDENCIES.md).
The maintainer accepted the measured adoption gate on 2026-10-09. Its scope and
retained costs appear in [the benchmark evidence](#accepted-benchmark-and-resource-costs).
Crate publication and production rollout are separate maintainer actions.

## Catalog initialization

Initialization persists the catalog identifier and pending locator before
creating `redb`, then publishes the active locator through an atomic rename.
Interrupted initialization remains pending. Repeating initialization can't
overwrite catalog data or repair pending work. Information and validation
commands examine locator identity, store/projection versions, complete event
history, contiguous revisions, deduplication index, and projection equivalence.
Unsupported history stops use.

## Projection rebuild

Check reads storage without changing its bytes and requires the stored projection
and identity index to match complete supported history. Rebuild explicitly reconstructs both derived records
from every immutable event in committed position order. Its version-1 result uses
command `catalog rebuild`, the catalog context, and the same projection-shaped
`data` as catalog information. The authoritative revision stays fixed. Rebuild is
catalog-wide and rejects a repository selector.

Rebuild retains registrations, capacity, assignments,
preservation references, withheld availability, pending operations, recovery
outcomes, release recency, and independent stream revisions. It never resumes
checkout, creation, release, preservation, fetch, or recovery. Registered paths
may be missing. Git and checkout filesystem access are unnecessary. Historical
facts don't produce diagnostic notifications. The command itself uses ordinary
outcome diagnostics.

Rebuild replaces only the derived projection and identity index, atomically in
one durable transaction. Repeated rebuilds retain the same authority and complete event
history. History is never deleted, compacted, archived, or rewritten. Supported
historical registration versions keep their original stream routing. Unsupported
history, unsupported version metadata, corrupt positions/revisions/identities,
foreign catalog identity, and locator/configuration conflicts reject before
writable storage access. Rebuild doesn't perform an implicit schema upgrade.
Ordinary reads and mutations continue to refuse mismatched derived state.

Stable exclusive maintenance coordination spans the whole rebuild and waits for
live commands, including their Git work outside short catalog sessions. An
interrupted rebuild exposes a complete old or new derived state, or ordinary use
stays blocked. Inspect the catalog before requesting another rebuild after an
unknown outcome. If `redb` explicitly requires physical storage repair, use
`recover preview --catalog` and `recover apply --catalog`. That recorded recovery
validates complete immutable authority and retains existing locator operation IDs
and history. Physical repair may complete while derived damage remains:
`store_state` then reports `rebuild_required`, `catalog_mutations_available` is
false, and guidance requests `catalog rebuild`. The tool exposes ownership only
after complete reconstruction. Known storage-recovery operations remain available
for inspection by ID. Rebuild never initializes missing state or chooses another authority.

The pure positioned-history interface keeps typed decoders and reducers real.
Concrete `redb` checks cover derived corruption, immutable-history refusal, and
actual sync errors. Literal historical CloudEvents and generated capacity,
revision, and ownership sequences verify deterministic reconstruction. Public
command coverage compares ownership and recovery states. It also verifies
withholding, protected files, Git index/ref state, and full history, including
replay without Git or registered paths. Actual process kills bracket validation
and publication. Kernel lock waiters establish live-command coordination.
These checks establish observed process/commit behavior, without claiming
whole-machine power-loss guarantees or isolation from processes that ignore
coordination.

## Event storage

Immutable CloudEvents 1.0 JSON is authoritative. Events use the
versioned `io.lowkeylab.worktreepool.*` type namespace, catalog source URI, unique
source/id identity, subject, recording time in `UTC`, `application/json` typed
data, and string `operationid`. Affected-entity subjects remain separate from
stream routing. Refresh results carry `causationid` identifying their intent
event, which replay validates. Global committed positions determine history
order. Recorded stream identifiers and expected revisions enforce independent
catalog/repository sequences. Repository enrollment and new worktree registration
belong to the catalog stream. New explicit registration uses `worktree.registered.v2`.
immutable historical `worktree.registered.v1` retains its repository-stream routing.
Creation registration uses `worktree.creation.registered.v1`. Capacity, refresh,
assignment, and preparation facts belong to repository streams. Catalog stream
revisions include every catalog-owned registration, independently of repository
counts. `redb` transactions atomically update events, revision,
projection, and identity index with immediate durability. Replay uses pure
reducers and performs no Git or workflow effects. Failed or uncertain commits
stop retry and require reopening and inspecting event identities/revisions.

The standalone manifest records exact features. Root `Cargo.lock` remains the
Bazel dependency authority. The Bazel lockfile updater verified Rust 1.96
compatibility when adding the selected `redb` release.

## Test design and production wiring

The client boundary is the command result plus durable state through inspection.
Domain reducers own deterministic validation and transitions. The concrete store
owns `redb` transactions. Catalog workflows own bootstrap effects and locator
publication. Coordination owns scoped kernel locks. Path configuration snapshots
inputs. The command handler composes these modules. Deterministic internal
collaborators stay real, with no repository trait solely for mocking.

Every command fixture owns private temporary catalog/config/state files and
starts independent processes. Broad integration tests assert output and public
state. The embedded database runs in-process against real private files, with no
promised external schema consumers. Focused store tests inject corruption or
storage sync failure only at the existing storage boundary. A fault decorator
supplies I/O failure rather than verifying internal calls. Pure reducer properties
exercise decisions without filesystem effects. Listener tests supply a failing
destination and an independent recorder for the promised delivery contract.
They avoid incidental logger wording and call-count assertions.

Production registers listeners before catalog work. Initialization takes the
stable maintenance lock, then the catalog lock. Durable locator intent precedes
database creation. Immediate commit precedes active-locator publication, and
reporting follows committed authority. Database handles close before catalog
locks release. A guarded session exposes its private store only by borrow,
preventing the handle from escaping its lock lifetime. Repository workflows
hold maintenance protection throughout, take a stable repository lock before
each short catalog session, and close storage before Git effects. Repository
enrollment uses a stable enrollment lock before its catalog session.

The narrow initialization, refresh, acquisition, and release callbacks observe durable
checkpoints and external effect results. The
normal command supplies a no-op observer and performs the same effects. The
test-only helper signals each checkpoint, then waits until its parent kills it
under a bounded watchdog. Production has no environment test mode.

The public command and store/domain/listener Bazel targets pass on Rust 1.96.
They cover authority refusal, immutable history, configuration/path/output
contracts, commit uncertainty, listener isolation, concurrent initialization,
explicit enrollment, retained paths/files/modes, independent resource streams,
and actual process death at initialization/refresh checkpoints. Real local
remotes cover refresh success, missing main, transport failure, ref-lock failure,
parallel repositories, and interruption during transport without lock inheritance. The missing-catalog and
human-identity tests failed before implementation. Stable public results support
refactoring resistance, but no refactor-survival experiment accompanies this
slice. Fault tests establish commit/error semantics and don't claim simulated
power-loss durability. Normal processes establish kernel-lock behavior. Later
checks verify recovery and standalone installation. Creation checks cover
private paths, capacity, real Git failures, process interruption, independent
callers, protected destinations and lost results. Atomic batch faults verify
registration and ownership reopen together.

## Integration test execution

The public Git integration target runs locally with the development shell's
inherited `PATH`. Fixtures clear other inherited settings and own private local
repositories/remotes. Run complete Bazel test targets for acceptance. With
`googletest`, a literal `--test_filter=name` matches a fully qualified test name
and can skip every body while the Rust harness prints success. Use
`--test_arg=name`, a qualified name, or `--test_filter='*name*'` when filtering.

## Acquisition orchestration

Acquisition records exclusive reservation before preparation, preservation
intent before reference creation, and checkout intent before checkout. Old
detached tips receive unique `refs/worktree-pool/<operation-id>` reachability
references with Git reference `fsync` enabled. Expected-empty reference creation
refuses replacement. Checkout disables hooks, creates no branch, retains ignored
files, and uses ordinary detached checkout without force, reset, stash, or cleanup.
Each observed effect result and acquisition outcome commits before successful
output. No catalog handle remains across a Git effect. Repository coordination
serializes competing assignments and shared references, while independent
repositories can prepare concurrently. State decisions reopen under that lock.

## Acquisition modules and verification

`acquisition.rs` holds typed assignment facts, pure transitions and ordering.
`acquisition_workflow.rs` orchestrates the same concrete catalog and Git adapters
used by the command-line tool. The small pure ordering interface accepts explicit commit and
release-position values. It needs no repository mock. Private real Git/redb
fixtures assert public results, known-ID inspection, and preserved files/refs.
Worked decision inputs cover commit/recency/ID precedence. Independent processes
cover exclusive assignments and independent-repository progress. Actual SIGKILL
checks cover reservation, preservation intent/effect/result, checkout intent/effect,
and acquisition commit. A post-refresh barrier proves fixed default commits.
Other fixtures cover mandatory explicit-ref refresh, missing refs/failed remotes,
ignored collisions/fallback, hidden Git state, observation caches, retained ignore
changes, and lost stdout. These establish observed process and commit semantics. They don't simulate
machine power loss. Installation verification separately exercises the minimum
Git runtime.

## Creation orchestration

Creation registration and preparing assignment reservation commit atomically
before filesystem effects. Directory preparation has a durable creation intent
and path checkpoint. Checkout intent precedes ordinary `git worktree add --detach`
at the fixed commit. There are no force, reset, stash, branch-creation or arbitrary
setup hooks. The workflow refuses existing destinations or Git metadata and
revalidates its prepared empty directory immediately before Git. Real canonical
checkout/common-directory/Git-directory membership, clean state, and detached commit
checks precede atomic creation and acquisition result commitment.

## Creation verification

Release keeps idle worktrees and ignored files for reuse. Real fixtures verify
new detached acquisition, retained ignored artifacts, release, and reuse with a new
handle. These behavior checks establish no measured build speedup. The separate
benchmark gate evaluates benefit and retained resource costs.

## Release events, modules, and verification

Release facts use versioned `assignment.release.started.v1`,
`assignment.release.preservation.finished.v1`, and `assignment.release.finished.v1`
types under the existing event namespace. They route through repository streams,
identify the affected worktree subject, and validate causal checkpoint chains.
`release.rs` owns pure facts/reducers. `release_workflow.rs` holds concrete
lock-scoped orchestration. Production and crash fixtures use the same workflow
and Git/store adapters. Private real fixtures cover release rejection, branch workflows,
detached workflows, durable preservation failures, retained-file recency and
collisions, live callers, stale handles, concurrent processes, crashes, and lost
stdout. A real `redb` sync fault checks atomic release/ownership/recency reopening.
These checks don't simulate whole-machine power loss or filesystem isolation.

## Recovery event and locator history

Worktree recovery facts use `recovery.started.v1`, `recovery.preserved.v1` and
`recovery.finished.v1`. Repository refresh reconciliation uses
`repository.recovery.started.v1` and `repository.recovery.finished.v1`, all under
`io.lowkeylab.worktreepool`. They retain operation identities, causal checkpoint
chains and repository stream revisions. A pending recovery blocks competing
acquire/release/refresh for that repository after process death. Independent
repositories retain progress. Database handles close before Git effects and
catalog unlock, and lock descriptors aren't inherited by child processes.
Indeterminate commit handling reopens and inspects the exact event ID and revision
before any further effect. It never blindly retries a transaction or Git effect.

Catalog initialization and storage-repair recovery precede repository authority.
Their schema-versioned immutable `recovery_history`, exact operation IDs and
intent/completed checkpoints live in the stable active-catalog locator, not
repository CloudEvents. The current locator recovery fields are a projection of
that history. The locator retains historical legacy checkpoint metadata explicitly without
inventing missing intent facts. Catalog recovery IDs support public operation
list/inspect and exact recover selectors even while repository storage is pending.
A partial operation list explicitly reports unavailable repository operations.
Later lifecycle changes must preserve prior locator history and known results.

## Recovery modules and verification

`recovery.rs` owns pure worktree decisions and causal replay.
`recovery_workflow.rs` owns concrete preservation and ownership orchestration.
`recovery_inspection.rs` owns exact registered selection and read-only observations.
`repository_recovery.rs` owns the distinct refresh lifecycle, while `catalog.rs`
owns locator/bootstrap/storage authority and `catalog_recovery_cli.rs` composes its
public receipts. These modules keep real Git, filesystem, kernel locks and `redb`
effects. Checkpoint observers provide deterministic interruption boundaries without
replacing those dependencies. Private real fixtures verify SIGKILL/restart,
preview preservation, known/unidentified results, competing processes and actual
`redb` sync-failure atomicity. They don't establish whole-machine power-loss safety
or a benchmark speedup.

## Relocation events and physical coordination

Logical relocation is authoritative in two typed catalog-stream CloudEvents:
`io.lowkeylab.worktreepool.catalog.relocation.started.v1` and
`io.lowkeylab.worktreepool.catalog.relocation.completed.v1`. Started records the
exact operation/catalog identities, lossless source/destination paths and copied
revision before any destination effects. Completed links to that exact Started
identity. The tool commits Completed only at the destination after the durable
locator switch. Each append atomically updates event history and projection, advancing
both global and catalog-stream revisions. `events list` includes both facts.
Pure replay and rebuild reconstruct pending/completed relocation state without
filesystem effects. Prior immutable event records remain byte-for-byte prefixes.
Handles, owners, capacity, and checkout paths remain unchanged.

The stable locator retains schema-version-3 `relocation_history` as technical
physical coordination, with contiguous positions, exact operation IDs and fixed
CloudEvent identities. Its checkpoints are `starting`, `intended`, `prepared`,
`switched`, `committing` and `completed`. Starting fixes the Started wire event and
prior history proof before its append. Intended freezes tagged `SHA-256` evidence
of the closed Started-bearing source. Committing fixes the Completed wire event
before its append. Copy preparation requires exact closed bytes, identity, history,
and projected state. A replacement with only matching catalog ID and revision is
insufficient. Completed logical authority comes from the accepted CloudEvent, and
ordinary mutations remain barred until its matching technical publication finishes.

Prior bootstrap/storage-repair lifecycle facts and IDs remain unchanged, including
explicitly observed legacy facts. The tool rejects nonempty schema-version-1
locator-only relocation history. Preserve files and operation IDs for manual
verification and reconciliation. There is no automatic migration, backfill, fabricated event
identity or timestamp. Unpublished schema-version-2 decoded-proof journals also
refuse use with manual-preservation guidance. The tool never reinterprets them as
raw proof. Prefix evidence hashes exact ordered stored-record strings, and fixed
Started/Completed records use the normal Store encoder with exact positions and
expected stream revisions. Repair/retry requires those literal records unchanged.
Empty older locator relocation metadata remains compatible.
Inconsistent new event/journal correlation refuses mutation. A replay-derived
pending Started with a missing journal names its exact ID and requires manual
restoration of that matching journal. The pending reason is
`catalog_relocation_pending`. Successful commands use `catalog_relocated`,
`catalog_relocation_completed`, or `catalog_relocation_already_completed`.
Inspection uses `catalog_relocation_observed`.

## Relocation recovery checkpoints

- After Starting publication, recovery inspects the exact fixed Started identity and
  expected revisions before appending or observing it. After Started commit but
  before Intended publication, it validates the prior immutable prefix and freezes
  the closed source evidence. After intent or directory creation, it can prepare the
  recorded destination.
- After complete copy but before its checkpoint, recovery validates identical bytes,
  identity, complete history and projection before advancing the same operation.
- For an empty, partial or conflicting copy before preparation, verify the recorded
  source and preserve the conflicting destination file outside the destination
  directory manually. Recover the same ID to create a fresh complete copy.
- After preparation or switching, missing or changed destination bytes require
  manual restoration of the exact verified copy from the retained source, with
  conflicting bytes preserved first. Recovery then revalidates both copies before
  advancing. Missing or changed source bytes also require manual restoration of
  the exact bytes recorded by the intent, preserving conflicting bytes first.
- After switching or fixed Completed publication, recovery inspects the exact
  accepted Completed identity/revisions before retry. A known committed result
  advances technical publication without appending a duplicate event.
- If an unclean semantic commit requires physical `redb` repair, only explicit
  exact-ID relocation recovery records a distinct correlated StorageRepair intent.
  It validates the immutable prefix, fixed event identities, `UUID`, and revisions.
  If derived records remain damaged, it explicitly rebuilds them under that same
  recorded repair scope. Ordinary `catalog check`, `catalog rebuild`,
  `recover preview`, and open commands never repair.
  Interrupted repair/rebuild resumes the recorded repair pair through the same
  relocation ID, validating history before recording completion and rebinding that
  file's digest. This route remains available while relocation bars ordinary rebuild.
  Unknown or mismatched history requires manual preservation/restoration instead.
  `redb` can repair physical metadata before `UUID`/history validation on an unclean
  file. That inherited limitation never authorizes a semantic retry.
- After completed publication, repeating exact-ID recovery changes no history and
  never inspects retained inactive files, even after later authority changes.

## Retirement events and reconciliation

`retirement_preconditions_unmet` is a rejected result with exit code 2. Other
Git, filesystem, pending, and commit-uncertainty reasons retain their documented
codes. Retirement records `worktree.retirement.started.v1` and
`worktree.retirement.finished.v1` facts under `io.lowkeylab.worktreepool`.
The intent belongs to the repository stream. Its result retires the global
registration in the catalog stream, retaining its repository identity in the payload.
An intended retirement keeps capacity counted and blocks conflicting repository
work. Only its committed result frees that record's count. Inspect the exact
retirement ID through `operation inspect`, or use `recover preview/apply
--operation <retirement-id>` after interruption. Apply revalidates removal and
preservation before completing an intended result. A committed exact identity
returns its historical receipt without repeating facts or affecting a newer
registration. Unknown identities never select the newest operation.

## Retirement verification

Retirement verification uses private real Git/filesystem/redb fixtures and public
command-line inspection, exact operation recovery, real subprocess SIGKILL boundaries, and
actual backend sync failures. The commit-fault observer arms the existing real
backend immediately before the same production transaction commit. Earlier
storage errors aren't classified as failed commits. Reopen checks complete old or
complete new history and projection before any retry. These checks don't establish
whole-machine power-loss behavior or a performance benefit.

## Standalone installation acceptance

Repository maintainers reproduce the actual source-package/install acceptance with:

```sh
nix develop --command bazel run //worktree_pool/install:acceptance -- \
  --output /absolute/new/private-verification-directory
```

The output directory must not exist. The target resolves Cargo/rustc and the
complete generated `sysroot` through Bazel's pinned Rust 1.96.0 toolchain. It
runs real Cargo package verification, extracts the exact archive, compares
source hashes, records normalized metadata and features, and performs locked
installation from the extracted package. It builds checksum-pinned official
Git 2.36.0 and static `zlib` 1.3.1 privately, then runs the full real command-line suite
against the installed executable from an unrelated working directory.
Runtime `PATH` contains only that Git and the shell/sleep utilities needed by
fixtures. That path excludes Cargo, rustc, and Bazel. It retains actual
compiler/Git versions, archive contents/hashes, native executable dependency inspection,
installed version, command logs and raw test results. Application version is
0.1.0. The Bazel development binary's generated package version can be 0.0.0.
Source equivalence never promises byte-identical Cargo/Bazel executables.

The suite's injected crash/commit-fault children use the Bazel test-support
library to establish interruption/storage boundaries. Their public observation
and recovery commands use the installed artifact. Helper execution isn't
independent proof that the installed command-line tool can inject those faults. Tests own
private Git/filesystem/redb fixtures and use no mock framework. Cargo registry
access and shared repository caches remain external dependencies. Runtime
verification establishes `PATH` independence. Toolchains can remain elsewhere on the
host filesystem. The minimum Git build tests local transport only, with optional
HTTP, transport layer security, and scripting features turned off. Source installation verification uses the
Nix-managed Linux host. The produced binary depends on that host's C runtime
closure and isn't a universal prebuilt Linux distribution.

## Accepted benchmark and resource costs

The maintainer accepted the measured gate and retained resource costs on
2026-10-09. The protocol covered 100 pairs: five workloads, two alternating
batches of ten, 200 arms and 240 caller intervals. All 252 workload/input guards
passed. Pooled/disposable median complete acquire-to-build times in seconds were:

| Workload                           | Pooled | Disposable | Pooled wins in each batch |
| ---------------------------------- | ------ | ---------- | ------------------------- |
| Unchanged                          | 13.653 | 28.781     | 10/10, 10/10              |
| Small edit                         | 35.997 | 73.393     | 10/10, 10/10              |
| Revision change                    | 17.613 | 28.918     | 10/10, 10/10              |
| Return revision                    | 19.704 | 29.747     | 10/10, 10/10              |
| Two callers, overall/slower caller | 35.648 | 36.866     | 9/10, 7/10                |
| Two callers, faster caller         | 20.764 | 36.450     | 10/10, 10/10              |

The primary workloads met the agreed eight-of-ten criterion in both batches.
The secondary case stayed inside the accepted regression guard. Consistency is
not statistical confidence or a promise for other repositories. This experiment
built `//nicknamer/server/lib:lib` with a local origin and controlled cache policy.
It excludes GitHub latency, and archive/remote/page caches and host contention
remain incompletely attributable. The repository benchmark protocol records controls, paired variation and
correctness obligations separately from the standalone distribution.

Four initially warmed slots retained 25,096,658,944 allocated bytes and summed
idle Bazel-server resident set size of 10,598,858,752 bytes. Final retained allocation was
25,112,670,208 bytes. Final summed resident set size was 6,097,256,448 bytes with two servers
recreated only for resource snapshots. It isn't four continuously warm servers.
Summed resident set size includes shared pages and isn't unique physical or whole-process-tree
peak memory. The largest caller allocated 6,283,272,192 bytes and the largest
observed server resident set size was 5,911,044,096 bytes. Catalog logical size grew from
61,440 to 3,149,824 bytes and events from 2 to 1,696 across the measured lifecycle.

Registered capacity is a count limit, not a disk/RAM budget. Retained worktrees,
build output bases, caches, and server state can cost much more than the catalog.
Retirement never deletes those resources. Inspect known and unknown usage and
plan explicit human retention decisions. Benchmark acceptance supports this
measured adoption scope. Minimum-toolchain installation and public behavior are
separate correctness evidence and never establish a new performance benefit.

## Git observations and coordination

Git separates membership records with zero bytes. Observations turn off optional
index writes and clear ambient repository-routing overrides. The catalog lock and
database aren't held across Git subprocesses. Repository locks serialize shared
reference changes while independent repositories can progress. Preservation writes
use reference `fsync` before ownership changes.

## Relocation verification limits

Ambiguous files or inconsistent locator facts refuse mutation. Corrupt or
unsupported lifecycle metadata requires manual verification and reconciliation.
The tool never invents missing history, chooses a newer operation, or initializes
a replacement authority. Real subprocess crash tests demonstrate process
interruption behavior and cooperative locking, not whole-machine power-loss
recovery or protection from hostile same-user filesystem changes. `SHA-256` evidence
strengthens accidental replacement detection, not hostile-filesystem isolation.

## Catalog storage repair

Ordinary initialization never repairs or replaces existing authority. Explicit
catalog apply resumes recognized initialization checkpoints or records a repair
intent before writable reopening of a genuinely unclean database. It validates the
expected catalog `UUID` and complete history before publishing completion. It never
resets or creates over present conflicting, corrupt, or unsupported authority.
`redb` may repair allocator/header metadata before `UUID` validation on an unclean
substituted foreign file. That explicit-apply rejection preserves authoritative
events/history but doesn't promise physical-byte preservation or filesystem
isolation. Healthy rejected conflicts and all read-only paths preserve bytes.

## Source distribution policy

A maintainer needs a Linux x86-64 source-build environment with Rust/Cargo 1.96+
and a native linker/C toolchain. Runtime needs Git 2.36+ and the resulting native
C runtime libraries. Execution never invokes Rust, Cargo or Bazel. Runtime verification establishes
that these tools are absent from `PATH`. Installation
is distinct from repository development commands. The package is ready for
publication review. This work doesn't publish it. The maintainer must
verify name availability immediately before publication. A conflict requires
maintainer resolution.

After a maintainer publishes version 0.1.0, the intended registry command is:

```sh
cargo install --locked --version 0.1.0 worktree-pool
```

Before publication, install from an extracted verified `worktree-pool-0.1.0.crate`
using `cargo install --locked --path /path/to/worktree-pool-0.1.0`. The retained
archive includes its standalone lockfile, exact manifest, source, `AGPL-3.0` license,
and public docs. It excludes Bazel `BUILD` files, benchmark/installation adapters
and external workspace dependencies. Run `worktree-pool --version`, then initialize
one catalog, explicitly register the repository, and acquire with `--repo` from
any working directory. Retain the returned assignment handle and follow release
and recovery obligations in the README. Installation never initializes or migrates catalogs.
