# Discord Prediction Market Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development for domain implementation and independent review; the controller integrates storage, transport, and verification. Execute continuously in the existing isolated worktree.

**Goal:** Deliver a standalone Rust Discord prediction bot with an append-only PostgreSQL event store and immutable read projections.

**Architecture:** Pure decisions produce typed events; deterministic replay constructs guild state. PostgreSQL serializes and atomically appends individual event rows and command receipts. Discord and the scheduler invoke that service, while queries read replayed state.

**Tech Stack:** Rust 2024, Tokio, SQLx/PostgreSQL, Serenity, serde, UUID v7, Bazel rules_rs.

**Spec:** `docs/superpowers/specs/2026-09-16-discord-prediction-market-design.md`

## Global Constraints

- Standalone Rust bot; do not reuse Predix storage or domain code.
- 100 points per 24 hours by default, initial grant on enrollment, missed intervals caught up.
- Guild isolation, moderator resolution/cancellation, integer pooled payouts, deterministic largest-remainder allocation, refunds when no winners.
- CloudEvents wire version `1.0`, UUID v7 IDs, immutable historical payloads. No explicit eventindex field.
- Order individual events by server revision. Assign consecutive event revisions after the latest committed event while holding a server transaction lock.
- All source changes immediately followed by `nix develop --command bazel run //:gazelle`. All Rust operations through Bazel. Format all before completion and run repository build.
- Assertions target observable state/output. Real PostgreSQL for storage transaction tests; controlled timestamps, no timing sleeps for domain tests.

## Task 1: Pure domain decisions and replay

**Ownership:** `prediction_bot/lib/src/domain.rs`, `prediction_bot/lib/src/domain_tests.rs` only. Controller owns library root, dependencies, and BUILD configuration.

**Interfaces:** Use serde on all state/event types, integer epoch seconds for timestamps, `u64` Discord IDs, UUID strings for market IDs. Public contract:

```rust
pub struct Policy { pub amount: i64, pub interval: i64 }
pub struct Actor { pub user_id: u64, pub moderator: bool, pub bot: bool }
pub enum Command {
    Join,
    Create { id: String, question: String, options: Vec<String>, closes_at: i64 },
    Bet { id: String, outcome: usize, amount: i64 },
    Resolve { id: String, outcome: usize },
    Cancel { id: String },
    Grant { user_id: u64 },
}
pub struct Decision { pub events: Vec<Event>, pub response: String }
pub fn decide(state: &State, actor: Actor, command: &Command, now: i64, defaults: Policy) -> Result<Decision, DomainError>;
pub fn apply(state: &mut State, event: &Event) -> Result<(), DomainError>;
```

State public fields: `policy: Option<Policy>`, `accounts: BTreeMap<u64, Account>`, `markets: BTreeMap<String, Market>`; derive Default, Clone, Debug, PartialEq, Serialize, Deserialize. Account fields `balance: i64`, `next_grant: i64`. Market fields `creator: u64`, `question: String`, `options: Vec<String>`, `closes_at: i64`, `created_at: i64`, `status: Status`, `bets: Vec<Bet>`. Status is Open, Resolved { outcome: usize, refunded: bool }, Cancelled. Bet fields `user_id: u64`, `outcome: usize`, `amount: i64`. Event vocabulary as spec; events tagged serde with `kind` snake_case. Public `Event::name() -> &'static str` returns CloudEvents suffix e.g. `bet.placed`; `Event::subject() -> String` returns the prescribed subject. Controller generates CloudEvents metadata and validates type/schema envelopes. Pure domain uses checked integer arithmetic and validates replay transitions; mutable temporary reducer state is not published directly.

- [x] Write executable domain tests first with a stub API, demonstrate failing behavior, then implement. For enrollment: decide Join at 1000, apply returned events, assert balance 100 and next_grant 87400. Repeat Join and assert no new grant. For grant: at 173800 assert balance 300 and next_grant 260200; at 87399 no additional grant.
- [x] Test pooled settlement with two winners staking 1 and 2, loser staking 2: payout 2 and 3, total conserved. Test tied remainders by numeric user ID, no-winner refunds, cancellation, invalid outcomes and deadlines, negative stakes, bots, moderator permissions, and overflow without partial published changes.
- [x] Replay events into fresh State, assert identical state. Reject invalid events and replay recorded exact payouts without recomputing distribution.
- [x] Run `aspect test //prediction_bot/lib:domain_test` through Nix/Bazel, record red/green evidence and report changed files. Do not stage others' work.

## Task 2: CloudEvents and PostgreSQL event store

**Ownership:** Controller: library root, `events.rs`, `store.rs`, migrations and real PostgreSQL tests, root Cargo dependency updates and bot BUILD files.

**Interfaces:** `Store::connect(url, application_id, policy)`, `Store::execute(guild_id, key, actor, command)` and deterministic-time variant for tests; `Store::view(guild_id) -> Arc<View>` with state and revision; `Store::guilds()` and `Store::grant_due()`. `migrate(pool)` owns schema setup. `CloudEvent::new(...)`, event validation and replay check metadata, source consistency and identities. UUID v7 generation via existing uuid crate's v7 feature. Command receipt stores immutable response; each CloudEvent is stored as its own event row, with all command writes in one transaction.

- [x] Add dependency features and Bazel-managed lock update support as needed; immediately run Gazelle after source edits.
- [x] Write CloudEvents fixture tests and SQL transaction integration tests before implementation. Duplicate Join/Bets return original response; two concurrent 80-point bets on a 100-point account permit exactly one; two workers credit a due interval once. Replay from a fresh Store yields the same balances. Same market ID in another guild is inaccessible.
- [x] Implement append-only schema; runtime role has SELECT/INSERT only. Use READ COMMITTED transaction, guild advisory lock, replay after lock, receipt lookup, checked revision allocation, atomic multi-row append and immutable projection publication. Rejected commands append nothing. Postcommit projection failure cannot pretend rollback.
- [x] Test unsupported metadata, unknown optional extension round trips, UUID version rejection, gaps, schema incompatibility, permission rejection for UPDATE/DELETE/TRUNCATE, rollback, terminal races and projection rebuild.

## Task 3: Discord transport, scheduler and operation

**Ownership:** Next implementer: `prediction_bot/lib/src/discord.rs`, adapter tests, `prediction_bot/bin/src/main.rs`, README and compose/environment examples. Controller retains BUILD and migration ownership.

**Interfaces:** Consume domain and Store interfaces above. Expose `discord::run(store: Arc<Store>, token: String)`; executable supports migrate mode and normal start. Use startup application info to obtain application ID, validated positive configuration, minimal GUILDS intent, registered guild-only slash group, ephemeral deferred replies with mentions disabled. A worker executes grant_due once per minute and logs failures per command. Handle shutdown and guard one active gateway per application with a PostgreSQL advisory lock.

- [x] Write adapter input tests for DM and bot rejection, moderator flags, option parsing, malformed amounts/outcomes/times. Assert domain requests and public outputs.
- [x] Implement join/balance/leaderboard/create/list/show/bet/resolve/cancel using Serenity. Parse closes_at as RFC3339, options as pipe-separated strings and numeric outcomes as one-based user input.
- [x] Keep errors safe; bound Discord output and disallow mentions. Do not log credentials or raw database errors that might include secrets.
- [x] Document local migration owner and restricted runtime role, persistent PostgreSQL volume, environment defaults, Bazel run/test commands, replay and recovery tradeoffs. Never deploy or contact a real Discord server without supplied credentials.

## Task 4: Review and verification

- [x] Independently review the assembled diff for spec compliance, transaction races, replay correctness, permission enforcement and test quality; address findings and repeat affected tests.
- [x] Run focused domain/CloudEvents/Discord/PostgreSQL tests, `aspect format --scope=all`, and `aspect build //...` through `nix develop --command`. Capture results and distinguish environmental blockers from code failures.
- [x] Commit conventional commits and report delivered behavior, passing checks, and remaining live Discord verification limits.

## Completion evidence

- Domain: 16 tests passed; CloudEvents: 3 tests passed.
- Discord crate: 28 tests passed, including domain/metadata cases and acknowledged-interaction/application-identity regressions.
- PostgreSQL: 10 tests passed against PostgreSQL 16, including concurrency, rollback, immutable runtime privileges, replay corruption and gateway lock release.
- All four bot test targets passed in invocation `9bd106ae-cb59-47cd-b880-746f129799f3`.
- Repository-wide build passed for 358 targets in invocation `374d2c07-5750-4010-82a4-e1484743fb52`.
- Gazelle, repository-wide formatting, and explicit new-file formatting completed. Compose configuration validated.
- Independent domain, storage and assembled integration reviews approved after fixes.
- No live Discord connection was tested because no credentials were supplied.
