# Book Ranking Domain Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a pure event-driven personal book ranking library with deterministic replay and CloudEvents JSON support.

**Architecture:** Accepted events are the only source of ranking state. A pure decision function validates commands, an event fold derives current state, and a separate codec translates typed events to CloudEvents JSON. Identity registration is a separate in-memory value.

**Tech Stack:** Rust 2024, existing `uuid`, `chrono`, `serde`, `serde_json`, `thiserror`, and `googletest`; Bazel/Aspect through `nix develop --command`. No new dependencies planned.

**Spec:** [Approved domain design](../specs/2026-09-27-book-ranking-domain-design.md). Read the entire spec before implementing.

## Global Constraints

- `BookId` and `ReaderId` are distinct caller-supplied UUID types; authoritative EventId is also a caller-supplied UUID.
- CloudEvents `specversion` is `"1.0"`; source is `urn:uuid:<ReaderId>`; subject is `books/<BookId>`.
- `sequence` is a positive unsigned 64-bit value encoded as a decimal string without leading zeros.
- One accepted state-changing command appends exactly one authoritative event.
- Current rankings are derived from history, including original timestamps. No clock reads, random ID generation, or external lookups inside the library.
- Preserve existing book order. Additions only; no ties, reranking, removal, persistence, API clients, telemetry listeners, or authentication.
- Reuse root Cargo dependencies; do not run Cargo directly or add per-package Cargo manifests.
- Run Gazelle immediately after each source-edit batch and before formatting or tests. Run full formatting, build, tests, and lint before delivery.
- Preserve the pre-existing untracked `.angular/` directory. Execute implementation in an isolated worktree using the worktree skill.

## Review Focus

- A forged second BookId for an existing work must not bypass registry checks (Task 1 and Task 3).
- Revision arithmetic must reject overflow instead of wrapping; a rejected command leaves history intact (Task 3).
- A repeated EventId with a different sequence must not be accepted as a new event (Task 2).
- Very large sequence values must round-trip exactly; timestamp offsets must preserve instants (Task 4).
- Unknown context extensions must survive JSON round trips without permitting unknown event types or notice types into authoritative replay (Task 4).

## Files and responsibilities

Create the package `book_ranking/domain` with crate name `book_ranking_domain`:

| File             | Responsibility                                                          |
| ---------------- | ----------------------------------------------------------------------- |
| `lib.rs`         | Public re-exports and crate documentation; forbid unsafe code           |
| `identity.rs`    | UUID newtypes, OpenLibraryWorkId, Book, BookRegistry                    |
| `event.rs`       | Sequence, EventMetadata, RankingEvent, EventKind, ComparisonChoice      |
| `projection.rs`  | RankingProjection, pending session, strict replay and event application |
| `command.rs`     | Command types, pure decide, history-owning Ranking aggregate            |
| `notice.rs`      | Pure derived completion/pause notices                                   |
| `cloudevents.rs` | Typed profile validation and JSON codec                                 |
| `error.rs`       | Typed registration, domain, replay, and codec errors                    |
| `BUILD.bazel`    | Gazelle-generated library and colocated unit test target                |
| `README.md`      | Small public API example and scope limitations                          |

Keep tests in each responsible module's `#[cfg(test)]` section, with exhaustive insertion coverage in `command.rs`. Do not create generic repository, clock, emitter, or catalog interfaces.

Generate BUILD rules with Gazelle first, following `hearthstone_simulator/simulator/BUILD.bazel` for a `rust_library` and `rust_test(crate = ":domain")`. Intended test target is `//book_ranking/domain:domain_test`; inspect generated output and adjust only after Gazelle if needed. Ensure all modules belong to the library and GoogleTest is a test-only dependency.

## Shared verification cycle

Every task below uses this sequence; each named test must have executed assertions, not merely a successful filtered command:

1. Write the specified failing tests and run `nix develop --command bazel run //:gazelle` immediately.
2. Run `nix develop --command aspect test //book_ranking/domain:domain_test --test_output=errors`. Confirm the intended failure, distinguishing source failures from tool/environment failures.
3. Implement the specified interfaces and behavior; run Gazelle immediately after source edits.
4. Run `nix develop --command aspect format --scope=all`, then the same full package test target. Confirm passing bodies in the test log.
5. Run `git diff --check`, inspect the scoped diff, and commit only task-owned files using the listed conventional commit message.

## Task 1: Identity registration and package foundation

**Files:** Create `lib.rs`, `identity.rs`, `error.rs`; generate `BUILD.bazel`.

**Interfaces produced:**

- `BookId::new(Uuid) -> BookId`, `ReaderId::new(Uuid) -> ReaderId`, `EventId::new(Uuid) -> EventId`; private fields and read-only `as_uuid(&self) -> &Uuid`.
- `OpenLibraryWorkId::try_from(&str) -> Result<Self, IdentityError>`; canonical `OL` + positive decimal digits without leading zeros + `W`.
- `Book::new(BookId, OpenLibraryWorkId) -> Book`, read-only identity accessors.
- `BookRegistry::new() -> Self`, `register(&mut self, Book) -> Result<(), IdentityError>`, `get(&self, BookId) -> Option<&Book>`.

- [ ] Write `register_book_is_idempotent`: register the same mapping twice and assert lookup returns the same Book.
- [ ] Write `registration_rejects_conflicting_identity`: reject both same work/different UUID and same UUID/different work; assert original lookup is unchanged.
- [ ] Write `work_id_requires_canonical_work_form`: accept `OL45804W`; reject `OL7353617M`, `/works/OL45804W`, empty digits, zero, leading zeros, and non-ASCII digits.
- [ ] Run the failing-test cycle, then implement the types and bidirectional registry. UUID validity comes from `Uuid`, without prescribing v4 versus v7.
- [ ] Complete the shared verification cycle; commit `feat(book-ranking): add domain identities and book registry`.

## Task 2: Authoritative events and strict replay

**Files:** Create `event.rs`, `projection.rs`; extend `error.rs`, `lib.rs`.

**Interfaces produced:**

- `Sequence::new(u64) -> Result<Sequence, DomainError>`; positive only, `value(&self) -> u64`.
- `EventMetadata { id: EventId, reader_id: ReaderId, sequence: Sequence, time: DateTime<Utc> }`.
- `EventKind::{PlacementStarted, ComparisonAnswered { opponent: BookId, choice: ComparisonChoice }, PlacementPaused, PlacementResumed}`.
- `RankingEvent::new(EventMetadata, BookId, EventKind) -> RankingEvent`; immutable public accessors. Constructors do not claim lifecycle validity.
- `ComparisonChoice::{PreferCandidate, PreferOpponent, Skip}`.
- `RankingProjection::replay(ReaderId, &[RankingEvent]) -> Result<Self, ReplayError>` and `apply(&self, &RankingEvent) -> Result<Self, ReplayError>`.
- Projection accessors: `reader_id`, `revision -> u64`, `entries -> &[RankingEntry]`, `pending -> Option<&PlacementSession>`, `next_opponent -> Option<BookId>`.
- Entry accessors `book_id`, `added_at`; session accessors `candidate`, `bounds -> (usize, usize)`, `is_paused`, `started_at`, `last_activity_at`.

The projection privately tracks previously applied EventIds for duplicate rejection during incremental application. Equality includes all replay-relevant state. `ReplayError` reports attempted sequence and a typed reason. Return a fresh state on success; failures expose no partially mutated state.

- [ ] Write `first_start_places_book`: event 1 immediately places A with its event time and clears pending state.
- [ ] Write `comparison_history_derives_order`: Start(A), Start(B), PreferCandidate(B,A) yields `[B,A]`, revision 3, and added times from events 1 and 3.
- [ ] Write `replay_equals_incremental_application`: replay complete and pending histories and compare against successive `apply` calls.
- [ ] Write `invalid_history_is_rejected`: cover missing/repeated/reordered sequences, repeated ID with fresh sequence, wrong reader/candidate/opponent, duplicate book start, second pending start, answers while paused, and invalid resume/pause transitions.
- [ ] Write `timestamps_do_not_order_decisions`: equal and decreasing instants preserve sequence-driven results and original times.
- [ ] Implement the spec's bounds, midpoint tie-break, skip exhaustion, explicit pause/resume, and completion rules in one event application path. Compute midpoint without adding indices in an overflow-prone way. Sequence overflow is a typed failure.
- [ ] Complete the shared verification cycle; commit `feat(book-ranking): derive rankings by replaying decisions`.

## Task 3: Pure commands and aggregate append

**Files:** Create `command.rs`; extend exports/errors.

**Interfaces consumed:** Registry lookup, events, projection replay/apply and accessors from Tasks 1–2.

**Interfaces produced:**

- `Command::{Start { book_id: BookId }, Answer { opponent: BookId, choice: ComparisonChoice }, Pause, Resume}`.
- `CommandContext { expected_revision: u64, event_id: EventId, time: DateTime<Utc> }`.
- `decide(&RankingProjection, &BookRegistry, CommandContext, Command) -> Result<RankingEvent, DomainError>`.
- `Ranking::new(ReaderId) -> Self`, `from_history(ReaderId, Vec<RankingEvent>) -> Result<Self, ReplayError>`.
- `Ranking::execute(&mut self, &BookRegistry, CommandContext, Command) -> Result<RankingEvent, DomainError>`; `history(&self) -> &[RankingEvent]`, `projection(&self) -> &RankingProjection`.

`decide` checks expected revision and registry membership, constructs the candidate event, and validates it through the same pure event application path. `execute` updates history/projection only after validation succeeds. No separately maintained writable ranking order is exposed.

- [ ] Write `rejected_command_keeps_history_and_projection`: unknown book, stale revision, duplicate event UUID, duplicate addition, wrong opponent, and invalid lifecycle operations return typed errors without mutation.
- [ ] Write `skip_changes_opponent_without_implying_preference`: construct `[A,B,C,D,E]` through valid events; X first sees C, skip selects B, choosing X over B selects A.
- [ ] Write `all_useful_opponents_skipped_pauses`: rank A, start B, skip A; B remains pending and unranked. Resume retries A with preserved bounds/start time.
- [ ] Write `explicit_pause_preserves_winning_decisions`: pause and resume mid-placement; bounds survive, skipped set resets, existing ranked times do not change.
- [ ] Write `all_small_insertions_preserve_order`: for list lengths 0 through 8 and every insertion gap, answer according to that intended position and assert final order, uniqueness, exact inserted position, and termination. Build all fixtures through commands/events.
- [ ] Write `revision_increment_rejects_overflow`: exercise checked sequence successor on `u64::MAX`; no wrapping or fabricated event.
- [ ] Implement command/aggregate interfaces and complete the shared verification cycle; commit `feat(book-ranking): add validated placement commands`.

## Task 4: CloudEvents codec and derived notices

**Files:** Create `cloudevents.rs`, `notice.rs`; extend exports/errors.

**Interfaces produced:**

- `DomainNotice::{BookRanked, PlacementAutomaticallyPaused}` with typed common fields from the spec and corresponding payload fields.
- `derive_notices(&RankingProjection, &RankingEvent, &RankingProjection) -> Vec<DomainNotice>`; caller supplies a valid before/event/after transition.
- `CloudEventDocument` encapsulating an authoritative `RankingEvent` and preserved optional context extensions; read-only event accessor.
- `encode_event(&RankingEvent) -> Result<String, CodecError>`.
- `decode_event(&str) -> Result<CloudEventDocument, CodecError>`; `encode_document(&CloudEventDocument) -> Result<String, CodecError>`.
- `encode_notice(&DomainNotice) -> Result<String, CodecError>`.

Use private serde wire structs and validated conversions, not derived deserialization directly into invariant-bearing domain types. Do not require serde features on UUID/chrono: explicitly format/parse their canonical strings. Encode extension attributes at top level and preserve valid unknown scalar context values; reject invalid extension names/values. Codec errors are separate from invalid-history errors.

- [ ] Write `cloudevent_roundtrip_preserves_each_event_kind`: test all four authoritative types, source/subject, UTC time, exact choice strings, and decimal-string sequence.
- [ ] Write `invalid_envelopes_are_rejected`: cover absent/empty required attributes, unsupported spec/type versions, invalid UUID/time/sequence, source/subject disagreement, wrong content type, data as escaped JSON, and conflicting `data_base64`. Reject duplicate JSON object keys that could obscure required values.
- [ ] Write `large_sequence_and_offset_time_roundtrip`: decode sequence `18446744073709551615` without precision loss; reject overflow/zero/leading zeros/numeric encoding. Decode an RFC 3339 offset equivalent to UTC and verify the same instant on output.
- [ ] Write `extensions_survive_roundtrip`: preserve an unknown valid string/bool/32-bit integer extension; reject uppercase names, array/object values, and out-of-range integer extension values. Additional payload fields may be ignored for forward-compatible additive data; missing required payload fields remain errors.
- [ ] Write `derived_notices_are_not_replay_events`: completion and exhausted-skip notices have the spec's type, deterministic suffix ID, causationId and inherited context; authoritative codec rejects them. Explicit pauses do not generate automatic-pause notices.
- [ ] Implement pure encoding/decoding and notice classification. Never publish while replaying. Include the approved JSON example as a parsed-value assertion, not a formatted string snapshot.
- [ ] Complete the shared verification cycle; commit `feat(book-ranking): encode domain events as CloudEvents`.

## Task 5: Public usage and repository verification

**Files:** Create `book_ranking/domain/README.md`; finalize `lib.rs` documentation and tests.

- [ ] Add a public API test that registers A/B, starts and places them, encodes their history, decodes it, rebuilds the ranking, and proves state equality. Assert duplicate/stale requests leave that history unchanged. Run Gazelle after editing Rust.
- [ ] Document caller-supplied identity/time, single active placement, skip/resume, authoritative histories, CloudEvents profile, and the absence of restart durability.
- [ ] Run `nix develop --command aspect format --scope=all`.
- [ ] Run `nix develop --command aspect test //book_ranking/domain:domain_test --test_output=errors` and inspect execution counts.
- [ ] Run `nix develop --command aspect build //...`.
- [ ] Run `nix develop --command aspect test //...`.
- [ ] Run `nix develop --command aspect lint`; resolve relevant failures and distinguish pre-existing/environment blockers explicitly.
- [ ] Run `git diff --check`, inspect the full scoped diff, and commit `docs(book-ranking): document event-driven domain usage` with any final verified source changes.

## Plan review and execution

This plan proposes Rust and the package path above for approval; neither source nor BUILD files have been created. Plan self-review maps all spec requirements to Tasks 1–5, including pure time inputs, derived telemetry facts, strict replay, and JSON interoperability.

Recommended execution: native implementation in this session, followed by an independent whole-branch review. The tasks share a small typed API and benefit from one implementer retaining that context. Alternatively, use subagent-driven execution with task-by-task independent reviews. Await the user's plan review and execution choice before implementation.
