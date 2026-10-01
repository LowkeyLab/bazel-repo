# Book Smartz PostgreSQL Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide durable Rust operations for registering books and executing and recovering each reader's ranking decisions in PostgreSQL.

**Architecture:** Keep the existing pure ranking aggregate and replay codec. A concrete SQLx store coordinates per-reader transactions and persists CloudEvents; a separate observer translates completed operation facts into diagnostics. Read rankings by replaying their authoritative histories.

**Tech Stack:** Repository-resolved Rust, Tokio, SQLx/PostgreSQL, CloudEvents, googletest, testcontainers, tracing, Nix/Bazel/Aspect.

**Spec:** [Approved persistence design](../specs/2026-10-01-book-smartz-postgres-design.md), including the domain-ownership revision committed in `0e6eb7b4`.

## Global Constraints

- PostgreSQL; database-backed Rust operations only. No API server, authentication, account profiles, UI, external catalog fetches, snapshots, broker, or automatic command retries.
- Existing `BookId`, `ReaderId`, `OpenLibraryWorkId`, `CommandContext`, commands, event profile, and pure domain remain authoritative.
- Caller-supplied IDs and UTC instants remain explicit; sequence, rather than timestamp, determines order.
- Store sequence as a checked 20-digit, zero-padded decimal string, preserving the entire domain `u64` range.
- Enforce uniqueness of `(reader, sequence)` and `(reader, event ID)`.
- The caller constructs and owns a configured PostgreSQL pool and its shutdown.
- Ordinary reads and writes never migrate. Migration bookkeeping is isolated in the `book_smartz` schema.
- Listener delivery errors cannot replace a database result. Successful write observations follow acknowledged commit.
- Use Bazel for all development operations; do not use Cargo directly or install tools.
- After EVERY source-file edit, immediately run `nix develop --command bazel run //:gazelle` before any further edit or formatting. Run Gazelle before manually editing any BUILD file.
- Before each task commit, run `nix develop --command aspect format --scope=all` and `git diff --check`; stage only task files. Source changes trigger the Gazelle rule even for tests.
- Run focused tests for each task, then the full build and lint before completion. Never infer test execution from an empty filtered run.
- Reuse this isolated worktree and `codex/book-smartz-postgres`; preserve unrelated work if any appears.

## Review Focus

These implied edge cases supplement the main acceptance scenarios; each is assigned below.

1. A reader's very first command races another first command: exactly one event commits, the other is stale (Task 3).
2. SQL NULL/missing envelope fields bypass a CHECK, or large unsigned sequences narrow to signed values: reject invalid rows and preserve `u64::MAX` (Task 1).
3. Migration connection state leaks into pooled application work: unrelated bookkeeping and later connections remain unaffected (Task 1).
4. A repeated event ID with a fresh revision differs from a stale retry: preserve domain rejection precedence and never double-apply (Tasks 3–4).
5. A diagnostic listener fails after the database commits: callers still receive success and fresh loads see the event (Task 5).

## File and interface map

Create `book_smartz/storage/` with focused files:

- `lib.rs`: public exports, store construction, public operation wrappers.
- `error.rs`: typed storage errors and safe classification.
- `migration.rs`: schema bootstrap and isolated SQLx migration execution.
- `books.rs`: identity registration and lookups.
- `ranking.rs`: aggregate reconstruction and transactional command execution.
- `wire.rs`: checked sequence and stored-envelope conversions.
- `observation.rs`: structured facts, observer contract, protected dispatch.
- `tracing_listener.rs`: structured tracing mapping only.
- `tests/{mod.rs,support.rs,migration_tests.rs,book_tests.rs,ranking_tests.rs,recovery_tests.rs}`: database tests.
- `observation_tests.rs`: fast producer/adapter mapping checks where no database is required.
- `tests/fixtures/ranking-history-v1.json`: fixed historical wire fixture, not regenerated during tests.
- `BUILD.bazel` and `README.md`: crate/test wiring and caller usage.

Create `book_smartz/migrations/{001_initial.sql,BUILD.bazel}`. Keep `book_smartz/domain` unchanged unless a demonstrated integration constraint requires a separately reviewed minimal change. No new third-party dependencies are planned.

Public interfaces to implement (type names shared across tasks):

```rust
pub struct Store { /* private pool and observer */ }
pub type SharedObserver = std::sync::Arc<dyn Observer>;
pub struct OperationContext { pub correlation_id: Option<uuid::Uuid> }
pub enum Registration { Created, AlreadyPresent }
pub struct CommandResult {
    pub event: book_smartz_domain::RankingEvent,
    pub projection: book_smartz_domain::RankingProjection,
}
pub trait Observer: Send + Sync {
    fn observe(&self, observation: &Observation) -> Result<(), ObservationDeliveryError>;
}
impl Store {
    pub fn new(pool: sqlx::PgPool, observer: SharedObserver) -> Self;
    pub async fn migrate(&self, operation: OperationContext) -> Result<(), StoreError>;
    pub async fn register_book(&self, book: &Book, operation: OperationContext)
        -> Result<Registration, StoreError>;
    pub async fn book_by_id(&self, id: BookId, operation: OperationContext)
        -> Result<Option<Book>, StoreError>;
    pub async fn book_by_work_id(&self, id: &OpenLibraryWorkId, operation: OperationContext)
        -> Result<Option<Book>, StoreError>;
    pub async fn load_ranking(&self, reader: ReaderId, operation: OperationContext)
        -> Result<Ranking, StoreError>;
    pub async fn execute(&self, reader: ReaderId, context: CommandContext,
        command: Command, operation: OperationContext) -> Result<CommandResult, StoreError>;
}
```

`OperationContext` derives `Default` with no correlation ID. `StoreError` retains
`Identity(IdentityError)`, `Domain(DomainError)`, `CorruptHistory`, `Database`,
`Migration`, and `CommitUncertain` categories, with causal errors where available.
Stale revision and duplicate-event rejection remain recognizable through the
existing domain variants; do not change domain error precedence to flatten them.
Display summaries must not format raw database causes.

`Observation` contains a typed `Operation` (migration, registration, either lookup,
ranking load, or ranking command), typed `Outcome`, `Duration`, optional correlation,
reader/event IDs, revision, and ranking-load event count. Outcomes include applied,
created, already-present, found, absent, loaded, committed, rejected, failed, and
commit-uncertain. Rejections and failures carry bounded enums, not strings. A
successful migration can use `Applied` for the idempotent ensure-schema operation;
do not claim a migration was newly installed when it was already current.

Private database operations return `Result` to one observation wrapper, preventing
duplicate success/failure emissions. Construct the complete observation contract
in Task 1 and wire producers as operations are added. Task 5 adds the production
tracing listener and proves dispatch failure containment.

## Task 1: Schema, migration isolation, and store foundation

**Files:** Create migration files, storage `lib.rs`, `error.rs`, `migration.rs`,
`wire.rs`, `observation.rs`, `tests/mod.rs`, `tests/support.rs`,
`tests/migration_tests.rs`, and `BUILD.bazel`.

**Interfaces:** Produce `Store::new`, `Store::migrate`, error/observer types above,
and private `sequence_key(value: u64) -> Result<String, StoreError>` and
`parse_sequence_key(value: &str) -> Result<u64, StoreError>`.
Test support provides `Fixture::new().await`, `pool() -> &PgPool`, and
`store(observer: SharedObserver) -> Store`; its container lives until test end.
A test-only `Recorder` holds structured observations behind a mutex.

- [ ] Add failing `migrations_are_isolated_and_repeatable`: create unrelated `public._sqlx_migrations` state, migrate twice, assert the sentinel is unchanged and `book_smartz._sqlx_migrations` records version 1. Reacquire connections and assert their search path has not changed. Add concurrent migration coverage.
- [ ] Add failing `sequence_range_and_required_metadata_are_enforced`: assert `sequence_key(u64::MAX) == "18446744073709551615"`; reject zero, short strings, nondigits, and `"18446744073709551616"`. Use real inserts to reject absent/null required JSON identities and mismatched sequence, candidate, reader, opponent, or event ID.
- [ ] Establish `//book_smartz/storage:storage_test` using the existing `prediction_bot/lib:store_test` pattern, `image_data("postgres_18")`, `image_env("postgres_18")`, `//test_images/rust:test_images`, and the same Docker execution properties. Fixture uses `test_images::postgres().await.start().await`; no image tag or network-pull fallback. Run the target and record the intended compilation/behavior failure before implementation.
- [ ] Implement schema bootstrap on an acquired connection marked `close_on_drop()`, with fixed migration search path and SQLx `Migrator` embedded SQL, following `prediction_bot/lib/src/store.rs`. Ensure safe concurrent schema initialization using a fixed transaction-scoped bootstrap lock. SQLx bookkeeping must be in `book_smartz`; fully qualify application queries. Verify these concrete APIs against resolved source during implementation.
- [ ] Implement books, reader-stream, and event tables. Use C-collated fixed-width text sequence with positive/u64-bound checks. Use foreign keys for reader, candidate, and optional opponent, unique keys from the spec, and explicit non-null predicates for required envelope fields (SQL CHECK's unknown result must not admit missing data). Compare both padded extension and decimal payload sequence. Do not require an opponent for non-comparison events.
- [ ] Implement safe errors, store/observer foundation, and migration observations. Bootstrap must create no runtime login, password, or global role. Embed SQL via Bazel `compile_data`; supply SQLx, domain, Tokio, tracing, UUID, chrono, serde_json, and thiserror dependencies only where used.
- [ ] Run `nix develop --command aspect test //book_smartz/storage:storage_test`; expect all new migration tests executed and passing. Format, inspect diff, and commit `feat(book-smartz): add isolated PostgreSQL schema and migrations`.

## Task 2: Durable book registration and identity lookup

**Files:** Create `storage/books.rs`, `storage/tests/book_tests.rs`; update storage
exports, test module, observations, and BUILD wiring after Gazelle.

**Interfaces:** Consume `Store`, errors, observer, fixture, and migrated books table;
produce the three book operations in the interface map.

- [ ] Add failing `registration_preserves_both_identity_directions`, using Book UUID 1 with `OL1W`: first registration is `Created`, repeat is `AlreadyPresent`, both lookups return that Book, unknown lookups return `None`. Register UUID 1/`OL2W` and UUID 2/`OL1W`; assert the respective identity errors and unchanged lookups.
- [ ] Add failing `concurrent_registration_is_idempotent`: two simultaneous identical calls yield one created and one already-present; conflicting calls leave exactly one valid mapping. Include the both-identities-conflict case and require book-ID conflict precedence.
- [ ] Run the storage target and verify these tests fail for missing book operations.
- [ ] Implement bound SQL queries and transaction-based insertion/conflict resolution without overwrites. Validate work IDs loaded from storage. Acquire an explicit transaction for registration so commit uncertainty can be classified consistently in Task 4. Decode malformed stored identity as corruption, not a legitimate absent book.
- [ ] Emit one typed completion observation per operation. After closing and reconnecting the pool, assert both identity lookups still work. Assert conflicting registration never emits created success.
- [ ] Run the storage target; expect all book and migration checks passing. Format, inspect diff, and commit `feat(book-smartz): persist registered book identities`.

## Task 3: Durable ranking decisions and concurrent writers

**Files:** Create `storage/ranking.rs`, `storage/tests/ranking_tests.rs`, historical
fixture; update `wire.rs`, exports, test modules, and BUILD `compile_data`.

**Interfaces:** Produce `load_ranking` and `execute` from the interface map. Consume
existing `Ranking::from_history`, `Ranking::execute`, codecs, book lookup helpers,
and sequence conversion. Private `decode_row` validates metadata against the decoded
CloudEvent and returns a `RankingEvent` or corruption error.

- [ ] Add failing `reconnect_recovers_ranking_and_pending_placement`: register A/B/C; rank A, begin B, skip A to pause, reconnect and assert revision 3, ranked entries `[A]`, pending B, paused state, original timestamps, and preserved skipped/bound state. Resume then prefer B over A; assert `[B, A]` and no pending placement. Compare the complete recovered projection, not only list length.
- [ ] Add failing `first_and_existing_stream_races_reject_stale_writers`: concurrently start A/B at revision zero with different event IDs; assert one success, one `StaleRevision`, one event. Repeat at an existing stream revision with two valid competing commands. Use barriers, not sleeps.
- [ ] Add failing `reader_streams_and_duplicate_event_rules_are_independent`: two readers may use the same event UUID; within one stream a reused ID at the current revision is a duplicate domain error, and a stale retry retains stale-revision precedence. Invalid commands leave history and revision unchanged.
- [ ] Run the storage target and confirm ranking tests fail before implementation.
- [ ] Implement the spec's read-committed transaction: insert stream row on conflict do nothing, lock it, read ordered history, rehydrate, load only command-referenced registered books, execute through the aggregate, validate/encode the new event, insert, commit, then return event/projection. Roll back a rejected first command's stream creation. Normal load uses one ordered query and returns an empty ranking for an unknown reader without writing.
- [ ] Add `historical_v1_history_remains_replayable` with fixed pre-storage CloudEvents JSON derived from existing domain examples. Include an envelope without the optional padded extension and one with valid opaque extensions; validate through the existing codec before inserting canonical stored form. Reload must preserve decisions/timestamps and optional stored extensions; do not invent a production import endpoint.
- [ ] Add privileged corruption fixtures for a sequence gap and semantically invalid transition that satisfy SQL shape checks. Assert load returns `CorruptHistory`, never partial state. Assert notices are not extra stored decisions and replay emits only one load observation, no command outcomes.
- [ ] Run `nix develop --command aspect test //book_smartz/domain:domain_test //book_smartz/storage:storage_test`. Format, inspect diff, and commit `feat(book-smartz): persist ranking decisions with reader concurrency control`.

## Task 4: Rollback, commit uncertainty, and recovery

**Files:** Create `storage/tests/recovery_tests.rs`; update `error.rs`, `books.rs`,
`ranking.rs`, and test support only as needed for the recovery contract.

**Interfaces:** Preserve public signatures. Classify acknowledged database commit
rejection as database failure and transport/acknowledgment loss during commit as
`CommitUncertain`. No production test-mode switches or retry loop.

- [ ] Add failing `failed_append_rolls_back_the_first_stream`: a test-installed constraint/trigger rejects insertion after stream creation; assert failure, no stream/event remains, and no committed observation. Existing-stream failure likewise leaves prior history unchanged.
- [ ] Add failing `closed_pool_and_definite_commit_failure_are_safe`: close a fixture pool and exercise reads/writes; assert typed failures with no credential text in Display/observations. Use a deferred test constraint to force an acknowledged COMMIT rejection and verify definite failure rather than uncertainty.
- [ ] Run the storage target and record the intended failures, then implement phase-aware error classification without discarding underlying causes.
- [ ] Implement test-only transport fault control in `tests/support.rs` using existing Tokio network facilities and a PostgreSQL connection with TLS disabled only in the fixture. A protocol-aware proxy forwards COMMIT but suppresses its result before breaking the client connection. Coordinate with protocol messages/barriers, not timing sleeps; establish that PostgreSQL committed through a separate direct connection.
- [ ] Add `lost_commit_acknowledgment_is_recoverable`: observe `CommitUncertain`, find the original event and contents via direct reload, retry the original context, and assert still exactly one event. Add the corresponding registration case: recovery by identical registration yields already-present. Unit error mapping alone is not evidence of this network behavior; if the fault mechanism cannot run, explicitly report that coverage gap as the spec permits.
- [ ] Add cancellation recovery coverage while commit is in flight. No completion observation is required for a dropped future; reconnect and safely resolve/retry using the original context, checking that no duplicate decision can result.
- [ ] Run the storage target and retain executed fault evidence. Format, inspect diff, and commit `fix(book-smartz): distinguish uncertain commits and preserve recovery`.

## Task 5: Diagnostic listener, caller documentation, and acceptance

**Files:** Create `storage/tracing_listener.rs`, `storage/observation_tests.rs`,
`storage/README.md`; update observation dispatch, lib exports, BUILD wiring,
and integration producer assertions.

**Interfaces:** Produce `pub fn tracing_observer() -> SharedObserver`. Reuse the
observer contract and facts from prior tasks; no global subscriber installation.
Add `//book_smartz/storage:observation_test` as a fast test target without Docker.

- [ ] Add failing `structured_listener_maps_outcomes_and_safe_context` with an in-memory tracing subscriber/Layer recording fields and levels. Assert spec severities, correlation, reader/event IDs, revision, count, and duration in milliseconds. Assert sentinel secret/raw payload values never occur in captured fields. Do not assert rendered strings or JSON diagnostics.
- [ ] Add failing `observer_failure_cannot_change_committed_result`: use recording, error-returning, and unwinding-panic observers with real command commits, registration, loads, and migration. Assert operation results are preserved and fresh loads agree. Verify bounded sanitized fallback through an internal test seam, never recursive observer calls.
- [ ] Run observation and storage targets and confirm intended failures. Implement the tracing listener and protected synchronous dispatch; normal observer failures produce one minimal sanitized fallback, without raw panic/error text. Contain unwinding panics where supported; document that aborts cannot be caught.
- [ ] Exercise public construction with the real tracing listener, caller subscriber, and PostgreSQL: the first operation is observed, command success appears only after commit, rejected/rolled-back work is not committed activity, and load/replay does not re-emit old decisions. These are composition checks, not constructor-call mocks.
- [ ] Write a compilable usage example showing explicit migration, caller-owned pool, acquisition/lock/statement timeout configuration, scoped subscriber, book registration, command execution, load, and shutdown. Explain registration versus ranking, uncertain-commit recovery using the original context, linear replay cost, cancellation gap, telemetry best effort, and absence of account authentication. If written as a Rust source fixture, obey Gazelle and compile it with Bazel.
- [ ] Run `nix develop --command aspect test //book_smartz/domain:domain_test //book_smartz/storage:storage_test //book_smartz/storage:observation_test`. Verify intended test names/counts executed, including fault tests or their explicit reported gap.
- [ ] Run `nix develop --command aspect format --scope=all`, `nix develop --command aspect build //...`, `nix develop --command aspect lint`, and `git diff --check`. Fix attributable failures; report non-blocking warnings and environment failures accurately. Recheck focused tests after behavior changes.
- [ ] Commit `feat(book-smartz): add persistence diagnostics and usage documentation`; review the complete branch against the approved spec. Do not publish a PR or deploy without a user request.

## Self-review and handoff

All spec sections map to tasks: migrations/unsigned wire boundaries (1), catalog
identity (2), aggregate ownership/replay/concurrency/evolution (3), failure and
recovery (4), observations/composition/documentation and repository checks (5).
Review Focus conditions have explicit tests above. Database tests remain real
managed-dependency integration checks; only diagnostic consumers and transport
faults use test substitutes. Proposed test speed and network fault coverage are
not execution evidence.

This plan is prepared for user review. No production implementation or tests have
run during planning. Select native execution or subagent-driven execution after
review; preserve that choice for the implementation phase.
