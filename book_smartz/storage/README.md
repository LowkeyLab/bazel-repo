# Book Smartz PostgreSQL storage

This crate persists registered book identities and each reader's authoritative
ranking decisions. It provides database operations, not an API server or account
authentication. `ReaderId` values identify streams. The caller enforces access control.

The caller constructs a PostgreSQL pool, configures its limits, supplies an observer
before the first operation, explicitly migrates, and owns pool shutdown. See
[the compiled usage example](usage/lib.rs) (`//book_smartz/storage:usage`) for the complete
sequence, including acquisition, statement and lock timeouts and an async-scoped
tracing subscriber. Supply the URL through caller configuration. The crate doesn't read environment variables. Ordinary operations never migrate. Migration
bookkeeping resides in `book_smartz`, separate from other applications.

Registering a book establishes the immutable `BookId`/Open Library work mapping.
It doesn't rank it. Registration remains committed if a later ranking command
fails. Starting the first book immediately ranks it. Later placements may remain
pending for comparisons. Loading replays ordered history, including pending,
paused, and skipped-opponent state, without publishing old decisions again.
Loads and commands take work proportional to a reader's history. There are no persisted
snapshots or constant-time replay guarantees.

Commands use caller-supplied event IDs, expected revisions and instants in Coordinated Universal Time.
Sequence determines order. Concurrent commands for one revision serialize. One
can commit and the store rejects the other as stale. No automatic retry or implicit
conversion of duplicate/stale rejection into success occurs.

After `StoreError::CommitUncertain`, reconnect to the authoritative database and
load history. Match the original event ID **and intended contents**. Absence in one
read doesn't prove an in-flight commit can't finish. Retrying preserves the
original command, event ID, expected revision and instant. Stream locking,
revision checking and uniqueness prevent applying a command twice. You can repeat identical book
registration. Cancellation while awaiting commit has the same
recovery risk but can't return a completion error or guarantee an observation.

## App command composition

`load_ranking_documents(reader)` returns complete validated CloudEvents, including
optional context and extensions. It checks the stored indexes and strict domain
replay before returning any history. Existing `load_ranking` and `execute` callers
retain their established behavior.

`begin_command(reader)` acquires the same reader-stream lock as `execute` and
returns a `CommandTransaction`. Its `ranking_documents()` and
`application_history()` expose complete locked histories without another pool
connection. Read `ranking()`, execute the pure domain command,
attach valid context to its envelope, and call `append_ranking(document,
expected_revision)`. The transaction validates the revision and full replay and
accepts at most one ranking decision. Book registration stays independent.

App facts use `ApplicationEvent::decode` with source
`urn:book-smartz:application:<reader UUID>`, a canonical `Uuid` event ID, JSON object
data, occurrence time, and a positive padded 20-digit `sequence` extension.
Parsing rejects duplicate JSON members at every nesting level before normalization.
It also rejects JSON numbers whose decimal value would change during storage,
including rounded large integers, precise fractions, and underflow. Equivalent
exponent spellings and signed zero retain their numeric meaning. The app owns the data schema and reconstruction of frozen intent, prepared
facts, and rejection. `load_application_history(reader)` returns these immutable
facts in a contiguous stream with its own sequence, independent of ranking
revision. Source plus ID distinguishes app and ranking identities.

A composition scope accepts at most one app fact through
`append_application`. It can commit that fact alone before an effect, or stage a
ranking decision and related fact in one transaction. Both remain invisible until
`commit` succeeds. Any append error or cancellation poisons the scope. Attempting
to commit it fails and dropping it schedules rollback. Never interpret a staged
result as saved. Keep provider requests outside the scope and configure pool and
lock timeouts to bound lock ownership.

The new composition and envelope-read operations emit no diagnostics. Loading
history performs no commands and publishes no activity. The app caller
owns privacy-safe completion reporting and recovery: inspect the complete original
ranking envelope after uncertainty and compare identity and contents. Application
intent alone never proves ranking success, and absence from one read never proves
rollback. Cancellation while awaiting `commit` requires the same reconciliation.

Migration 2 adds app history without changing migration 1 or existing
ranking tables. The previous storage API continues to work after migration. Keep
this additive schema and its durable history when reverting a caller.

`tracing_observer()` maps typed completion facts into structured records using the
caller's current subscriber and span. Creation, migration, and committed commands
are info. Reads, idempotent registration and expected rejection are debug.
Failures and commit uncertainty are error. Duration is numeric milliseconds.
Reader, event, and correlation IDs are diagnostic fields. Records include command kind, revision,
and load event count when known. The library installs no metrics or exporters.
These are completion records, not reconstructed operation spans.

Delivery is synchronous, best effort, without retries, queues, or flushing.
Observers should return promptly and must not panic. Delivery errors and
unwinding panics can't replace a completed database result. A failed delivery
writes one constant sanitized stderr diagnostic directly, without re-entering the
observer or subscriber. The fallback ignores stderr errors. Rust invokes the caller's
panic hook **before** unwind containment: that hook may print a panic payload.
The library doesn't install or replace a global hook and can't promise secrecy
for arbitrary caller hooks or observers. Containment can't recover from process aborts.
Library-emitted observation fields and fallback messages exclude credentials,
connection strings, raw SQL causes and complete book/event payloads. Callers can
inspect retained error causes deliberately and control other dependencies' logs.

A crash between commit and observation, cancellation, filtering or sink failure
can lose diagnostics. These records aren't a durable audit feed or exact activity
counters. The caller owns diagnostic destination, retention, and access.
