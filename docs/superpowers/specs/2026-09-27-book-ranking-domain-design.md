# Book Smartz: personal book ranking domain

## Intent and scope

The project is named `book_smartz`. Readers build their own ordered bookshelf by comparing a newly added book with
books already ranked. This iteration implements only a pure domain library and
its tests. It supports adding books, winner-or-skip comparisons, and resumable
placement. An ordered stream of accepted domain events is authoritative; the
current ranking and pending placement are projections of those events. The
product exposes the current ranking only, despite retaining decisions for replay.

Excluded: persistence, authentication, website UI, external API clients, telemetry
listeners, historical ranking snapshots, reranking, removal, ties, community
rankings, and automatic catalog reconciliation. Language and package placement
are implementation-plan decisions; the contracts below are language independent.

## Identity and boundaries

- `BookId` is our own UUID, supplied by the caller rather than generated inside
  domain operations.
- `ReaderId` is a distinct UUID type supplied by the caller. It identifies the
  ranking owner. A future application maps authenticated accounts to readers and
  enforces access; possession of a reader UUID is not authentication.
- `OpenLibraryWorkId` is a validated identifier such as `OL45804W`, not a UUID.
- A `Book` associates one `BookId` with one `OpenLibraryWorkId`. Grouping follows
  Open Library works, including editions and translations assigned to that work.
- A supplied book registry must enforce both mapping directions: one local book
  per work ID, and one work ID per local book. Pure registration accepts an
  identical mapping idempotently and rejects conflicting mappings. This prevents
  two caller-created UUIDs from bypassing duplicate checks for the same work.
- Ranking operations accept registered identities. Display metadata is not part
  of ranking identity or necessary for this iteration.

The registry owns identity associations. Each reader's ranking aggregate owns an
append-only in-memory event stream, from which ordered entries and at most one
pending placement are derived. The registry remains a separate pure value; this
iteration does not require event sourcing the catalog. No repository interfaces,
event broker, or storage implementations are introduced.

Open Library's [API documentation](https://openlibrary.org/dev/docs/api/books)
distinguishes works from editions. Its
[editing guidelines](https://openlibrary.org/help/faq/editing) group translations
and abridgements under a work, while adaptations and dramatizations are separate
works. A future catalog integration must explicitly reconcile merges without
silently altering rankings. No merge operation is included here.

## Event-driven domain architecture

The domain separates command decisions from event application:

```text
command + current projection + explicit event UUID/time + registered book identity
    -> decide -> accepted event or typed error
prior event stream + accepted event
    -> replay/apply -> current ranking and pending placement
```

`decide` validates intent without mutating state. Every accepted state-changing
command produces exactly one event. The aggregate validates and appends that
event, then advances its projection by applying it. Rejecting a command appends
nothing. The projection is never an independent source of truth, and callers
cannot directly edit positions, placement bounds, or timestamps.

| Value            | Meaning                                                                                                                |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------- |
| RankingHistory   | Reader ID and ordered accepted events; authoritative input to replay                                                   |
| PersonalRanking  | Derived reader ID, ordered entries, revision, optional pending placement                                               |
| RankingEntry     | Derived Book ID and `addedAt`; position comes from projected list order                                                |
| PlacementSession | Derived candidate, inclusive insertion bounds, skipped opponents for this round, status, `startedAt`, `lastActivityAt` |
| ComparisonChoice | Prefer candidate, prefer opponent, or skip                                                                             |
| DecisionResult   | One proposed event or a typed error                                                                                    |

An empty history projects to an empty ranking at revision zero. Each event carries
its reader ID, contiguous sequence number starting at one, candidate Book ID,
and caller-supplied UTC instant in CloudEvents `time`. Projection revision equals the last
applied sequence. Commands carry the expected revision; stale commands fail.
Reads and replay never append events or advance the sequence.

Replay uses recorded events only: no Open Library lookup, mutable catalog metadata,
clock, random UUID generation, or authentication is consulted. Starting placement
requires a registered book at command validation time; its stable Book ID is
recorded in the event. Later metadata changes do not alter replay.

Replay validates reader identity, sequence continuity, event preconditions, and
pair validity under the rules below. Duplicate, missing, out-of-order, wrong-reader,
or semantically invalid events return a typed invalid-history error identifying
the failing sequence; no partial projection is exposed as valid current state.
This is strict replay, not duplicate-tolerant message consumption. Stale command
retries fail rather than appending the same answer twice.

The full event history is retained in memory. Bounds and skipped sets are derived
working state, not replacements for history. Losing the caller-held history loses
the ranking; restart recovery, durable append, snapshots, and event schema migrations remain
outside this iteration. Pure CloudEvents JSON encoding/decoding is included;
transport bindings and storage are not. Changes to event interpretation
or selection rules must preserve existing replay fixtures; future incompatible
rules require an explicit version/migration design rather than silently replaying
old choices with new semantics.

## Placement rules

The existing list is ordered best to worst and contains `n` books. Insertion
positions are the gaps `0..n`; initial bounds are `lo = 0`, `hi = n`.

1. Starting placement rejects an already ranked book or an existing pending
   placement. On an empty ranking, insert immediately at position zero.
2. An eligible opponent has index `j` with `lo <= j < hi` and has not been
   skipped in this round. It must split the remaining insertion positions.
3. Pick the eligible opponent nearest `(lo + hi - 1) / 2`, breaking ties toward
   the lower index. Selection is deterministic, with no randomness or I/O.
4. Preferring the candidate sets `hi = j`. Preferring the opponent sets
   `lo = j + 1`. Skipping preserves both bounds and marks the opponent skipped.
5. If `lo == hi`, insert at that position, record `addedAt`, and clear placement.
   The relative order and `addedAt` values of existing entries remain unchanged.
6. Otherwise, select the next eligible opponent. If none remain, pause placement
   without modifying the ranked entries.
7. Explicit pause is also available. Resume clears the skipped set for a new
   round but preserves bounds and `startedAt`. Previously skipped opponents can
   reappear if still relevant; winning decisions do not need to be repeated.

Only the currently selected pair can be answered. Reject an answer for another
opponent, an absent or paused session, or a stale revision. Pair selection itself
is a read and never changes time or revision. Invalid commands leave state
unchanged and append no events.

Some skipped opponents may eventually be necessary for exact placement. Never
invent a preference or arbitrarily finalize an unresolved position. Placement
has no automatic expiry. Pending books are not ranked entries.

Example: for `A > B > C > D > E`, adding X first presents C. Skipping C presents
B (the tie-break chooses B over D). Preferring X over B narrows placement to
before B, so A is next. All existing relative ordering is preserved.

## Time semantics

The caller supplies a UTC instant to each state-changing operation. UUID
generation and reading the current clock are outside the domain boundary.

- CloudEvents `time`: the accepted command's supplied instant, retained in history.
  It is required by our domain profile, although optional in CloudEvents itself.
- `startedAt`: derived from the `PlacementStarted` event's `time`.
- `lastActivityAt`: derived from the latest event affecting the pending placement.
- `addedAt`: derived from the event that makes placement complete: the first-book
  start event or the decisive comparison event.

Replay preserves these original values; it never stamps events or projections
with the time at which rebuilding happens.

These are recorded instants, not local calendar commitments. No time zone, local
date conversion, scheduling, or publication-date model is necessary. Equal or
backward-moving timestamps are accepted: revisions establish logical order, and
timestamps must not influence ranking or opponent selection. No duration is
computed by subtracting wall-clock values.

## CloudEvents contract

Use the [CloudEvents 1.0.2 specification](https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/spec.md)
and its [JSON event format](https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/formats/json-format.md).
The envelope's `specversion` is `"1.0"`, not the specification document's patch
version. CloudEvents defines the envelope; our profile below defines ranking
payloads and replay constraints. It supplies no persistence or ordering guarantee.

| Attribute         | Domain profile                                                                                               |
| ----------------- | ------------------------------------------------------------------------------------------------------------ |
| `specversion`     | Required string `"1.0"`                                                                                      |
| `id`              | Required caller-supplied EventId UUID, encoded as a canonical UUID string; distinct from BookId and ReaderId |
| `source`          | Required absolute URI `urn:uuid:<ReaderId>`; identifies this reader's single ranking stream                  |
| `type`            | Required versioned name from the mapping below                                                               |
| `subject`         | Required by this profile: `books/<BookId>` for the candidate                                                 |
| `time`            | Required by this profile: original occurrence instant; encode UTC as RFC 3339 with `Z`                       |
| `datacontenttype` | Required by this profile: `application/json`                                                                 |
| `data`            | Required JSON object containing common ranking fields and event-specific fields                              |

CloudEvents requires `id`, `source`, `specversion`, and `type`. Our additional
requirements are deliberately stricter. No custom context extensions or
`dataschema` URI are necessary initially. Incompatible domain payload changes
need a new type version, independently of `specversion`.

| Domain event       | CloudEvents `type`                   |
| ------------------ | ------------------------------------ |
| PlacementStarted   | `bookranking.placement.started.v1`   |
| ComparisonAnswered | `bookranking.comparison.answered.v1` |
| PlacementPaused    | `bookranking.placement.paused.v1`    |
| PlacementResumed   | `bookranking.placement.resumed.v1`   |

All `data` objects carry `readerId`, `candidateBookId`, and `sequence`. UUID fields
use canonical UUID strings. `sequence` is a positive unsigned 64-bit value encoded
as a decimal string without leading zeros, avoiding JSON numeric precision loss.
It stays in domain data, not a CloudEvents integer extension. Comparison data
also contains `opponentBookId` and `choice`, whose values are `prefer_candidate`,
`prefer_opponent`, or `skip`. Lifecycle events need no additional data fields.

The source must match `data.readerId`, and subject must match
`data.candidateBookId`. Event identity is `(source, id)`; sequence controls order.
Neither UUID sort order nor `time` controls replay. The caller supplies a fresh
EventId for each newly accepted occurrence; reuse of an ID in a stream is rejected,
even with a new sequence. Transport retransmission would retain the original
identity, but strict domain replay still rejects duplicate entries.

Example of a comparison event: event 1 started and immediately placed A; event 2
started B; event 3 records the preference for B over A:

```json
{
  "specversion": "1.0",
  "id": "e83ecc4f-68bf-4b24-9a63-60cc7cfad460",
  "source": "urn:uuid:b3692ae0-c782-4b0e-9335-4b2bdc7048d5",
  "type": "bookranking.comparison.answered.v1",
  "subject": "books/4b3d5226-5df7-4c1d-935e-bc42a3434868",
  "time": "2026-09-27T12:00:00Z",
  "datacontenttype": "application/json",
  "data": {
    "readerId": "b3692ae0-c782-4b0e-9335-4b2bdc7048d5",
    "candidateBookId": "4b3d5226-5df7-4c1d-935e-bc42a3434868",
    "sequence": "3",
    "opponentBookId": "286e3364-b260-4f4f-8ef4-8074d3eb52ba",
    "choice": "prefer_candidate"
  }
}
```

The whole structured JSON event uses media type `application/cloudevents+json`;
`datacontenttype` describes only `data`. Encode data as an object, not escaped JSON
text or `data_base64`. Use typed payload variants within the domain rather than
unrestricted JSON maps. A pure codec validates the envelope and payload before
replay; unsupported specification/type versions, missing profile fields, invalid
UUIDs/timestamps/sequences, and mismatched source/subject are typed errors.
Unknown optional context extensions do not affect domain decisions and must not
be mistaken for unsupported domain event types. Preserve them when round-tripping.

JSON round trips preserve event identity, instant, and payload semantics; byte
ordering and timestamp spelling are not domain contracts. This format contract
does not mandate an SDK, HTTP server, message broker, exporter, or database.

## Authoritative domain events and observability

| Event              | Payload beyond the common data fields                 | Projection effect                                                      |
| ------------------ | ----------------------------------------------------- | ---------------------------------------------------------------------- |
| PlacementStarted   | None                                                  | Initialize bounds/session; immediately insert if ranking is empty      |
| ComparisonAnswered | Opponent Book ID and choice                           | Narrow bounds or mark skip, then derive completion or automatic pause  |
| PlacementPaused    | None; this event represents an explicit pause request | Mark active session paused                                             |
| PlacementResumed   | None                                                  | Reactivate paused session and clear round skips while retaining bounds |

A comparison records the actual pair and choice, including skips. Replay verifies
that this was the selected pair and applies the specified insertion rules.
Completed position is derived from the decisions; a separately recorded
`BookRanked` event or stored final order is not needed to determine it. Likewise,
exhausting useful opponents derives an automatic pause from the skip event.
Explicit pause/resume events are necessary because those reader actions cannot
be inferred from preference decisions.

For example, `Start(A)`, `Start(B)`, `Answer(B, A, prefer candidate)` projects to
`B > A`. Replaying the same history produces the same ranking, revision, and
timestamps. A skip records no preference but does affect which pair comes next.

Domain events are authoritative behavioral inputs, not diagnostic logs. To support
future observability, a pure transition classifier can derive `BookRanked`
(position and entry count) and `PlacementAutomaticallyPaused` (reason: no eligible
opponents) notices from the old projection, applied event, and new projection.
These notices are outputs only and are never appended to history or consumed to
reconstruct state. Accepted domain events already describe starts, choices,
explicit pauses, and resumes. Notices inherit reader ID, candidate Book ID,
sequence, and occurrence time from the triggering event. If exposed as events,
these notices also use CloudEvents: types `bookranking.book.ranked.v1` and
`bookranking.placement.automaticallypaused.v1`, the same source/subject/time, and
IDs formed as `<trigger-event-id>:bookranked` or
`<trigger-event-id>:automaticallypaused`. These deterministic string IDs are
CloudEvents-valid and cannot collide with the authoritative UUID IDs. Their data
includes the common fields, `causationId` referencing the triggering event, and
notice-specific fields. They are not accepted by the authoritative replay codec;
one accepted command still appends exactly one domain event.

Replay must have no telemetry side effects and must not count historical actions
as new activity. A future application may publish accepted events and derived
notices only for newly committed commands. Commit timing, delivery guarantees,
listener registration, metrics, traces, retries, retention, and shutdown remain
out of scope. Merely returning or appending an in-memory event claims no durable
commit. Typed command errors are separate from accepted history.

This provides structured facts for operators to distinguish starts, skips,
pauses, and completions. Names, email addresses, titles, and raw request content
are absent. IDs can support correlation but must not become metric labels.

## Verification design

Tests exercise cohesive domain behavior with real private in-process values.
There are no shared or out-of-process dependencies, no mocks, and no clock or
repository interface. Fixed UUIDs and explicit instants are ordinary inputs.

| Behavior              | Observable assertion and fault detected                                                                                                                                                 |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Identity registration | Output/state: identical mapping is idempotent; conflicting mappings and invalid work IDs are rejected                                                                                   |
| Empty ranking         | State/outcomes: first book is inserted immediately with supplied timestamp                                                                                                              |
| Ordered insertion     | State: top, middle, and bottom placement preserves the existing order and adds exactly one book                                                                                         |
| Winner decisions      | Output/state: next opponent is useful and final position satisfies all accepted preferences                                                                                             |
| Skips                 | State: bounds and ranking remain unchanged; skipped opponents do not recur within a round                                                                                               |
| Pause/resume          | State: unfinished placement is retained, bounds survive, and a new round retries relevant skipped opponents                                                                             |
| Invalid commands      | Output/state: duplicate addition, second pending addition, stale revision, wrong opponent, and invalid lifecycle transitions leave state unchanged                                      |
| Explicit time         | State/outcomes: supplied instants are retained; equal/backward times do not change ordering behavior                                                                                    |
| Event decisions       | Output: given history and a command, return the expected typed event or error; rejection appends nothing                                                                                |
| Replay equivalence    | State: full replay equals incremental application for ranking, pending placement, revisions, and timestamps                                                                             |
| Invalid history       | Output: duplicate/gapped/reordered sequences, mixed readers, invalid pairs, and invalid lifecycle events are rejected                                                                   |
| Derived notices       | Output: completion and automatic pause notices match transitions without becoming authoritative history                                                                                 |
| CloudEvents contract  | Output: JSON round-trip preserves typed event semantics; required fields, source/subject agreement, event-ID uniqueness, supported versions, and precise sequence encoding are enforced |
| Replay isolation      | Output/state: replay with no external dependencies preserves original times and produces no telemetry effects                                                                           |

Use exhaustive small-list insertion scenarios to protect boundary arithmetic
without coupling tests to helper methods. Test deterministic midpoint tie-breaks
because they are part of the chosen selection contract. Assert returned values
and visible state, never logging text or internal collaborator call counts.

Construct ranking fixtures through events rather than manually populated final
orders. Include replay of partially completed sessions, skip exhaustion, explicit
pause/resume, and completed insertion. Replaying the same history twice must
produce equal independent projections, without appending duplicate activity.

These checks target ranking correctness and refactoring resistance without
network/database fixtures. Feedback speed is expected to be fast but has not
been measured. No implementation, regression tests, provider checks, or delivery
checks have been executed as part of this design.

## Review status

The conversational pure-domain scope is approved. At the user's request, this
revision makes accepted ranking events authoritative and derives current state
through replay. It replaces the prior mutable-state-plus-outcomes design while
retaining the no-persistence scope. Event envelopes, strict replay validation,
and derived observability notices are concrete design choices for review.
At the user's request, authoritative events and exposed notices now use CloudEvents;
pure JSON format support is included without adding persistence or transport. Written-spec approval precedes an
implementation plan; production code is not authorized by this document alone.
