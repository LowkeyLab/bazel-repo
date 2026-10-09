# Worktree pool

A synchronous Linux x86-64 command-line tool for explicitly managed persistent
Git worktrees. It supports catalog initialization and inspection, explicit
repository/worktree enrollment, registered resource inspection, checkpointed
repository refresh, safe acquisition and explicit release of registered worktrees,
assignment and operation inspection, event history, on-demand detached creation,
durable per-repository capacity, and explicit catalog relocation.

Development uses Nix and Bazel. The standalone manifest pins every direct
dependency and targets Rust 1.96, matching the Bazel toolchain. Its executable
name is `worktree-pool`. Registry publication and name verification remain
separate work.

## Catalog authority

Run `worktree-pool catalog init` once. Ordinary commands never initialize missing
storage. Catalog data defaults to `$XDG_DATA_HOME/worktree-pool/catalog.redb`,
configuration to `$XDG_CONFIG_HOME/worktree-pool/config.toml`, and stable locator
and coordination files to `$XDG_STATE_HOME/worktree-pool`. Fallback roots are
`~/.local/share`, `~/.config`, and `~/.local/state`. Tool-owned directories use
mode 0700 and files 0600. The tool rejects unsafe existing storage permissions.

`--catalog-dir` overrides `WORKTREE_POOL_CATALOG_DIR`, then the configuration's
`catalog_dir`, then defaults. `--json` overrides `WORKTREE_POOL_JSON`
(`true`/`false` or `1`/`0`), then configuration `json`, then false.
`--config` selects an explicit configuration file in `TOML` format.
Environment roots must be absolute. Location settings select the active
authority. They never relocate storage or initialize a second catalog.

Initialization persists the catalog identifier and pending locator before
creating `redb`, then publishes the active locator through an atomic rename.
Interrupted initialization remains pending. Repeating initialization can't
overwrite catalog data or repair pending work. Information and validation
commands examine locator identity, store/projection versions, complete event
history, contiguous revisions, deduplication index, and projection equivalence.
Unsupported history stops use.

## Check and rebuild derived state

```text
worktree-pool catalog check
worktree-pool catalog rebuild
```

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

## Output contract, version 1

JSON mode writes one object followed by a newline:

```json
{
  "schema_version": 1,
  "command": "catalog info",
  "outcome": "completed",
  "reason_code": "ok",
  "context": {},
  "data": {},
  "warnings": []
}
```

The example omits command-specific values. `context.catalog_id` is the stable
`UUID`. `context.catalog_path` is a lossless path. Catalog command `data` contains
`catalog_id`, numeric `revision`, `store_version`, and `projection_version`.
`events list` has `data.events`, an ordered array of structured CloudEvents.
Errors have `data.next_action`, safe human guidance. Resource commands include distinct
`repository_id`, `worktree_id`, `assignment_handle`, and `operation_id` values
where applicable.
Ownership and availability remain distinct concepts.

Paths use `{"encoding":"unix-bytes","bytes":[47,116,109,112],"display":"/tmp"}`.
The bytes are authoritative Unix filename bytes. Display is informational and
may contain Unicode replacement characters. Input arguments retain native OS
bytes. Configuration string paths must be UTF-8. Paths containing zero bytes
are invalid.

| Outcome   | Exit status | Meaning                                         |
| --------- | ----------- | ----------------------------------------------- |
| completed | 0           | The requested operation completed               |
| rejected  | 2           | The request failed its preconditions            |
| pending   | 3           | Durable unfinished work requires reconciliation |
| unknown   | 4           | Inspect recorded identity/state before retry    |

Stable reasons: `ok`, `invalid_arguments`, `invalid_configuration`,
`catalog_missing`, `catalog_exists`, `catalog_conflict`, `catalog_corrupt`,
`unsupported_version`, `initialization_pending`, `unsafe_permissions`,
`filesystem_error`, `storage_error`, `commit_unknown`, `git_failed`,
`resource_unregistered`, `selector_conflict`, `capacity_exhausted`,
`capacity_below_count`, `capacity_zero`, `capacity_all_assigned`,
`capacity_no_safe_worktree`,
`operation_pending`, `refresh_failed`, `worktree_unavailable`,
`retained_file_collision`, `unfinished_work`, `git_operation_in_progress`, and
`unsupported_index_state`, `assignment_unknown`, and `already_released`.

A broken stdout returns status 4 and sanitized stderr guidance. This doesn't
undo a committed outcome or retry initialization. Inspect the catalog to learn
what committed. Diagnostics exclude configuration contents and raw storage
errors, keeping credentials and file contents out of output.

Interrupted initialization inspection remains nonzero/pending. It includes the
recorded catalog ID/path, phase, last observed durable checkpoint, store state,
and available revision. A valid committed store under a pending locator yields
`store_committed`. Inspection never publishes or retries initialization.

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
slices verify recovery and standalone installation. Creation checks cover
private paths, capacity, real Git failures, process interruption, independent
callers, protected destinations and lost results. Atomic batch faults verify
registration and ownership reopen together.

## Registered resources and refresh

```text
worktree-pool repo register /absolute/repository
worktree-pool repo list
worktree-pool repo inspect --repo <repository-id-or-path>
worktree-pool worktree register /absolute/linked-worktree --repo <repository-id>
worktree-pool worktree list --repo <repository-id>
worktree-pool worktree inspect <worktree-id-or-registered-path>
worktree-pool repo refresh --repo <repository-id-or-path>
```

Commands work from unrelated directories. Without `--repo`, commands requiring
repository context use the current checkout. A positional repository selector
and `--repo` must agree. Canonical Git common-directory bytes identify one
repository. Linked checkouts share an identity. Separate clones don't.
Repository enrollment never enrolls other worktrees. Worktree enrollment checks
Git membership records separated by zero bytes and retains the existing absolute path,
permissions, checkout files, and ignored build state. Git observations turn off
optional index writes and clear ambient repository-routing overrides.

Repository records contain identity, common directory, recorded context path,
durable capacity, and repository stream revision. The default capacity is four.
Every registered record counts, including missing and withheld worktrees.
Repeating enrollment returns the existing record without appending history.
Worktree inspection accepts registered identities/paths only, including recorded
missing paths. Discovery never exposes an unrelated checkout.

Command data uses `repository`, `repositories`, `worktree`, or `worktrees` as
appropriate. Repository inspection also reports `registered_count`,
`operations`, and `creations`. Worktree results separately expose `registration_state`
(`registered`, `missing`, `mismatched`, `unreadable`). They expose `ownership` (`unassigned`,
`preparing`, or `assigned`), `availability` (`unverified` or `withheld`), `withheld_reason`,
and `pending_work`. An existing checkout needs acquisition-time safety
validation before reuse. Missing/mismatched paths and pending repository work
remain withheld. Inspection completes successfully without making them reusable.

Refresh records intent before invoking Git, fetches exactly origin's main branch
into `refs/remotes/origin/main`, then records its full resolved commit. It doesn't
change checkout files, prune registrations, or expose other worktrees. The
catalog lock and database aren't held across Git subprocesses. Repository locks
serialize shared-reference changes. Independent repositories can progress.

Refresh data contains its `operation`: operation/repository identities,
`intent_event_id`, `state`, `last_checkpoint`, and nullable `resolved_commit`.
Success records `completed`/`result_committed`. A failed fetch or commit
resolution records `needs_reconciliation`/`fetch_failed` and returns
pending/`refresh_failed`. Interruption before result commitment retains
`pending`/`intent_recorded`, even when Git actually completed. Repeating a pending
refresh returns the existing operation without retrying effects or changing
history. Explicit reconciliation follows in a later slice. Lost stdout doesn't
undo a completed refresh. Inspect the recorded repository operations.

The public Git integration target runs locally with the development shell's
inherited `PATH`. Fixtures clear other inherited settings and own private local
repositories/remotes. Run complete Bazel test targets for acceptance. With
`googletest`, a literal `--test_filter=name` matches a fully qualified test name
and can skip every body while the Rust harness prints success. Use
`--test_arg=name`, a qualified name, or `--test_filter='*name*'` when filtering.

## Acquire persistent worktrees

```text
worktree-pool acquire --repo <repository-id-or-path> [reference]
worktree-pool assignment list [--repo <repository-id-or-path>]
worktree-pool assignment inspect <assignment-handle>
worktree-pool operation list [--repo <repository-id-or-path>]
worktree-pool operation inspect <operation-id>
```

Every acquisition refreshes origin/main first, including explicit references. The
refreshed default uses the commit in the recorded refresh result. An explicit
reference resolves once afterward. Preparation keeps that commit fixed even
when a reference changes. Only registered, unowned, present, safe worktrees are
eligible. Selection prefers the matching commit, then the latest recorded
release position, then worktree ID. Initial registrations have no release
position. Completed release facts supply actual release positions. If no safe
registration is available and capacity remains, acquisition creates another
detached linked worktree. Existing safe worktrees always get priority.

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

`acquire` data contains `assignment`: its unique handle, operation, repository,
worktree, absolute lossless `path`, fixed `resolved_commit`, nullable `branch`,
and `state` (`preparing`, `active`, or historical `released`). A completed detached acquisition has a null
branch. These same assignment records appear in list/inspect results. Operation
list/inspect includes refresh, acquisition, release, and creation records. Acquisition records carry
`state`, `last_checkpoint`, `intent_event_id`, and nullable `preservation_tip`
and `preservation_reference`. Known lost results remain available for inspection by handle
or operation ID. The command-line tool never guesses a newest assignment.

Pending acquisition states are `reserved`, `preservation_intended`, `preserved`,
`checkout_intended`, and `needs_reconciliation`. A completed result is
`completed`. Worktree inspection reports preparing or assigned ownership
separately from withheld availability and includes unfinished operations in
`pending_work`. Process death never ends a reservation. Pending work blocks
conflicting effects until explicit reconciliation, which follows in a later
slice. Lost stdout returns unknown/status 4 while committed ownership remains.

Safety probes reject dirty tracked/index/untracked state, operation
markers and index locks, hidden index flags, and sparse layouts. Prospective
retained-file collisions withhold that candidate and allow a safe alternative.
The preflight includes empty directories, prefix conflicts and symlinks. The Git
`--no-overwrite-ignore` option alone can remove an empty ignored directory. Changed
ignore rules may reveal retained files as untracked work after checkout: preparation
then remains pending, with bytes retained and no active assignment reported.
Withheld reasons distinguish `retained_file_collision`, `unfinished_work`,
`git_operation_in_progress`, `unsupported_index_state`, and `safety_uncertain`.
Inspection never clears recorded withholding.

Ordinary indexed regular files require exact raw blob equality and executable
mode agreement. Indexed symlinks require exact link bytes. These checks avoid
optimistic stat/fsmonitor caches. Pool Git commands turn off `fsmonitor` and object
replacement. Submodule/gitlink and special index layouts remain conservatively
unsupported. Filter, encoding, or line-ending conversion that changes raw bytes
can also withhold reuse. Exact raw-byte checks don't invoke clean filters.
Its locks coordinate pool commands, not same-user filesystem
changes after observations.

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
changes, and lost stdout. These establish observed process and commit semantics. They don't simulate machine power loss or establish minimum Git runtime support.

## Capacity and on-demand creation

```text
worktree-pool pool configure --repo <repository-id-or-path> --max-worktrees 4
```

The maximum defaults to four and persists in the repository's event stream.
`pool configure` data contains the updated `repository`, `registered_count`,
and committed `revision`. A maximum below the current count rejects with
`capacity_below_count`. A zero count permits a zero maximum. Environment or
output configuration can't replace this durable policy. Every registration counts,
including assigned, withheld, missing, and partially prepared records. Configuration
never removes registrations, files, or build state.

After mandatory refresh and safe reuse selection, acquisition creates only when
count is below the recorded maximum. Exhaustion returns rejected/status 2 with
`capacity_zero`, `capacity_all_assigned`, or `capacity_no_safe_worktree`. Its data
contains `maximum`, `registered_count`, `assigned_count`, and `next_action`, with
the repository ID in context. Explicit enrollment still uses `capacity_exhausted`.
There is no waiting for a release, overflow checkout, eviction, or deletion.
Required repository coordination and refresh still precede the capacity decision.

New paths use `<catalog-directory>/worktrees/<repository-id>/<worktree-id>` and
retain absolute byte-safe identity. Tool-created directories use mode 0700.
Existing worktrees retain their permissions and paths. Catalog relocation never
moves previously recorded worktrees.

Creation registration and preparing assignment reservation commit atomically
before filesystem effects. Directory preparation has a durable creation intent
and path checkpoint. Checkout intent precedes ordinary `git worktree add --detach`
at the fixed commit. There are no force, reset, stash, branch-creation or arbitrary
setup hooks. The workflow refuses existing destinations or Git metadata and
revalidates its prepared empty directory immediately before Git. Real canonical
checkout/common-directory/Git-directory membership, clean state, and detached commit
checks precede atomic creation and acquisition result commitment.

Worktree output includes nullable `creation`. Repository inspection includes
`creations`. Creation operations have a distinct `operation_id`, repository/worktree
IDs, `assignment_handle`, `acquisition_operation_id`, `intent_event_id`, `state`,
and `last_checkpoint`. States are `reserved`, `path_prepared`, `completed`, and
`needs_reconciliation`. Checkpoints are `creation_registered`,
`creation_path_prepared`, `creation_committed`, and `creation_uncertain`.
A pending creation also appears in `pending_work` and operation list/inspect.
Failed or interrupted effects retain preparing ownership and countable unavailable
registration. Removing an obstruction doesn't authorize retry: explicit
reconciliation must resolve the recorded state. An empty directory after interruption doesn't prove
that preparation or checkout completed.

Release keeps idle worktrees and ignored files for reuse. Real fixtures verify
new detached acquisition, retained ignored artifacts, release, and reuse with a new
handle. These behavior checks establish no measured build speedup. The separate
benchmark gate evaluates benefit and retained resource costs.

## Release assignments

```text
worktree-pool release <assignment-handle> [--repo <repository-id-or-path>]
```

Release requires the current unique handle. An optional repository selector must
agree with that handle. Unknown handles return `rejected`/`assignment_unknown`.
Paths never substitute for a handle. Dirty tracked files, staged/index changes,
untracked files that Git doesn't ignore, Git operations, special indexes, missing/mismatched
registration, or uncertain observations reject without ending ownership. Copying
work elsewhere doesn't certify a checkout that still contains unfinished work.
There is no preservation-assertion flag that bypasses safety checks. Commit or
otherwise finish the checkout's unfinished state before release. The tool never
stashes, resets, forces checkout, removes work, or deletes operation markers.

Ignored files alone permit release and remain in place across subsequent
acquisitions. A later checkout collision with retained state withholds reuse.
An otherwise clean caller-created branch stays checked out and unchanged.
Detached tips require new, expected-empty `refs/worktree-pool/<release-operation-id>`
reachability references with reference `fsync`. Intent commits before that Git
effect. Its observed outcome commits before ownership changes. Final validation
checks the recorded checkout/common-directory/Git-directory binding, clean state,
unchanged tip/branch, and the exact detached preservation reference. Uncertain
state retains ownership and requires explicit reconciliation.

Release data contains `assignment`, `operation`, `already_released`, nullable
`current_availability`, `revision`, and `next_action`. The assignment's commit and
branch remain its acquisition snapshot. The release operation records the observed
`tip` and nullable full branch reference separately, plus operation/repository/worktree/
assignment identities, `state`, `last_checkpoint`, `intent_event_id`, and nullable
`preservation_reference`. A reference name alone isn't proof its effect completed.

Release states are `intended`, `preserved`, `completed`, and `needs_reconciliation`.
Checkpoints distinguish `release_intended`, `preservation_committed`,
`preservation_failed`, `release_committed`, and `release_state_uncertain`. Failed
or interrupted preservation retains an active assignment and returns
`pending`/`operation_pending`. Repeating that handle returns the recorded release
without retrying Git or changing history. Indeterminate database commits return
`unknown`/`commit_unknown`. Inspect the handle and operation before retrying.
A lost stdout returns status 4 while a committed release remains released.

Completed release atomically records historical `released` ownership and the
worktree's `last_release_position`. A repeated released handle completes with
`already_released` without changing a newer assignment or claiming current
availability. `current_availability` is null because release doesn't certify a
future checkout. Worktree inspection reports current ownership and availability
separately. Unassigned checkouts still need acquisition-time safety validation.
Assignment list/inspect retains released history. Operation list/inspect includes
release progress and preservation evidence. Pending work remains visible there
and in worktree `pending_work`.

Callers must stop work before release and never resume using obsolete handles.
Live process presence alone doesn't veto explicit release. Ownership is
cooperative: release doesn't revoke filesystem access or stop processes. Neither
process death nor elapsed time releases ownership.

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

## Explicit recovery and lost results

```text
worktree-pool recover preview [--repo <id-or-path>]
worktree-pool recover preview --operation <id>
worktree-pool recover preview --assignment <handle>
worktree-pool recover preview --worktree <id-or-registered-path>
worktree-pool recover apply --operation <id>
worktree-pool recover apply --assignment <handle> [--abandon]
worktree-pool recover apply --worktree <id-or-registered-path> [--abandon]
worktree-pool recover preview --catalog
worktree-pool recover apply --catalog
```

Repository preview lists candidates without choosing one. Apply requires an exact
recorded identity. Worktree paths preserve non-UTF-8 bytes and select only an
explicit registration. Assignment handles remain historical identities after
release. They never select a newer owner. Conflicting selectors reject. Known
operation/assignment/worktree inspection resolves lost output. An unidentified
result requires explicit inspection and selection rather than another acquisition.

Preview and catalog/repository/worktree/assignment/operation/event inspection use
read-only storage validation, including complete authoritative history, versions,
revisions and projection replay. Recovery previews observe actual checkout binding,
`HEAD`, branch, index, protected content, operation state and required preservation
roots under maintenance and repository coordination. They never repair storage,
fetch, checkout, create a worktree, update references or change ownership/history.
An unclean `redb` file can refuse read-only inspection until explicit catalog recovery.

Apply records its own durable intent and result. Detached work requires verified
direct `refs/worktree-pool/<operation-id>` roots. A symbolic reference with the
same resolved commit doesn't certify preservation. New writes use expected-empty
references and reference `fsync`. Recovery observes existing roots before any retry.
It never overwrites conflicting roots. Final validation repeats binding,
content/index/operation, tip/branch and required-root checks before ownership can
change. `--abandon` can't waive dirty or uncertain protection. Missing, moved,
mismatched, or unproven partial creation remains countable and protected. A prepared
empty directory after restart isn't proof a Git creation effect completed.

A verified interrupted acquisition or linked creation can activate its existing
preparing assignment. An unproven effect completes reconciliation with preparing
ownership and withheld availability. Interrupted release observes or safely
completes its recorded preservation before ending ownership. Explicit abandonment
preserves the actual safe tip before releasing an active or preparing assignment.
Without abandonment, an active assignment stays active. Safe unassigned
reconciliation only removes withholding and reports `unverified` availability.
Acquisition must still validate it. Recovery never repeats fetch, checkout or
worktree creation, and never resets, stashes, deletes, prunes or stops callers.

`outcome` and exit status describe the requested reconciliation independently of
`ownership` and `availability`: `completed`/0 can retain active/preparing ownership
or withheld availability. `pending`/3 retains a recorded unfinished operation.
`unknown`/4 requires inspecting durable outcome evidence. `rejected`/2 doesn't
certify a free worktree. Data includes the recovery operation, selected identities,
observations, preservation evidence, revision, and next action. Historical released
handles have nullable availability. A repeated stale release returns
`already_released`. A release performed by recovery has nullable `operation` and
its durable `recovery_operation` receipt.

Interrupted original operations can become `reconciled`, distinct from their
original successful `completed` result. Exact original IDs then return their
recorded completed recovery without adding another workflow. To request a new
observation after resolving protected state, explicitly select the worktree or
assignment. Refresh reconciliation never infers successful fresh fetch from an
existing local `origin/main` reference: it reports `fresh_fetch_proven: false` and
requires a new explicit repository refresh or acquisition. A previously committed
successful refresh remains an unchanged proven result.

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

Ordinary initialization never repairs or replaces existing authority. Explicit
catalog apply resumes recognized initialization checkpoints or records a repair
intent before writable reopening of a genuinely unclean database. It validates the
expected catalog `UUID` and complete history before publishing completion. It never
resets or creates over present conflicting, corrupt, or unsupported authority.
`redb` may repair allocator/header metadata before `UUID` validation on an unclean
substituted foreign file. That explicit-apply rejection preserves authoritative
events/history but doesn't promise physical-byte preservation or filesystem
isolation. Healthy rejected conflicts and all read-only paths preserve bytes.

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

## Catalog relocation

`worktree-pool catalog relocate <destination-directory>` copies the closed catalog
database to a new private directory and atomically switches the stable active
locator. The destination must not exist, its parent must exist without symbolic
path aliases, and the destination must be separate from source storage, stable
state, registered checkouts and Git common directories. Relative destinations
resolve against the current directory; parent traversal is rejected. New tool
directories use mode 0700 and files 0600. Configuration files and environment
settings are never rewritten.

After completion, select the destination explicitly with `--catalog-dir` or update
your own location setting. Obsolete selectors return `catalog_conflict`, include
the actual active location and explain the next action. They never adopt the old
database. The stable locator and maintenance, catalog and repository lock files
stay under the original state directory. Relocation excludes concurrent commands
for its entire duration, including commands preparing Git effects. It rejects
unresolved catalog, repository and worktree operations; completed assignments may
remain active throughout relocation.

Catalog identity, handles, capacity, preservation records, complete database
bytes and event history are retained. Existing absolute checkout paths remain
fixed, including worktrees created beside the old catalog. No worktree is moved,
deleted or cleaned. The source database also remains in place as inactive tool
data. Each relocation retains another database copy and consumes additional disk;
receipts report `retained_source`, its measured `retained_source_bytes` when
available, and `retained_source_active`. An unavailable measurement is null.
Earlier retained locations remain recorded in lifecycle history. There is no
automatic deletion, backup/restore subsystem or silent configuration migration.

The stable locator retains schema-version-1 `relocation_history` with contiguous
positions and exact operation IDs, alongside its separate current `relocation`
projection. Each fact records catalog identity, lossless source/destination paths,
source revision, tagged SHA-256 evidence of the closed source database, and
checkpoint. Recovery requires source and copy bytes to match this recorded
evidence; a replacement with the same catalog ID and revision is insufficient.
Checkpoints are `intended`, `prepared`, `switched`
and `completed`. Prior bootstrap/storage-repair lifecycle facts and IDs remain
unchanged, including explicitly observed legacy facts. Relocation never rewrites
old event envelopes or worktree paths. Unsupported or inconsistent history stops
use. Ordinary mutation requires a completed relocation. Its pending reason is
`catalog_relocation_pending`; successful commands use `catalog_relocated`,
`catalog_relocation_completed`, or `catalog_relocation_already_completed`.
Inspection uses `catalog_relocation_observed`.

Inspect the exact recorded ID with `operation inspect <id>` or
`recover preview --operation <id>`. Preview and catalog information/check commands
preserve database and locator bytes. Operation listing retains catalog lifecycle
results even when repository storage is unavailable, and explains partial results.
Receipts expose active and selected paths, checkpoint, mutation availability and
next action independently. `worktrees_moved` is false. Applying an exact older
completed lifecycle result is a read-only no-op, even during a newer relocation.

Explicit recovery uses `recover apply --operation <id>`; `recover apply --catalog`
selects a pending catalog relocation. An unknown ID never selects another request.
The recorded source remains the authority before switching; the destination is
the only selected location after switching. Both stages bar ordinary mutations
until completion. Interrupted copies are never blindly overwritten or reset:

- After intent or directory creation, recovery can prepare the recorded destination.
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
- After completed publication, repeating exact-ID recovery changes no history.

Ambiguous files or inconsistent locator facts refuse mutation. Corrupt or
unsupported lifecycle metadata requires manual verification and reconciliation;
the tool never invents missing history, chooses a newer operation, or initializes
a replacement authority. Real subprocess crash tests demonstrate process
interruption behavior and cooperative locking, not whole-machine power-loss
recovery or protection from hostile same-user filesystem changes. SHA-256 evidence
strengthens accidental replacement detection, not hostile-filesystem isolation.

## Retire registrations and inspect retained resources

Retirement frees one registered record's count capacity after human removal. It
keeps the complete registration, assignment, operation, and event history. It never
removes a checkout, Git metadata, ignored files, preservation refs, commits,
external build state or caches. It never stops processes or runs build cleanup.

First release the latest assignment safely, or apply explicit worktree recovery
while the checkout still exists. Recovery must leave ownership unassigned and
establish safe preservation. Keep the exact completed release or recovery
operation ID. A completed reconciliation that retains active or preparing
ownership, or leaves availability withheld, doesn't authorize retirement.
Humans then remove the checkout and its linked Git worktree metadata using their
own Git or filesystem workflow. Confirm removal explicitly:

```sh
worktree-pool worktree retire <worktree-id> \
  --reconciliation <completed-release-or-recovery-operation-id> --removed
```

The command validates the latest generation's proof, absent registered checkout
and Git directory, and every required detached preservation root. Existing
files, ignored files, dangling links, substituted symlink ancestors, moved
worktrees, uncertain access and unresolved repository operations refuse retirement.
Missing files alone never release an assignment or free capacity. If removal
preceded safe reconciliation, restore the work and reconcile it safely first.
The removal assertion doesn't waive preservation or certify lost content.

An attached proof also requires its recorded branch to remain a direct reference
at the exact recorded tip. Deleted, symbolic, or moved branch protection refuses
retirement. Detached roots must remain direct exact refs in
`refs/worktree-pool/<operation-id>`. The tool doesn't repair or remove those refs
as part of retirement. Cooperative locks don't stop humans from changing files or
refs outside the tool. This isn't filesystem isolation.

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

Retirement JSON data includes `operation`, `retired`, `already_retired`,
`ownership`, `availability`, `resources` and `revision`. Its operation retains the
original worktree record, exact reconciliation identity and causal checkpoint.
Inspect exact retired worktree IDs with `registration_state: retired`.
Path selectors select only a current registration. A later registration can reuse
the same path with a new identity. Historical retired paths report unknown,
unattributed bytes rather than attributing a newer checkout's files to old history.
Inspect exact completed acquisition, release, and recovery IDs.
Repeated apply returns their historical result. Their retired observation is
explicitly historical and never examines a newer checkout at the recorded path.

Worktree inspection exposes `resources.worktree`,
`resources.known_external_build_state`, `resources.shared` and `resources.unknown`.
Repository inspection reports `resources.worktree_bytes`, per-worktree observations
and shared Git-common-directory usage once. Human output includes the same data.
The metric is `logical_regular_file_bytes`, with unique device/inode identities
counted once across a repository report. The tool measures common Git storage first.
It excludes checkout-root `.git` metadata from worktree bytes and counts each
regular file identity once, including repeated hard links. Separately requested worktree
reports aren't an additive repository total.

Measurements include ignored regular files but don't follow symlinks or cross
filesystem devices. Arbitrary Bazel convenience symlinks don't establish external
storage ownership. The pool has no authoritative external build-state roots, so
known external bytes are `null`, with `no_authoritative_build_state_roots`, and
undiscovered output bases, caches, servers and remote state remain unknown. No
Bazel invocation or ambient cache-directory discovery occurs during inspection.
This limitation avoids invented attribution or exclusive-usage claims.

Each observation includes its encoded path, status, and bytes. Missing,
inaccessible, unsafe, or incomplete roots have `bytes: null`. Partial traversal
reports its observed subtotal separately as `measured_bytes`. Skipped paths
carry explicit reasons. Measurements aren't filesystem snapshots and can change
while humans or build processes run. Logical lengths don't measure allocated
blocks, compression, reclaimable space or process memory. Inspection keeps
catalog and locator bytes unchanged and performs no cleanup. Capacity limits
registered count rather than bytes, and retirement makes no claim of external
resource removal.

Retirement verification uses private real Git/filesystem/redb fixtures and public
command-line inspection, exact operation recovery, real subprocess SIGKILL boundaries, and
actual backend sync failures. The commit-fault observer arms the existing real
backend immediately before the same production transaction commit. Earlier
storage errors aren't classified as failed commits. Reopen checks complete old or
complete new history and projection before any retry. These checks don't establish
whole-machine power-loss behavior or a performance benefit.
