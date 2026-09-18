# Prediction Bot Announcements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Announce market creation, resolution, and cancellation in an administrator-selected server channel with durable retries.

**Architecture:** Save announcement snapshots in a PostgreSQL outbox within the existing market transaction. A separately supervised worker sends through the real Serenity HTTP client, preserving order within each server and allowing progress across servers. Configuration, rendering, persistence, and delivery have focused modules; PostgreSQL and Wiremock exercise their actual composition.

**Tech Stack:** Rust, Tokio, Serenity, SQLx/PostgreSQL, serde, Wiremock, testcontainers, Bazel/Aspect.

**Spec:** `docs/superpowers/specs/2026-09-18-prediction-bot-announcements-design.md` (approved).

## Global Constraints

- Announce only market creation, resolution, and cancellation. No backfill, per-bet messages, subscriptions, external queue, or public betting controls.
- Configuration requires Administrator or Manage Guild; commands are server-only and private.
- One ordinary server text-channel destination per guild; no threads or special channel types initially.
- Pending messages move when the destination changes; disable discards them. Reenable never revives discarded work.
- An in-flight request may reach its former destination. A stale failure must not pause a new destination.
- Retry policy: 5 seconds after the first failure, doubling to a maximum of 5 minutes, without a maximum attempt count. Respect longer provider delays.
- Exactly-once Discord delivery is not promised; repeated command delivery must not duplicate enqueueing.
- Event and command tables remain append-only for the runtime role. No source event schema changes.
- Use existing dependencies. Keep the real Store and Serenity client in integration tests. No mock repository or transport trait per collaborator.
- Use Bazel/Aspect for build, generation, formatting, and tests. Prefix commands with `nix develop --command` in this worktree.
- Immediately after each source-file edit, run `nix develop --command bazel run //:gazelle`, before formatting or another source edit. Run Gazelle before manually editing BUILD files, which have ignored sections requiring deliberate updates.
- Before each commit run `nix develop --command aspect format --scope=all` and the focused tests. Before completion run `nix develop --command aspect build //...`.
- This worktree already exists and is detached. Preserve user changes; use it directly. Do not push or open a PR as part of plan execution.

## File and interface map

New production files:

- `prediction_bot/migrations/003_announcements.sql`: settings/outbox schema and runtime privileges.
- `prediction_bot/lib/src/announcements.rs`: module exports, domain snapshot and settings types, command interface.
- `prediction_bot/lib/src/announcements/render.rs`: deterministic message construction and content limits.
- `prediction_bot/lib/src/announcements/persistence.rs`: SQL transactions and conditional delivery updates.
- `prediction_bot/lib/src/announcements/worker.rs`: due-work orchestration, retry decisions, and worker lifecycle.
- `prediction_bot/lib/src/discord/announcements.rs`: Discord destination validation and configuration responses.

New tests:

- `prediction_bot/lib/src/announcements/tests.rs`: renderer/retry and real-HTTP tests included through the existing library test crate.
- `prediction_bot/lib/tests/announcements_test.rs`: PostgreSQL plus HTTP integration tests.
- `prediction_bot/lib/tests/support/announcements.rs`: private container/runtime-login and HTTP fixtures for that integration test.

Modify `lib.rs`, `store.rs`, `discord.rs`, `discord_tests.rs`, `lib/BUILD.bazel`, `migrations/BUILD.bazel`, existing migration assertions in `lib/tests/store_test.rs`, and `README.md`. Add focused lifecycle/error reporting through existing audit conventions only where the worker needs operational visibility; retain existing audit behavior.

### Shared contract

Use the following names consistently. Types live in `announcements.rs`; keep SQL helpers crate-private. Cross-module fields below are public within the crate unless an integration test uses the application-facing type.

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum SnapshotV1 {
    Created { id: String, question: String, creator: u64,
        options: Vec<String>, closes_at: i64, occurred_at: i64 },
    Resolved { id: String, question: String, winner: String,
        refunded: bool, occurred_at: i64 },
    Cancelled { id: String, question: String, occurred_at: i64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigurationChange { Set { channel_id: u64 }, Disable }
#[derive(Clone, Debug)]
pub struct AnnouncementStatus {
    pub channel_id: Option<u64>, pub enabled: bool,
    pub version: i64, pub pause_reason: Option<String>, pub pending: i64,
}
#[derive(Clone, Debug)]
pub(crate) struct PendingAnnouncement {
    pub guild: u64, pub revision: i64, pub channel_id: u64,
    pub configuration_version: i64, pub attempts: i64,
    pub snapshot: SnapshotV1,
}
pub type Clock = std::sync::Arc<dyn Fn() -> i64 + Send + Sync>;
```

Application-facing methods, implemented on the existing `Store`:

```rust
pub async fn configure_announcements(
    &self, guild: u64, key: &str, actor: crate::domain::Actor,
    change: ConfigurationChange,
) -> Result<String, crate::store::StoreError>;
pub async fn announcement_status(
    &self, guild: u64, actor: crate::domain::Actor,
) -> Result<AnnouncementStatus, crate::store::StoreError>;
```

Persistence helpers in `announcements::persistence`:

```rust
pub(crate) async fn enqueue(
    tx: &mut sqlx::PgConnection, guild: u64, revision: i64,
    event: &crate::domain::Event, state: &crate::domain::State,
) -> Result<(), crate::store::StoreError>;
pub(crate) async fn next_due(
    store: &crate::store::Store, now: i64, limit: i64,
) -> Result<Vec<PendingAnnouncement>, crate::store::StoreError>;
pub(crate) async fn still_eligible(
    store: &crate::store::Store, item: &PendingAnnouncement,
) -> Result<bool, crate::store::StoreError>;
```

Worker APIs:

```rust
pub async fn deliver_due(
    store: &crate::store::Store, http: &serenity::http::Http, clock: &Clock,
) -> Result<(), crate::store::StoreError>;
pub fn start_announcement_worker(
    store: std::sync::Arc<crate::store::Store>,
    http: std::sync::Arc<serenity::http::Http>, clock: Clock,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()>;
```

Only one scheduler may call `deliver_due` for an application at once; gateway startup owns that invariant under its existing guard. Integration tests invoke it sequentially unless exercising different isolated application fixtures.

## Task 1: Durable settings and administrative changes

**Files:** create migration, `announcements.rs`, `announcements/persistence.rs`, integration fixture and test files; modify `lib.rs`, `store.rs`, migration assertions and both BUILD files.

**Consumes:** existing `Store`, `Actor`, `StoreError`, SQLx migration mechanism, guild advisory lock, and `prediction_commands` receipts.

**Produces:** shared types, `Store::configure_announcements`, `Store::announcement_status`, migrated tables usable with the runtime login.

- [ ] Add `pub mod announcements;` and the shared types, then register the new integration target `//prediction_bot/lib:announcements_test` using `store_test`'s image data, execution properties and tags, plus the existing Wiremock dependency. Include support sources in this target. Run Gazelle immediately after every Rust edit and before BUILD edits.
- [ ] Create `fixture()` in `tests/support/announcements.rs` returning `(ContainerAsync<Postgres>, Arc<Store>, PgPool)`: retain the container, migrate through an owner pool, connect `Store` using the actual runtime login, and return the owner pool for fixture-only fault injection. Follow the current test-images helper. Define `admin()` returning `Actor { user_id: 7, moderator: true, bot: false }`.
- [ ] Add this failing integration test; also check unauthorized users, empty initial status, and repeated set/disable command receipts:

```rust
#[tokio::test]
async fn old_configuration_receipt_cannot_restore_a_disabled_channel() {
    let (_container, store, _owner) = fixture().await;
    let change = ConfigurationChange::Set { channel_id: 20 };
    let receipt = store.configure_announcements(10, "discord:101", admin(), change)
        .await.unwrap();
    store.configure_announcements(10, "discord:102", admin(), ConfigurationChange::Disable)
        .await.unwrap();
    assert_eq!(store.configure_announcements(10, "discord:101", admin(), change)
        .await.unwrap(), receipt);
    let status = store.announcement_status(10, admin()).await.unwrap();
    assert!(!status.enabled);
    assert_eq!(status.pending, 0);
}
```

- [ ] Run `nix develop --command aspect test //prediction_bot/lib:announcements_test --test_filter=old_configuration_receipt_cannot_restore_a_disabled_channel`; confirm it fails at the missing schema/behavior boundary. Resolve build setup errors separately.
- [ ] Implement migration 003 with the following schema, retaining existing event-table grants:

```sql
CREATE TABLE prediction_announcement_settings (
  guild_id TEXT PRIMARY KEY CHECK (guild_id ~ '^[1-9][0-9]{0,19}$'),
  channel_id TEXT CHECK (channel_id ~ '^[1-9][0-9]{0,19}$'),
  enabled BOOLEAN NOT NULL DEFAULT FALSE,
  configuration_version BIGINT NOT NULL DEFAULT 0 CHECK (configuration_version >= 0),
  pause_reason TEXT,
  CHECK (NOT enabled OR channel_id IS NOT NULL)
);
CREATE TABLE prediction_announcement_outbox (
  guild_id TEXT NOT NULL,
  revision BIGINT NOT NULL,
  snapshot_version INTEGER NOT NULL CHECK (snapshot_version = 1),
  snapshot JSONB NOT NULL CHECK (jsonb_typeof(snapshot) = 'object'),
  state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'delivered', 'discarded')),
  attempts BIGINT NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  next_attempt_at BIGINT NOT NULL,
  last_failure TEXT,
  delivered_channel_id TEXT,
  delivered_message_id TEXT,
  PRIMARY KEY (guild_id, revision),
  FOREIGN KEY (guild_id, revision) REFERENCES prediction_events(guild_id, revision),
  CHECK (state <> 'delivered' OR
    (delivered_channel_id IS NOT NULL AND delivered_message_id IS NOT NULL))
);
CREATE INDEX prediction_announcement_due
  ON prediction_announcement_outbox(next_attempt_at, guild_id, revision)
  WHERE state = 'pending';
REVOKE ALL ON prediction_announcement_settings, prediction_announcement_outbox FROM PUBLIC;
GRANT SELECT, INSERT, UPDATE ON prediction_announcement_settings,
  prediction_announcement_outbox TO prediction_bot_runtime;
GRANT SELECT ON _sqlx_migrations TO prediction_bot_runtime;
```

- [ ] Embed migration 003 in `migrate()` and `compile_data`, export the SQL file, and require its successful `_sqlx_migrations` row during `connect_with_audit()`. The migration table exists before SQLx runs migration 003. Preserve the economy schema marker and event immutability guard.
- [ ] In `configure_announcements`, validate guild/key/actor, begin a transaction, acquire the existing guild advisory lock, and check the receipt before changing settings. Require the same actor for receipt replay. Increment the configuration version with checked arithmetic. Set clears pause and resets pending deadlines/attempt counts; disable marks pending rows discarded. Insert a receipt with the current event revision (zero is valid) and no economy events. Use a whole-second UTC timestamp acquired after the lock and bound into the receipt insert. Commit settings, outbox updates, and receipt together. `announcement_status` enforces the same administrator policy and returns defaults for an unconfigured guild.
- [ ] Verify migration idempotence and startup without migration 003. Update the old expected SQLx version list from `[1, 2]` to `[1, 2, 3]`. Assert the runtime login cannot UPDATE/DELETE/TRUNCATE existing receipts/events and can perform the new operations. Run both integration targets, format, and commit as `feat(prediction-bot): persist announcement channel settings`.

## Task 2: Enqueue snapshots atomically with market events

**Files:** modify `announcements/persistence.rs`, `store.rs`, `tests/announcements_test.rs`.

**Consumes:** settings/outbox schema, `SnapshotV1`, existing event application and transaction receipts.

**Produces:** `enqueue` and durable snapshots for exactly the supported events.

- [ ] Add tests using `store.execute_at` for join/create, then resolve after close and cancellation of another market. Use fixed times 1000 and 2000. Query outbox snapshots through SQL only for these persistence invariants. A repeated command must leave exactly one row at its original event revision.
- [ ] Inject this fixture-local failure through the owner connection and assert no market or receipt is committed:

```rust
sqlx::query("ALTER TABLE prediction_announcement_outbox ADD CONSTRAINT reject_fixture_guild CHECK (guild_id <> '10')")
    .execute(&owner).await.unwrap();
let result = store.execute_at(10, "discord:202", admin(),
    &Command::Create { id: "fixture-market".into(), question: "Will it rain?".into(),
        options: vec!["Yes".into(), "No".into()], closes_at: 2000 }, 1000).await;
assert!(result.is_err());
assert!(!store.view(10).await.unwrap().state.markets.contains_key("fixture-market"));
```

Enroll the actor and configure the destination before installing the constraint. In a separate case reject the command receipt insert after enqueueing and assert the outbox is empty as well.

- [ ] Run the new tests and observe the missing enqueue/atomicity behavior before implementation.
- [ ] Invoke `enqueue(&mut tx, guild, view.revision, event, &view.state)` after each event has been applied and inserted, before the receipt and commit. It reads enabled settings within that transaction, maps only the three event variants to snapshots, and uses the applied state for resolution/cancellation question text and winning option. Paused settings still enqueue. Store the event's original occurrence time as the initial deadline. Do not invoke enqueue from replay or after commit.
- [ ] Verify joins, grants and bets never enqueue; disabled periods and historical replay never backfill; reconnecting a store preserves pending work; concurrent disable/create is serialized by the guild lock. Run the two storage integration targets, format, and commit as `feat(prediction-bot): enqueue market announcements atomically`.

## Task 3: Deterministic rendering and retry decisions

**Files:** create `announcements/render.rs`, `announcements/worker.rs`, `announcements/tests.rs`; update module declarations.

**Consumes:** `SnapshotV1`.

**Produces:** `render(&SnapshotV1) -> CreateMessage`, `retry_at(now: i64, failures: i64, provider_delay: Option<i64>) -> i64`, and a delivery-failure classification used by Task 4.

- [ ] Add output tests for the three message types, resolution refunds, original event timestamps, Unicode and hostile mention/Markdown text. Serialize `CreateMessage` and check required semantic fields and empty allowed-mention parsing; avoid full Serenity-object snapshots.
- [ ] Add explicit retry-policy examples:

```rust
#[test]
fn retries_start_after_failure_and_cap_the_local_delay() {
    assert_eq!(retry_at(1000, 1, None), 1005);
    assert_eq!(retry_at(1000, 2, None), 1010);
    assert_eq!(retry_at(1000, 100, None), 1300);
    assert_eq!(retry_at(1000, 1, Some(600)), 1600);
    assert_eq!(retry_at(i64::MAX - 1, 1, None), i64::MAX);
}
```

- [ ] Run `nix develop --command aspect test //prediction_bot/lib:discord_test --test_filter=announcements`; observe failure before implementing the outputs.
- [ ] Render one plain message using event heading, question, ID, original event timestamp, and the spec's event-specific fields. Display creator as a non-pinging user ID. Escape user-provided formatting, count Unicode safely, and cap the rendered content to 2000 UTF-16 units while preserving the heading and market ID. Append a truncation marker within that bound. Always set `CreateAllowedMentions::new().everyone(false).all_users(false).all_roles(false).replied_user(false)` through the same builder pattern already used in `discord.rs`.
- [ ] Implement overflow-safe retry arithmetic with a capped exponent: `5_i64.saturating_mul(1_i64 << (failures.saturating_sub(1).clamp(0, 6) as u32)).min(300)`, then take the maximum with a nonnegative provider delay and saturating-add to completion time. Classify transport/timeouts and server failures as retryable; permission/missing-channel/payload failures pause the guild with fixed safe reason strings; authentication failure requires operator correction. Do not store raw response bodies or credentials.
- [ ] Repeat the output tests, format, and commit as `feat(prediction-bot): render announcements and define retry policy`.

## Task 4: Durable delivery, conditional outcomes, and recovery

**Files:** modify persistence/worker modules, `tests/announcements_test.rs`, and its support fixture.

**Consumes:** `enqueue`, `SnapshotV1`, renderer, retry policy, real `Store` and Serenity `Http`.

**Produces:** `next_due`, `still_eligible`, `deliver_due`; conditional outcome updates. Define the outcome in `worker.rs`:

```rust
pub(crate) enum AttemptOutcome {
    Delivered { message_id: u64 },
    Retry { reason: &'static str, provider_delay: Option<i64> },
    Pause { reason: &'static str },
}
```

The persistence completion helper is `finish_attempt(store: &Store, item: &PendingAnnouncement, outcome: &AttemptOutcome, completed_at: i64) -> Result<(), StoreError>`.

- [ ] Extend the private fixture with `discord_http(&MockServer) -> Http` using the existing proxy helper, dummy token/application ID, and disabled rate limiter. Define a fixed `Clock` with `Arc::new(|| 1000)` and an adjustable one using `Arc<AtomicI64>` for multi-attempt tests.
- [ ] Configure Wiremock to return a valid Discord message response on `POST /api/v10/channels/20/messages`. Verify that route, required message text, and disabled mentions. Return the same minimum Discord JSON fields as the existing successful mention-send test. Start with an assertion that one queued market results in one message and no pending work after `deliver_due`.
- [ ] Add an HTTP failure/restart scenario: first return a retryable error, call `deliver_due`, reconstruct the store and clock, then call at 1004 and 1005. Verify no early POST and delivery at the due time. No wall-clock sleeps.
- [ ] Run `//prediction_bot/lib:announcements_test` and confirm the new delivery tests fail before implementation.
- [ ] Implement due selection using enabled/unpaused settings and `NOT EXISTS` any lower pending revision for that guild. Apply the deadline condition only after identifying the oldest pending event. Order eligible guilds by deadline/guild, selecting at most 8 per batch. Decode snapshots with an explicit version check. Do not claim rows permanently or hold a transaction while sending.
- [ ] Recheck pending state, destination/version, enabled/pause state, and earliest revision immediately before sending. Process the selected distinct guilds concurrently with Tokio `JoinSet`, each request bounded by a 30-second timeout. Await the whole batch before the next one; isolate each guild's outcome so another guild continues. Acquire `clock()` after I/O for the next retry deadline.
- [ ] In `finish_attempt`, acquire the guild transaction lock. A success changes a still-pending row to delivered and records the actual attempted channel/message ID even if configuration changed. A discarded row stays discarded. A failure updates attempts/deadline or pause only if the row is pending and configuration version still matches. Overflow-safe counters must never panic. Errors saving completion retain recoverable pending work and are surfaced to the supervisor.
- [ ] Add deterministic race tests with an endpoint that signals request arrival and waits for release: set a new channel or disable while blocked, then release success/failure. Assert stale failures cannot pause the new destination, pending items use the new channel, discarded work stays discarded, and successful old-channel delivery is not reposted.
- [ ] Inject a database constraint rejecting `state = 'delivered'` after HTTP acceptance. Reconstruct the worker against the same DB, remove the fixture constraint, and verify work retries rather than disappears; explicitly permit two HTTP effects. Also verify creation precedes resolution and another server progresses while one server fails.
- [ ] Run the storage/HTTP integration target, format, and commit as `feat(prediction-bot): deliver durable announcements with retries`.

## Task 5: Discord configuration commands and safe destination validation

**Files:** create `discord/announcements.rs`; modify `discord.rs`, `discord_tests.rs`, `README.md`.

**Consumes:** `ConfigurationChange`, configuration/status store methods and real `Http`.

**Produces:** `Action::AnnouncementsSet { channel_id: u64 }`, `Action::AnnouncementsStatus`, `Action::AnnouncementsDisable`, and `InputValue::Channel(u64)`; nested registration and parsing.

- [ ] Add parser and real-HTTP handler tests for nested `/market announcements set`, `status`, and `disable`. Keep existing one-level command tests unchanged. Exercise non-admin, DM, bot-authored, missing/extra options and cross-guild destination rejection.
- [ ] Register `announcements` as a `SubCommandGroup` with `set`, `status`, `disable`; `set` takes a required channel option restricted to ordinary text channels. Do not restrict the root `/market` command to administrators, which would hide normal member commands.
- [ ] In `from_discord`, accept either an existing `SubCommand` or the specifically named group containing exactly one supported subcommand. Flatten that group to `announcements.set`, `announcements.status`, or `announcements.disable` in `Input.subcommand`; reject other nesting. Parse channel option values to the new typed variant.
- [ ] Add these permission assertions using the existing `input()` fixture:

```rust
#[test]
fn announcement_configuration_requires_server_management() {
    let mut request = input("announcements.disable", vec![]);
    assert!(parse(&request).is_err());
    request.moderator = true;
    assert!(matches!(parse(&request), Ok((10, _, Action::AnnouncementsDisable))));
}
```

- [ ] Route actions through the existing deferred private-response path. Validate channel type/guild and effective bot permissions before calling `configure_announcements`. Fetch the channel, guild role information and bot member roles through the injected HTTP client as needed; apply everyone/role/member overwrites using the installed Serenity permission APIs. Check View Channel and Send Messages. A read failure produces a safe private error and no settings write. Never send a probe announcement during validation.
- [ ] Use a focused `validate_destination(http: &Http, guild: u64, bot_user_id: u64, channel_id: u64) -> Result<(), &'static str>` in the Discord announcement module. Independently keep actor authorization in the store, so bypassing the parser cannot alter settings.
- [ ] Status reports channel, disabled/enabled/paused, pending count and safe reason. Configuration receipts explain the in-flight exception. Help and README describe setup, permissions, supported events, retries, no backfill, disable semantics and possible duplicate delivery.
- [ ] Run `//prediction_bot/lib:discord_test` and configuration integration tests; confirm unrelated market commands and mention replies remain unchanged. Format and commit as `feat(prediction-bot): expose announcement channel commands`.

## Task 6: Production worker composition and end-to-end acceptance

**Files:** modify worker module, `discord.rs`, integration tests, and worker-related audit definitions/tests only as required by the existing typed logging convention.

**Consumes:** `deliver_due`, existing gateway guard, grant-worker lifecycle, real client HTTP handle, and shutdown watch receiver.

**Produces:** `start_announcement_worker` and production wiring through `run_gateway_client`.

- [ ] Add a composition integration test that queues an announcement, calls `start_announcement_worker` with the migrated runtime store and proxied real HTTP client, observes the outgoing message via a notification, then sends shutdown and awaits the returned handle with a timeout. Assert a later event is still pending after shutdown. Include a failed-send/restart case through this same entrypoint.
- [ ] Implement an immediate first poll and a one-second Tokio interval between completed batches. Use `MissedTickBehavior::Skip`; do not overlap `deliver_due` calls. Exit when shutdown is true or its sender closes. Report discovery/persistence failures through existing typed audit conventions and continue polling without losing pending work.
- [ ] In `run_gateway_client`, start the new worker with `Arc::clone(&client.http)`, `Arc::new(|| chrono::Utc::now().timestamp())`, the existing store and a cloned shutdown receiver. Start only while the gateway guard is held. Supervise unexpected task exit, so the gateway cannot silently run with a dead announcement worker. On normal shutdown stop discovery and grant at most the existing 15-second shutdown budget before aborting and awaiting unfinished work. Keep one active send per guild.
- [ ] Add a focused production-composition check through the shared startup path; retain the existing gateway startup/failure tests. Verify slash and modal creation both use `Store::execute` and therefore enqueue exactly once. When the full gateway cannot be exercised without live credentials, document that fidelity gap rather than asserting live readiness from a worker-only test.
- [ ] Run the final checks:

```bash
nix develop --command aspect test //prediction_bot/lib:domain_test //prediction_bot/lib:events_test //prediction_bot/lib:discord_test //prediction_bot/lib:store_test //prediction_bot/lib:announcements_test
nix develop --command aspect format --scope=all
nix develop --command aspect build //...
git diff --check
```

- [ ] Confirm only intended files changed. Report measured test duration, actual results, preserved private responses/append-only history, and the remaining live Discord/rate-limiter/gateway evidence gaps. Commit as `feat(prediction-bot): run and supervise announcement delivery`.

## Plan self-review and execution handoff

Spec coverage: configuration and runtime privileges (Task 1); atomicity, event scope and no backfill (Task 2); content and retry arithmetic (Task 3); ordering, recovery, crash duplicates and stale configuration outcomes (Task 4); Discord permissions/private replies/help (Task 5); production lifecycle and complete verification (Task 6).

The listed tests are proposed checks, not passing evidence. Keep real database semantics and HTTP adapters under test. No new feature source files have been changed while writing this plan.

Execution options are subagent-driven task-by-task implementation with review, or inline execution using executing-plans. Select the execution workflow before starting Task 1.
