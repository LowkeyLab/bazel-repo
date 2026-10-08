# Worktree pool

A synchronous Linux x86-64 command-line tool for explicitly managed persistent
Git worktrees. This slice implements `catalog init`, `catalog info`,
`catalog check`, and `events list`. Later slices add repository and ownership
commands.

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
Errors have `data.next_action`, safe human guidance. Later slices expand context
identities independently for repositories, worktrees, assignments, and operations.
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
`filesystem_error`, `storage_error`, and `commit_unknown`.

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
data, and string `operationid`. Ordered positions and expected revisions
determine order. `redb` transactions atomically update events, revision,
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
preventing the handle from escaping its lock lifetime. Later repository
operations must acquire the repository lock before the catalog lock.

The narrow initialization callback observes completed durable checkpoints. The
normal command supplies a no-op observer and performs the same effects. The
test-only helper signals each checkpoint, then waits until its parent kills it
under a bounded watchdog. Production has no environment test mode.

The public command and store/domain/listener Bazel targets pass on Rust 1.96.
They cover authority refusal, immutable history, configuration/path/output
contracts, commit uncertainty, listener isolation, concurrent initialization, and
actual process death at initialization checkpoints. The missing-catalog and
human-identity tests failed before implementation. Stable public results support
refactoring resistance, but no refactor-survival experiment accompanies this
slice. Fault tests establish commit/error semantics and don't claim simulated
power-loss durability. Normal processes establish kernel-lock behavior. Later
slices verify workflow crashes, broader concurrency, and standalone installation.
