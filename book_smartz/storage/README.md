# Book Smartz PostgreSQL storage

This crate persists registered book identities and each reader's authoritative
ranking decisions. It provides database operations, not an API server or account
authentication. Reader UUIDs identify streams; the caller enforces access control.

The caller constructs a PostgreSQL pool, configures its limits, supplies an observer
before the first operation, explicitly migrates, and owns pool shutdown. See
[the compiled usage example](usage/lib.rs) (`//book_smartz/storage:usage`) for the complete
sequence, including acquisition, statement and lock timeouts and an async-scoped
tracing subscriber. Supply the URL through caller configuration; the crate does
not read environment variables. Ordinary operations never migrate. Migration
bookkeeping resides in `book_smartz`, separate from other applications.

Registering a book establishes the immutable book UUID/Open Library work mapping.
It does not rank it. Registration remains committed if a later ranking command
fails. Starting the first book immediately ranks it; later placements may remain
pending for comparisons. Loading replays ordered history, including pending,
paused and skipped-opponent state, without publishing old decisions again.
Loads and commands take work proportional to a reader's history; there are no
snapshots or constant-time replay guarantees.

Commands use caller-supplied event IDs, expected revisions and UTC instants.
Sequence determines order. Concurrent commands for one revision serialize; one
can commit and the other is rejected stale. No automatic retry or implicit
conversion of duplicate/stale rejection into success occurs.

After `StoreError::CommitUncertain`, reconnect to the authoritative database and
load history. Match the original event ID **and intended contents**. Absence in one
read does not prove an in-flight commit cannot finish. Retrying preserves the
original command, event ID, expected revision and instant. Stream locking,
revision checking and uniqueness prevent double application. Identical book
registration can be repeated. Cancellation while awaiting commit has the same
recovery risk but cannot return a completion error or guarantee an observation.

`tracing_observer()` maps typed completion facts into structured records using the
caller's current subscriber and span. Creation, migration and committed commands
are info; reads, idempotent registration and expected rejection are debug;
failures and commit uncertainty are error. Duration is numeric milliseconds;
reader, event and correlation IDs are diagnostic fields. Command kind, revision
and load event count are included when known. No metrics or exporters are installed.
These are completion records, not reconstructed operation spans.

Delivery is synchronous, best effort, without retries, queues or flushing.
Observers should return promptly and must not panic. Delivery errors and
unwinding panics cannot replace a completed database result; a failed delivery
writes one constant sanitized stderr diagnostic directly, without re-entering the
observer or subscriber. Stderr failures are ignored. Rust invokes the caller's
panic hook **before** unwind containment: that hook may print a panic payload.
The library does not install or replace a global hook and cannot promise secrecy
for arbitrary caller hooks or observers. Process aborts cannot be contained.
Library-emitted observation fields and fallback messages exclude credentials,
connection strings, raw SQL causes and complete book/event payloads. Callers can
inspect retained error causes deliberately and control other dependencies' logs.

A crash between commit and observation, cancellation, filtering or sink failure
can lose diagnostics. These records are not a durable audit feed or exact activity
counters. The caller owns diagnostic destination, retention and access.
