# Book Smartz: PostgreSQL persistence

## Agreed outcome

Store registered books and each reader's ranking decisions durably in PostgreSQL,
exposed through Rust operations only. Reconnecting must recover the same ranking,
revision, timestamps, and unfinished placement, including pause state and skipped
opponents. Preserve the existing pure domain and its CloudEvents contract.

The user approved PostgreSQL, database-backed Rust operations without a server,
event history as the source of truth, and the failure, observability, and testing
contracts below. This document is the written specification for review before
implementation planning; conversational approval is not approval of this artifact.

Excluded: an API server, authentication, account profiles, UI, Open Library
fetching, book display metadata, removal or reranking, snapshots, event brokers,
and automatic command retries. Reader UUIDs identify streams; callers own access
control. Book identity remains a UUID associated with one Open Library work ID.

## Inspected baseline and decision

`book_smartz/domain` already supplies identity validation, book registration,
commands, strict replay, projections, and CloudEvents encoding. Every accepted
command produces one event. Caller-supplied IDs and UTC instants remain explicit;
sequence, rather than timestamp, determines order.

The repository declares SQLx with PostgreSQL, Tokio, UUID, migration, and chrono
support. `prediction_bot` already uses SQLx transactions and SQL migrations.
Reuse these dependencies and repository Bazel workflows. Introduce a concrete
PostgreSQL store, not a repository trait solely to permit mocks.

Two storage approaches were considered:

- Persist books and events and replay on each ranking load: selected because it
  directly preserves the existing domain and has one authoritative ranking state.
- Also persist ranking snapshots: deferred because it adds consistency and
  schema-version obligations before there is performance evidence requiring it.

Reads and commands require work proportional to that reader's history. This is
an accepted initial cost, not a claim that replay scales without limit.

## Domain ownership and language

Personal ranking is the model boundary for this slice. Rust packages and the
PostgreSQL schema are implementation boundaries, not separate bounded contexts.
There is no evidence requiring independent Catalog or Identity services. Open
Library supplies an external work identifier; the local `BookId` remains the
identity used in ranking decisions. No external catalog model enters replay.

| Term              | Responsibility and invariant                                                                         |
| ----------------- | ---------------------------------------------------------------------------------------------------- |
| Reader            | Caller-supplied ranking owner identity; not an account or authentication claim                       |
| Book              | Stable local identity associated with exactly one Open Library work                                  |
| Book registration | Enforces the unique mapping in both directions across the catalog                                    |
| Ranking           | Aggregate rooted at `ReaderId`; owns its ordered decisions, revision, entries, and pending placement |
| Pending placement | State inside the ranking aggregate, not an independently writable entity                             |
| Revision          | Position in one reader's decision stream; not a global event order or wall-clock time                |

The `Ranking` aggregate owns the immediate rules: at most one pending placement,
no duplicate ranked book, answers only for the selected active pair, contiguous
revisions, unique event IDs within its stream, and preservation of existing
relative order when inserting a book. `Ranking::execute` and strict replay
already enforce these rules. Storage supplies durable serialization around those
same decisions; it does not reimplement opponent selection or placement policy.

Registration has a simpler consistency requirement: preserve the immutable
one-to-one book/work mapping. A transaction and database uniqueness constraints
are sufficient. The in-memory `BookRegistry` is a validation collaborator, not
one giant catalog aggregate that must be loaded or locked for every reader.
Rankings reference books by identity. Because registration is committed before
use and this slice has no remapping or deletion, commands do not need a
cross-owner workflow to stabilize book identity.

Successful registration does not imply that a book is ranked. If starting its
placement later fails, the registered book remains available. A committed
`PlacementStarted` also does not always mean ranking is complete: only the first
book is placed immediately; later books remain pending until comparisons decide
their position. These distinctions preserve the current business behavior.

## Package and ownership boundaries

- `book_smartz/domain`: retains decisions, invariants, replay, and codecs without
  database or diagnostic delivery dependencies.
- `book_smartz/storage`: asynchronous operations, database mapping, transaction
  orchestration, typed storage errors, structured observations, and a tracing
  listener. Internal modules separate these responsibilities.
- `book_smartz/migrations`: embedded, versioned SQL migrations exposed through an
  explicit Rust migration operation. Ordinary reads and writes never migrate.
- Storage tests: exercise public operations against real PostgreSQL using the
  existing testcontainers and googletest conventions.

The caller constructs and owns a configured PostgreSQL pool and its shutdown.
The store receives a pool handle and an observer at construction. Pool acquisition
and statement/lock time limits belong to caller configuration and must be
illustrated in the usage documentation. The library does not read environment
variables, install a global tracing subscriber, or run a background service.

## Public Rust operations

Exact type names may follow repository conventions; these behaviors are fixed:

| Operation       | Input                                            | Result                                                      |
| --------------- | ------------------------------------------------ | ----------------------------------------------------------- |
| Migrate         | Caller-supplied PostgreSQL pool                  | Apply pending migrations or typed migration failure         |
| Register book   | Existing validated `Book`                        | Created or already present; conflicting mapping is rejected |
| Find book       | `BookId` or `OpenLibraryWorkId`                  | Validated `Book` or absence                                 |
| Load ranking    | `ReaderId`                                       | Replayed `Ranking`, exposing projection and ordered history |
| Execute command | `ReaderId`, `CommandContext`, existing `Command` | Accepted event and resulting projection after commit        |

Loading an unknown reader returns revision zero and an empty ranking without
writing. Histories returned by load let callers inspect an event ID after an
uncertain commit. Returning history supports recovery; it does not add a
historical-ranking product feature. Reads validate stored values and never
silently repair or skip corrupt events.

## Storage contract

Use a dedicated `book_smartz` schema with migration bookkeeping isolated from
other applications. A migration check must establish that applying these
migrations does not interfere with another application's migration history.

- Books: UUID primary key and unique validated work ID. Both mapping directions
  are immutable through this library. Identical registration is idempotent even
  under concurrency; conflicting registration returns the corresponding identity
  error. If both identities conflict, prefer the book-ID conflict consistently.
- Reader streams: one row per reader UUID used for writer coordination. It is
  created lazily inside the first command transaction and contains no account data.
- Ranking events: reader UUID, sequence, event UUID, candidate UUID, optional
  opponent UUID, and the validated CloudEvents document. Enforce uniqueness of
  `(reader, sequence)` and `(reader, event ID)`. Book references use foreign keys.
  Event IDs are scoped by reader, matching CloudEvents source-plus-ID identity.

Store sequence as a checked 20-digit, zero-padded decimal string, preserving the
entire domain `u64` range and lexical ordering without signed-integer truncation.
Reject zero, out-of-range values, or malformed sequence strings. The database
checks indexed identities and sequence against the envelope; the Rust codec and
strict domain replay additionally validate semantics on load.

Persist a validated JSON envelope and preserve valid optional extensions when
handling documents. No raw-envelope import operation is introduced. Validate
before JSON normalization; do not let database JSON handling replace the codec's
input validation. Current operations construct envelopes from domain events.

The library provides append-only event operations. Administrative SQL access is
outside that guarantee; corruption tests deliberately use privileged fixtures.
No mutable ranking-position table is introduced.

## Command transaction and concurrency

1. Begin a transaction using read-committed isolation. Ensure the reader-stream
   row exists, then acquire its row lock. Concurrent first commands must use the
   same coordination path as later commands.
2. Read that reader's events ordered by sequence after obtaining the lock. Decode,
   cross-check stored metadata, and replay through the existing domain.
3. Resolve the registered identities needed by the command into the domain's
   existing registry value. Never load the entire global catalog for a command.
4. Execute through the rehydrated domain aggregate with the supplied context.
   Reuse its expected-revision, transition, and duplicate-event validation. Map
   its typed rejection at the storage boundary without inventing a second rule
   or changing validation precedence. Database uniqueness is defense in depth.
5. Insert the single accepted event and commit. Only then expose success and emit
   a committed observation.

The stream lock spans read, decision, and append. Two valid commands at the same
revision for one reader yield one commit and one stale-revision rejection. Other
readers use different stream rows. A rejected command rolls back all its writes,
including a newly created stream row. Registration uses uniqueness constraints
and explicit conflict resolution to preserve its idempotent contract.

A ranking read obtains its history with one ordered query, so it sees one
consistent statement snapshot. It does not acquire the writer lock or publish
replayed events. Independent book registration is a separate operation; it is
not implicitly combined with ranking commands.

PostgreSQL documents row locks and their transaction lifetime in
[Explicit Locking](https://www.postgresql.org/docs/current/explicit-locking.html).
Concrete SQLx calls must be checked against the repository's resolved dependency;
the version-specific online SQLx documentation was unavailable during design.

## Business events and contract evolution

Commands (`Start`, `Answer`, `Pause`, `Resume`) express reader intent and can be
rejected. The ranking aggregate produces the existing versioned decision events;
the store makes them durable, and strict replay consumes them to reconstruct
state. A proposed event becomes authoritative durable history only when its
transaction commits. Ordering and event-ID uniqueness are per reader, with no
ordering promise between readers.

The existing `BookRanked` and `PlacementAutomaticallyPaused` notices are derived
consequences, not extra authoritative decisions. Do not append them to history,
advance revision for them, or treat replay as new business activity. Diagnostic
operation observations have a separate consumer and best-effort delivery policy.
CloudEvents encoding does not make these private replay records a promised
integration feed. No external business consumer requires publication here, so
an outbox or broker would add obligations without serving the agreed behavior.

Persisted event meaning must remain stable. Future changes to opponent selection,
placement rules, or event schemas must demonstrate that existing histories replay
with their original meaning, or introduce explicit version-aware interpretation
or migration. Do not rewrite old decisions or expire them as telemetry. Add a
fixed historical CloudEvents fixture exercised through database reload to protect
this compatibility boundary, separately from tests using newly encoded events.

The delivery sequence is additive: introduce schema and migration support, add
storage operations around the current domain, then exercise public composition
and document caller adoption. Existing in-memory callers retain their behavior.
No legacy durable store exists in this project; bulk import of caller-held
histories is outside this slice. Written-spec approval still precedes a detailed
implementation plan and execution choice.

## Failure and recovery semantics

Expose typed identity conflict, stale revision, invalid command, duplicate event
ID, corrupt history, migration failure, database failure, and uncertain-commit
outcomes. Preserve underlying causes for deliberate caller inspection, but do
not emit raw cause text through diagnostic listeners or default public summaries.

Before commit, a failed operation returns no success result and publishes no
committed observation. A definite commit rejection is a database failure.
Transport failure while awaiting commit acknowledgment is conservatively an
uncertain outcome; it must never be labeled a confirmed rollback. Cancellation
while awaiting commit has the same recovery risk, even though a canceled future
cannot return an error or promise a completion observation.

After uncertainty, reconnect to the authoritative database and inspect history
for the original event ID and intended event contents. Absence in one read is
not proof that an in-flight commit cannot finish. Any retry retains the original
event ID and expected revision; uniqueness, locking, and revision validation
prevent double application. The library does not automatically retry or silently
turn a duplicate/stale command into success. Book registration can be repeated
with the identical mapping after uncertainty.

## Observability contract

The consumer is the developer operating a future caller, investigating persistence
failures, contention, and successful durable activity. Structured observations are
diagnostic facts, separate from authoritative ranking events.

| Boundary                         | Structured outcome and fields                                                                                    | Tracing listener policy                                                         |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Registration completes           | Created, already present, conflict, failure, or uncertain commit; elapsed duration                               | Info for creation, debug for repeat/conflict, error for failure/uncertainty     |
| Lookup or ranking load completes | Found/absent or loaded; event count for ranking; duration; safe failure category                                 | Debug for successful reads, error for failure/corruption                        |
| Command completes                | Committed, rejected, failed, or uncertain commit; command kind, reader/event IDs, revision where known, duration | Info for committed, debug for expected rejection, error for failure/uncertainty |
| Migration completes              | Applied/current or failed; duration; safe failure category                                                       | Info for success, error for failure                                             |

Use typed outcomes and monotonic elapsed durations, expressed as milliseconds by
the listener. Optional correlation IDs are explicit caller context; reader/event
IDs may occur in diagnostic records, never metric labels. Exclude connection
strings, credentials, raw SQL errors, and full book/event payloads. No metrics or
remote exporters are required for this slice.

An observer is supplied before observed operations. The supplied tracing listener
uses the caller's subscriber and current tracing context. Delivery is synchronous,
best-effort, without queues, retries, durable buffering, or shutdown flushing.
Listener delivery errors cannot replace a database result. Observer implementations
must be non-panicking; containment for unwinding panics should protect a completed
operation where supported, while process-abort behavior cannot be recovered.
Failure of the observation path uses only a minimal sanitized direct diagnostic,
without recursively emitting through that same path. No normal and fallback
record should be emitted for the same successful delivery.

There is an unavoidable commit-to-observation crash gap. These records are not a
durable audit feed or exact activity counters. A canceled operation may lack a
completion record. The caller owns diagnostic destination, access, and retention;
the library introduces no retained telemetry store.

## Verification design

Assume this application owns these tables and other applications do not consume
them directly. Under that boundary PostgreSQL is a managed, out-of-process
dependency. Each integration test receives an isolated database or container;
shared mutable fixtures must not allow tests to affect one another.

Keep deterministic domain collaborators real. Do not introduce a fake database
or verify query counts. Existing domain tests cover ranking decisions; new tests
focus on durability and orchestration through public operations.

| Proposed check                                                                  | Observable assertion and distinct fault protected                                                                        |
| ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| Register and look up by both identities, including concurrent repeats/conflicts | Returned identity and committed state preserve both uniqueness directions                                                |
| Rank, disconnect, construct a fresh store, and load                             | Ranking order, revision, history, and original timestamps survive connection lifecycle                                   |
| Pause/skip and reload, then resume/answer                                       | Bounds, skipped opponents, pause state, and next comparison survive persistence                                          |
| Two readers using the same book and event UUID                                  | Streams remain independent; event uniqueness has the correct scope                                                       |
| Two commands at one revision, including the first command                       | Exactly one succeeds, one is stale, and load reveals one new event                                                       |
| Invalid command and duplicate event ID                                          | No new event or revision; returned errors retain their intended categories                                               |
| Force a database failure before commit                                          | Fresh load reveals no partial accepted operation                                                                         |
| Commit acknowledgment loss                                                      | Outcome is uncertain and recovery cannot create a second event; use a controlled PostgreSQL connection fault if feasible |
| Malformed metadata, sequence gaps, and invalid replay transitions               | Typed corruption error, never a partial ranking presented as valid                                                       |
| Fresh and repeated migration, alongside unrelated migration bookkeeping         | Correct schema and idempotence without affecting another application                                                     |
| Producer plus recording observer                                                | Structured facts match actual results; replay and rollback never claim new committed activity                            |
| Listener mappings and delivery failure                                          | Structured records preserve severity, fields, exclusions, and application result                                         |
| Normal store construction with real listener and database                       | Public operations use the real composition exercised by consumers                                                        |

Database checks use output and state assertions; observer checks use structured
recordings at the declared diagnostic boundary. These integration checks favor
regression protection and refactoring resistance over mocking internal steps.
Existing pure tests supply the faster feedback path. Actual timing and isolation
remain unverified until execution. If a controlled commit fault cannot be tested,
report that specific coverage gap rather than claiming an error-mapping unit test
establishes network-level recovery.

Implementation validation must follow repository instructions: Gazelle immediately
after source changes; full-scope formatting; focused domain and storage tests;
full repository build; lint; and diff checks, using Nix/Bazel/Aspect as appropriate.
No new tests have run as part of this design.

## Review status

The agreed scope and design are recorded. Implementation has not begun. Written
spec approval precedes the implementation plan and selection of its execution
method, as required by the requested brainstorming workflow.
