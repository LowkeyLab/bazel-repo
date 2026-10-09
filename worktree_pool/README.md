# Worktree pool

A synchronous Linux x86-64 command-line tool for explicitly managed persistent
Git worktrees. It supports catalog initialization and inspection, explicit
repository/worktree enrollment, registered resource inspection, checkpointed
repository refresh, safe acquisition and explicit release of registered worktrees,
assignment and operation inspection, event history, on-demand detached creation,
durable per-repository capacity, explicit recovery and projection rebuild, catalog
relocation, registration retirement, and retained resource inspection.

Runtime support is Linux x86-64 with Git 2.36+ and ordinary local
linked-worktree repositories. The executable and standalone package use the name
`worktree-pool`, version 0.1.0. See [installation](#standalone-source-installation)
and [command schemas](WIRE_SCHEMA.md). For development, implementation details,
package verification, and benchmark evidence, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Standalone source installation

Source installation needs Linux x86-64, Rust/Cargo 1.96+, and a native linker/C
toolchain. Runtime needs Git 2.36+ and the resulting native C runtime libraries.
Running the executable doesn't require Rust, Cargo, or Bazel.

The package is ready for publication review. After a maintainer publishes version
0.1.0, the intended registry command is:

```sh
cargo install --locked --version 0.1.0 worktree-pool
```

Before publication, install from an extracted verified `worktree-pool-0.1.0.crate`:

```sh
cargo install --locked --path /path/to/worktree-pool-0.1.0
worktree-pool --version
```

Installation never initializes or migrates catalogs. Follow the quick start to
initialize one catalog, register a repository, and acquire from any directory.

## Quick start

Initialize one catalog, register a repository, and acquire a worktree:

```sh
worktree-pool catalog init
worktree-pool repo register /absolute/repository
worktree-pool acquire --repo /absolute/repository
```

Use the returned assignment's absolute path and retain its `assignment_handle`.
Acquisition refreshes `origin/main` and returns a detached checkout. Create a
branch with Git if your work needs one. The pool reuses a safe registered worktree
or creates one when capacity permits.

Stop working in the checkout. Finish or commit any unfinished changes. Then release
using the exact handle:

```sh
worktree-pool release <assignment-handle>
```

Ignored files remain for reuse. Process death and elapsed time never release an
assignment. If a command stops unexpectedly or you lose its result, inspect its recorded
identity and use [explicit recovery](#explicit-recovery-and-lost-results).

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

Interrupted initialization remains pending. Repeating initialization can't
overwrite catalog data or repair pending work. Use `catalog info` to inspect the
recorded state, then preview explicit catalog recovery before applying it.
Unsupported catalog history stops use.

## Check and rebuild derived state

```text
worktree-pool catalog check
worktree-pool catalog rebuild
```

`catalog check` validates storage without changing it. `catalog rebuild`
reconstructs derived state from the complete immutable event history. It retains
registrations, capacity, assignments, preservation references, pending operations,
and recovery outcomes. It never fetches, checks out, creates, releases, or resumes
work. Rebuild is catalog-wide and rejects a repository selector.

Rebuild waits for live pool commands. History and the authoritative revision stay
unchanged. Missing registered paths don't prevent rebuilding. Unsupported history,
identity conflicts, or corrupt authority refuse use. Rebuild never initializes a
replacement catalog or performs an implicit schema upgrade.

After an interrupted or unknown result, inspect the catalog before requesting
another rebuild. If storage requires physical repair, use
`recover preview --catalog` and `recover apply --catalog`. Repair can complete
while derived damage remains: `store_state: rebuild_required` and
`catalog_mutations_available: false` then require `catalog rebuild` before use.

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
`UUID`. `context.catalog_path` is a lossless path. Catalog initialization and rebuild
`data` contain `catalog_id`, numeric `revision`, `store_version`, and
`projection_version`. `catalog info` and `catalog check` also include authority observations.
Relocation and recovery use distinct [receipt variants](WIRE_SCHEMA.md).
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
`capacity_no_safe_worktree`, `retirement_preconditions_unmet`,
`catalog_relocation_pending`,
`operation_pending`, `refresh_failed`, `worktree_unavailable`,
`retained_file_collision`, `unfinished_work`, `git_operation_in_progress`, and
`unsupported_index_state`, `assignment_unknown`, `already_released`, and
`already_retired`. [The command schemas](WIRE_SCHEMA.md#top-level-reason-vocabulary-version-1).

A broken stdout returns status 4 and sanitized stderr guidance. This doesn't
undo a committed outcome or retry initialization. Inspect the catalog to learn
what committed. Diagnostics exclude configuration contents and raw storage
errors, keeping credentials and file contents out of output.

Interrupted initialization inspection remains nonzero/pending. It includes the
recorded catalog ID/path, phase, last observed durable checkpoint, store state,
and available revision. A valid committed store under a pending locator yields
`store_committed`. Inspection never publishes or retries initialization.

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
Git membership and retains the existing absolute path, permissions, checkout
files, and ignored build state.

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
change checkout files, prune registrations, or expose other worktrees. Independent repositories can refresh concurrently.

Refresh data contains its `operation`: operation/repository identities,
`intent_event_id`, `state`, `last_checkpoint`, and nullable `resolved_commit`.
Success records `completed`/`result_committed`. A failed fetch or commit
resolution records `needs_reconciliation`/`fetch_failed` and returns
pending/`refresh_failed`. Interruption before result commitment retains
`pending`/`intent_recorded`, even when Git actually completed. Repeating a pending
refresh returns the existing operation without retrying effects or changing
history. Use explicit recovery after inspecting the recorded identity. Lost stdout doesn't
undo a completed refresh. Inspect the recorded repository operations.

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

Preparation preserves old detached tips under unique
`refs/worktree-pool/<operation-id>` references. Checkout disables hooks, creates
no branch, retains ignored files, and never forces, resets, stashes, or cleans up.
Competing pool commands can't acquire the same worktree.

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
conflicting effects until explicit reconciliation. Lost stdout returns unknown/status 4 while committed ownership remains.

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

## Capacity and on-demand creation

```text
worktree-pool pool configure --repo <repository-id-or-path> --max-worktrees 4
```

The maximum defaults to four and persists across commands.
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

Creation uses ordinary detached Git worktree creation at the fixed commit. It
refuses existing destinations and conflicting Git metadata. It never forces,
resets, stashes, creates a branch, or runs arbitrary setup hooks.

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

Release keeps idle worktrees and ignored files for reuse.

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
reachability references before ending ownership. Final validation
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
references. Recovery observes existing roots before any retry.
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

A pending recovery blocks competing acquire/release/refresh for its repository
after process death. Independent repositories can still progress. Catalog recovery
IDs remain available for inspection even while repository storage is pending.
An operation list explains when repository results are unavailable.

Ordinary initialization never repairs or replaces an existing catalog. Explicit
catalog recovery resumes recognized initialization or storage-repair checkpoints.
It rejects conflicting, corrupt, or unsupported authority. Repair of an unclean
database may change physical storage bytes even when identity validation later
rejects it. Preserve files when their identity is uncertain.

## Catalog relocation

`worktree-pool catalog relocate <destination-directory>` copies the closed catalog
database to a new private directory and atomically switches the stable active
locator. The destination must not exist, its parent must exist without symbolic
path aliases, and the destination must be separate from source storage, stable
state, registered checkouts and Git common directories. Relative destinations
resolve against the current directory. The tool rejects parent traversal. New tool
directories use mode 0700 and files 0600. Configuration files and environment
settings are never rewritten.

After completion, select the destination explicitly with `--catalog-dir` or update
your own location setting. Obsolete selectors return `catalog_conflict`, include
the actual active location and explain the next action. They never adopt the old
database. The stable catalog locator and coordination files stay under the original
state directory. Relocation excludes concurrent commands
for its entire duration, including commands preparing Git effects. It rejects
unresolved catalog, repository, and worktree operations. Completed assignments may
remain active throughout relocation.

Relocation preserves catalog identity, handles, capacity, preservation records,
event history and ownership. Existing absolute checkout paths remain
fixed, including worktrees created beside the old catalog. The tool never moves,
deletes, or cleans worktrees. The source database also remains in place as inactive tool
data. Each relocation retains another database copy and consumes additional disk.
Receipts report `retained_source`, `retained_source_active`, and recorded
`retained_source_bytes` measured at Intended after the Started-bearing source
closes. `retained_source_bytes_checkpoint` identifies that checkpoint. An absent
recorded measurement is null. This is neither a current/live measurement nor a
claim of reclaimable bytes. Historical receipts don't probe inactive paths.
Later manual removal, replacement, or repair can't change the recorded value.
Earlier retained locations remain recorded in lifecycle history. There is no
automatic deletion, backup/restore subsystem or silent configuration migration.

The pending reason is `catalog_relocation_pending`. Successful commands use
`catalog_relocated`, `catalog_relocation_completed`, or
`catalog_relocation_already_completed`. Inspection uses
`catalog_relocation_observed`. Unsupported older relocation metadata requires
manual preservation and reconciliation. The tool never migrates it automatically.
Missing recovery evidence requires restoration of the exact recorded operation.

Inspect the exact recorded ID with `operation inspect <id>` or
`recover preview --operation <id>`. Preview, `catalog info`, and `catalog check` commands
preserve database and locator bytes. Operation listing retains catalog lifecycle
results even when repository storage is unavailable, and explains partial results.
Receipts expose active and selected paths, checkpoint, mutation availability and
next action independently. `worktrees_moved` is false. Applying an exact older
completed lifecycle result is a read-only no-op, even during a newer relocation.

Explicit recovery uses `recover apply --operation <id>`. `recover apply --catalog`
selects a pending catalog relocation. An unknown ID never selects another request.
The recorded source remains the authority before switching. The destination is
the only selected location after switching. Both stages bar ordinary mutations
until completion. Interrupted copies are never blindly overwritten or reset:

If a destination copy is incomplete or conflicting, preserve the conflicting
file outside the destination directory before recovering the same operation ID.
After copy preparation or authority switching, verify the recorded files. Missing
or changed files require manual restoration of the exact verified source or destination bytes, with
conflicting bytes preserved first. Recovery revalidates the recorded evidence.

If interrupted relocation requires storage repair, use recovery for that exact
relocation operation ID. `catalog check`, `catalog rebuild`, `recover preview`, and ordinary open commands
don't repair relocation storage. Unknown or mismatched history requires manual
preservation and restoration.

Ambiguous files or inconsistent catalog state refuse mutation and require manual
verification and reconciliation. The tool never invents missing history, chooses
a newer operation, or initializes a replacement catalog. Coordination protects
cooperating pool commands. It doesn't isolate files from other same-user processes
or guarantee whole-machine power-loss recovery.

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

`retirement_preconditions_unmet` rejects with exit code 2. An interrupted
retirement still counts toward capacity. Inspect its exact operation ID or use
`recover preview --operation <retirement-id>` and
`recover apply --operation <retirement-id>`. Apply revalidates removal and
preservation before completion. Only completed retirement frees capacity.
Unknown identities never select the newest operation.

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

## Retained resource costs

Registered capacity limits worktree count, not disk or memory. Retained worktrees,
build output bases, caches, and server state can cost much more than the catalog.
Retirement never deletes those resources. Inspect known and unknown usage and
plan explicit retention decisions.

The maintainer accepted a measured adoption gate on 2026-10-09. The
[benchmark evidence](CONTRIBUTING.md#accepted-benchmark-and-resource-costs)
records its scope, timings, retained costs, and limits. Results aren't a promise
of faster builds in other repositories.
