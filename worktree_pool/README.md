# Worktree pool

A synchronous Linux x86-64 command-line tool for explicitly managed persistent
Git worktrees. It supports catalog initialization and inspection, explicit
repository/worktree enrollment, registered resource inspection, checkpointed
repository refresh, safe acquisition and explicit release of registered worktrees,
assignment and operation inspection, and event history. On-demand creation
follows in a later slice.

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
`io.lowkeylab.worktreepool.*.v1` type namespace, catalog source URI, unique
source/id identity, subject, recording time in `UTC`, `application/json` typed
data, and string `operationid`. Affected-entity subjects remain separate from
stream routing. Refresh results carry `causationid` identifying their intent
event, which replay validates. Global committed positions determine history
order. Recorded stream identifiers and expected revisions enforce independent
catalog/repository sequences. Repository enrollment belongs to the catalog
stream. Worktree enrollment and refresh belong to their repository stream. `redb` transactions atomically update events, revision,
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
slices verify creation, recovery, and standalone installation.

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
appropriate. Repository inspection also reports `registered_count` and
`operations`. Worktree results separately expose `registration_state`
(`registered`, `missing`, `mismatched`, `unreadable`), `ownership` (`unassigned`,
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

## Acquire existing registrations

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
position. Completed release facts supply actual release positions. This slice never
creates a worktree when none is available.

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
list/inspect includes refresh, acquisition, and release records. Acquisition records carry
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
