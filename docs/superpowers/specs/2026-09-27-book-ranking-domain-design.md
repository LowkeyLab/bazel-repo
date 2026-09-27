# Personal book ranking: pure domain model

## Intent and scope

Readers build their own ordered bookshelf by comparing a newly added book with
books already ranked. This iteration implements only a pure domain library and
its tests. It supports adding books, winner-or-skip comparisons, and resumable
placement. It exposes the current ranking only.

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

The registry owns identity associations; each reader's ranking aggregate owns
ordered entries and at most one pending placement. Both are in-memory domain
values. No repository interfaces or storage implementations are introduced.

Open Library's [API documentation](https://openlibrary.org/dev/docs/api/books)
distinguishes works from editions. Its
[editing guidelines](https://openlibrary.org/help/faq/editing) group translations
and abridgements under a work, while adaptations and dramatizations are separate
works. A future catalog integration must explicitly reconcile merges without
silently altering rankings. No merge operation is included here.

## Domain state

| Value            | Meaning                                                                                                                              |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| PersonalRanking  | Reader ID, ordered entries, state revision, optional pending placement                                                               |
| RankingEntry     | Book ID and `addedAt` UTC instant; position is derived from list order                                                               |
| PlacementSession | Candidate Book ID, inclusive insertion bounds, skipped opponents for this round, active/paused status, `startedAt`, `lastActivityAt` |
| ComparisonChoice | Prefer candidate, prefer opponent, or skip                                                                                           |
| Transition       | Next state and ordered structured outcomes, or a typed error                                                                         |

Successful state-changing commands advance the aggregate revision once. Commands
carry the expected revision; stale commands fail without changing state. Reads
do not advance it. This detects stale responses within the domain contract; it
does not provide database concurrency control.

No unbounded decision history is retained. Bounds preserve the consequences of
winner decisions; the skipped set tracks only the current round. Domain state
survives a pause when the caller retains it. Surviving process restart is outside
this iteration because persistence is excluded.

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
unchanged and return no successful outcomes.

Some skipped opponents may eventually be necessary for exact placement. Never
invent a preference or arbitrarily finalize an unresolved position. Placement
has no automatic expiry. Pending books are not ranked entries.

Example: for `A > B > C > D > E`, adding X first presents C. Skipping C presents
B (the tie-break chooses B over D). Preferring X over B narrows placement to
before B, so A is next. All existing relative ordering is preserved.

## Time semantics

The caller supplies a UTC instant to each state-changing operation. UUID
generation and reading the current clock are outside the domain boundary.

- `startedAt`: supplied instant when placement begins.
- `lastActivityAt`: supplied instant for the latest accepted placement command.
- `addedAt`: supplied instant when a book is successfully placed.
- Outcome `occurredAt`: the accepted command's supplied instant.

These are recorded instants, not local calendar commitments. No time zone, local
date conversion, scheduling, or publication-date model is necessary. Equal or
backward-moving timestamps are accepted: revisions establish logical order, and
timestamps must not influence ranking or opponent selection. No duration is
computed by subtracting wall-clock values.

## Observable outcomes

Domain transitions return immutable structured facts. They do not invoke
listeners or claim that state was persisted. A future application can publish
these facts after committing the corresponding state.

Every outcome carries reader ID, candidate Book ID, resulting revision, and
`occurredAt`. Concrete outcomes are:

| Outcome            | Additional fields and emission boundary                                         |
| ------------------ | ------------------------------------------------------------------------------- |
| PlacementStarted   | Emitted when addition starts, including immediate first-book placement          |
| ComparisonAnswered | Opponent Book ID and choice; emitted for each accepted answer, including skip   |
| PlacementPaused    | Reason: requested or no eligible opponents                                      |
| PlacementResumed   | Emitted on explicit resume                                                      |
| BookRanked         | Zero-based position and resulting entry count; emitted when insertion completes |

A completing answer returns `ComparisonAnswered` followed by `BookRanked`.
An exhausting skip returns `ComparisonAnswered` followed by `PlacementPaused`.
First-book addition returns `PlacementStarted` followed by `BookRanked`.

These contracts let a future operator distinguish starting, skipping, pausing,
and completing placement, rather than observing only exceptions. Typed errors
describe rejected operations separately. Names, email addresses, titles, and raw
request content are absent. IDs may support correlation but must not become
metric labels. Listener registration, metrics, traces, retries, delivery,
retention, and shutdown belong to a later application iteration.

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
| Structured outcomes   | Output: exact fact types, typed fields, and ordering match successful transitions; rejected commands return errors without success facts           |

Use exhaustive small-list insertion scenarios to protect boundary arithmetic
without coupling tests to helper methods. Test deterministic midpoint tie-breaks
because they are part of the chosen selection contract. Assert returned values
and visible state, never logging text or internal collaborator call counts.

These checks target ranking correctness and refactoring resistance without
network/database fixtures. Feedback speed is expected to be fast but has not
been measured. No implementation, regression tests, provider checks, or delivery
checks have been executed as part of this design.

## Review status

The conversational design and pure-domain scope are approved. This consolidated
spec adds precise boundary arithmetic, identity registration, revision handling,
and timestamp behavior for review. Written-spec approval precedes an
implementation plan; production code is not authorized by this document alone.
