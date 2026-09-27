# Personal book ranking: pure domain model

## Intent and scope

Readers build their own ordered bookshelf by comparing a newly added book with
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
command + current projection + explicit time + registered book identity
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
and caller-supplied `occurredAt` UTC instant. Projection revision equals the last
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
the ranking; restart recovery, durable append, snapshots, serialization, and event
schema migrations remain outside this iteration. Changes to event interpretation
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

- Event `occurredAt`: the accepted command's supplied instant, retained in history.
- `startedAt`: derived from `PlacementStarted.occurredAt`.
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

## Authoritative domain events and observability

| Event              | Payload beyond the common envelope                    | Projection effect                                                      |
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
sequence, and occurrence time from the triggering event.

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

| Behavior              | Observable assertion and fault detected                                                                                                            |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| Identity registration | Output/state: identical mapping is idempotent; conflicting mappings and invalid work IDs are rejected                                              |
| Empty ranking         | State/outcomes: first book is inserted immediately with supplied timestamp                                                                         |
| Ordered insertion     | State: top, middle, and bottom placement preserves the existing order and adds exactly one book                                                    |
| Winner decisions      | Output/state: next opponent is useful and final position satisfies all accepted preferences                                                        |
| Skips                 | State: bounds and ranking remain unchanged; skipped opponents do not recur within a round                                                          |
| Pause/resume          | State: unfinished placement is retained, bounds survive, and a new round retries relevant skipped opponents                                        |
| Invalid commands      | Output/state: duplicate addition, second pending addition, stale revision, wrong opponent, and invalid lifecycle transitions leave state unchanged |
| Explicit time         | State/outcomes: supplied instants are retained; equal/backward times do not change ordering behavior                                               |
| Event decisions       | Output: given history and a command, return the expected typed event or error; rejection appends nothing                                           |
| Replay equivalence    | State: full replay equals incremental application for ranking, pending placement, revisions, and timestamps                                        |
| Invalid history       | Output: duplicate/gapped/reordered sequences, mixed readers, invalid pairs, and invalid lifecycle events are rejected                              |
| Derived notices       | Output: completion and automatic pause notices match transitions without becoming authoritative history                                            |
| Replay isolation      | Output/state: replay with no external dependencies preserves original times and produces no telemetry effects                                      |

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
and derived observability notices are concrete design choices for review. Written-spec approval precedes an
implementation plan; production code is not authorized by this document alone.
