# Prediction Bot Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking. Subagent execution is an alternative only if selected by the user.

**Goal:** Correct the four logging-audit findings using typed operational events delivered directly to a listener, initially backed by JSON diagnostic logging.

**Architecture:** Producers emit sanitized facts through a shared synchronous listener. The store owns final command outcomes, Discord orchestration owns query and delivery outcomes, and the worker owns discovery/reconstruction failures. The production listener maps these events to tracing records; tests observe event semantics and application behavior, not rendered logs.

**Tech Stack:** Rust, Tokio, SQLx/PostgreSQL, Serenity, tracing/tracing-subscriber, Bazel/Aspect, existing testcontainers fixtures.

**Spec:** `docs/superpowers/specs/2026-09-17-prediction-bot-audit-design.md` (approved, including JSON output and the exclusion of diagnostic-output tests).

## Global Constraints

- Operational audit events are best effort and do not replace that history.
- Producers supply facts rather than formatting text or choosing severity.
- Listener implementations must not perform remote I/O, panic, or turn reporting failures into command failures.
- No global listener registry, durable delivery, queue, metrics exporter, tracing backend, or speculative fan-out.
- Do not add tests that capture or parse JSON log output, assert rendered severity or field layout, or snapshot log lines.
- JSON formatting remains a production configuration requirement verified by inspecting the subscriber setup.
- Preserve economic transactions, receipt recovery, acknowledgement gating, private user responses, and grant retries.
- Use Bazel/Aspect through `nix develop --command`; do not invoke Cargo directly.
- After every source-file edit, immediately run `nix develop --command bazel run //:gazelle`, before the next source edit or formatting. This also applies to tests. Do not manually edit BUILD files before Gazelle.
- Before each implementation commit and final completion run `nix develop --command aspect format --scope=all`. Inspect and retain only task-related changes.
- Run the focused checks below and ultimately `nix develop --command aspect build //...`. Record environmental blockers distinctly from failing assertions.

## File responsibilities

| File                                                               | Responsibility                                                                                |
| ------------------------------------------------------------------ | --------------------------------------------------------------------------------------------- |
| `prediction_bot/lib/src/audit.rs` (new)                            | Typed event contract, safe categories, listener interface, production listener factory        |
| `prediction_bot/lib/src/audit/logging.rs` (new)                    | Event-to-tracing field and severity mapping                                                   |
| `prediction_bot/lib/src/audit_tests.rs` (new)                      | Safe classification behavior; no log-output assertions                                        |
| `prediction_bot/lib/src/lib.rs`                                    | Export audit module                                                                           |
| `prediction_bot/lib/src/store.rs`                                  | Listener injection, operation-stage tracking, one final command outcome, grant failure events |
| `prediction_bot/lib/src/discord.rs`                                | Actual handlers use the orchestration seam; query/delivery/registration/lifecycle events      |
| `prediction_bot/lib/src/discord/transport.rs` (new)                | Narrow acknowledgement/delivery interface and Serenity implementation                         |
| `prediction_bot/lib/src/discord_tests.rs`                          | Existing adapter behavior and additional listener/transport behavior checks                   |
| `prediction_bot/lib/tests/store_test.rs`                           | Real PostgreSQL event/state integration tests and test-local recorder                         |
| `prediction_bot/bin/src/main.rs`                                   | Shared listener composition and JSON subscriber setup                                         |
| `prediction_bot/README.md`                                         | Operational event and best-effort delivery documentation                                      |
| `Cargo.toml`, `Cargo.lock`, `MODULE.bazel.lock`                    | JSON formatter feature and only necessary generated dependency changes                        |
| `prediction_bot/lib/BUILD.bazel`, `prediction_bot/bin/BUILD.bazel` | Necessary target/dependency updates after Gazelle                                             |

Existing `lib` sources use a glob and `discord_test` compiles that library's tests. Prefer these existing targets; do not create a separate test target for each module. Keep the recorder in test code rather than exporting production test helpers.

## Task 1: Introduce the event contract and safe classification

**Files:** Create `audit.rs`, `audit/logging.rs`, `audit_tests.rs`; modify `lib.rs` and, only as needed after Gazelle, `lib/BUILD.bazel`.

**Interfaces produced:**

```rust
pub trait AuditListener: Send + Sync {
    fn on_event(&self, event: &AuditEvent);
}
pub type SharedAudit = std::sync::Arc<dyn AuditListener>;
pub fn logging_listener() -> SharedAudit;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Validate, Acquire, Replay, Decide, Append, Commit, Refresh,
    Query, Acknowledge, Deliver, Discover, Reconstruct, Register,
    Migrate, Startup, Ready, Shutdown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureCategory {
    Configuration, Database, Connection, PoolTimeout, Constraint,
    History, Metadata, Overflow, Timeout, Transport, Discord, Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub category: FailureCategory,
    pub sqlstate: Option<String>,
    pub http_status: Option<u16>,
    pub discord_code: Option<isize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    InsufficientPoints, NotEnrolled, MarketUnavailable,
    PermissionDenied, InvalidInput,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome { Succeeded, Rejected(Rejection), Failed(Failure) }

pub fn store_outcome(stage: Stage, error: &crate::store::StoreError) -> Outcome;
pub fn discord_failure(error: &serenity::Error) -> Failure;
```

Define `CommandKind` for the existing Join/Create/Bet/Resolve/Cancel/Grant commands, `QueryKind` for Balance/Leaderboard/List/Show/Component reads, and `LifecycleKind` for Migration/Startup/Ready/Shutdown. Define `AuditEvent` as variants named `CommandCompleted`, `QueryCompleted`, `InteractionCompleted`, `GrantFailed`, `RegistrationCompleted`, and `Lifecycle`. All carry an `Outcome` and `Stage`; command/query variants carry `std::time::Duration`. Command fields are `guild: u64`, `key: Option<String>`, and `command: CommandKind`; query fields are `guild: u64`, `interaction_id: u64`, and `query: QueryKind`. Interaction fields are `guild: Option<u64>` and `interaction_id: u64`. Grant discovery/reconstruction carry `guild: Option<u64>`; execution events use the command variant and grant key. Registration carries `guild: u64`; lifecycle carries `kind: LifecycleKind` and `application_id: Option<u64>`.

Only validated existing command keys enter the event; reject malformed input without copying arbitrary key contents. Valid keys are canonical numeric `discord:<id>` and `grant:<user>:<boundary>` strings. Do not change which keys public store methods accept solely to improve diagnostics: emit `None` for noncanonical keys accepted by existing tests/callers. In-memory events contain no raw error or payload fields.

- [x] Add focused classification regression tests with explicit expected outcomes. Run them before implementation, noting compilation failure separately from an observed behavior failure.

```rust
#[test]
fn replay_validation_is_an_operational_failure() {
    let error = StoreError::Domain(DomainError::Invalid("insufficient points"));
    assert!(matches!(
        store_outcome(Stage::Replay, &error),
        Outcome::Failed(Failure { category: FailureCategory::History, .. })
    ));
    assert_eq!(
        store_outcome(Stage::Decide, &error),
        Outcome::Rejected(Rejection::InsufficientPoints)
    );
}

#[test]
fn configuration_details_do_not_enter_the_event_contract() {
    let error = StoreError::Database(sqlx::Error::Configuration(
        "postgres://sentinel-secret@private-host/database".into(),
    ));
    assert_eq!(store_outcome(Stage::Acquire, &error), Outcome::Failed(Failure {
        category: FailureCategory::Configuration,
        sqlstate: None, http_status: None, discord_code: None,
    }));
}
```

- [x] Run `nix develop --command aspect test //prediction_bot/lib:discord_test`; record the intended missing-contract failure.
- [x] Implement the contract and classification. Match domain rejection reasons only at `Decide`, allowlist externally safe SQLSTATE/status/code fields, and classify replay validation as history failure. Unknown reasons use a safe category rather than copying text.
- [x] Implement `logging_listener()` returning `Arc::new(LoggingListener)`. Map variants to stable event names and named tracing fields; do not Debug-format entire events or raw errors. Success/rejection maps to INFO, delivery and timeout failures to WARN, operational failures to ERROR. Code-review this mapping; do not test rendered logs.
- [x] Run the same focused tests, format, review the diff, and commit `feat(prediction_bot): add typed operational audit events`.

## Task 2: Emit store command and grant outcomes through the listener

**Files:** Modify `store.rs`, `lib/tests/store_test.rs`, and any necessary BUILD declarations after Gazelle.

**Interfaces consumed:** Task 1 event contract, `SharedAudit`, `logging_listener`, `store_outcome`.

**Interfaces produced:**

```rust
pub fn new_with_audit(
    pool: PgPool, application: u64, defaults: Policy, audit: SharedAudit,
) -> Store;
pub async fn connect_with_audit(
    url: &str, application: u64, defaults: Policy, audit: SharedAudit,
) -> Result<Store, StoreError>;
pub(crate) fn audit(&self) -> &SharedAudit;
```

Keep `Store::new` and `Store::connect` as delegating entrypoints using the production listener. `execute` and `execute_at` retain their existing signatures and results. Existing deterministic time entrypoints remain sufficient; do not add a clock framework.

- [x] Run current `//prediction_bot/lib:store_test` and `//prediction_bot/lib:discord_test` before moving behavior. Docker/remote-executor failures are a fidelity gap, not permission to replace PostgreSQL with a mocked repository.
- [x] Extend the PostgreSQL fixture to accept `SharedAudit`; retain its existing convenience wrapper. Use a test-local recorder:

```rust
#[derive(Default)]
struct Recorder(std::sync::Mutex<Vec<AuditEvent>>);
impl AuditListener for Recorder {
    fn on_event(&self, event: &AuditEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}
```

Extend the existing failed-append test to assert one final operational outcome alongside its existing revision-zero assertion. Use existing enrollment/market setup to reject a stake greater than balance, assert unchanged balance and `Rejected(InsufficientPoints)`, and verify replay failures classify as operational. A representative assertion after calling the real store is:

```rust
let events = recorder.0.lock().unwrap();
let commands: Vec<_> = events.iter().filter_map(|event| match event {
    AuditEvent::CommandCompleted { key, outcome, .. }
        if key.as_deref() == Some("discord:99") => Some(outcome),
    _ => None,
}).collect();
assert_eq!(commands, vec![&Outcome::Rejected(Rejection::InsufficientPoints)]);
```

- [x] Run `nix develop --command aspect test //prediction_bot/lib:store_test` and capture failing event assertions before adding emissions where feasible.
- [x] Retain the retry loop but collect its final result before emitting. Track stage internally without changing public errors. One practical implementation is a private transaction result carrying `(Stage, StoreError)` on failure; tag each acquisition, replay, decision, append, commit, and post-receipt refresh boundary using `map_err`. Adapt retry matching to inspect the contained database error. Emit after retry completion, then return the original `StoreError` to callers.

```rust
let outcome = match &result {
    Ok(_) => Outcome::Succeeded,
    Err((stage, error)) => store_outcome(*stage, error),
};
// Build CommandCompleted with validated correlation and elapsed time here.
result.map_err(|(_, error)| error)
```

No per-retry event, no success before commit, and no assumed rollback on commit failure. Receipt recovery refresh failure has stage `Refresh`. Invalid command-key input receives `Validate` and safe omitted correlation.

- [x] Replace `grant_due` reconstruction logs with `GrantFailed`; report discovery failure once at its worker boundary. Remove the per-grant execution log because `execute` now reports the final command outcome. Preserve continued iteration and future retry behavior.
- [x] Add a real PostgreSQL constraint that rejects one enrolled account's grant receipt, allow another account's grant, and invoke the actual `grant_due` method. Set enrollment time sufficiently in the past via `execute_at`; do not sleep. Assert the failed account remains unchanged, the other account advances, and the failed grant event retains its schedule key. Also exercise discovery failure and corrupt-guild reconstruction independently.
- [x] Run `store_test`, `domain_test`, and `discord_test`; format and commit `feat(prediction_bot): audit store and grant outcomes`.

## Task 3: Make Discord orchestration observable at the event boundary

**Files:** Modify `discord.rs`, `discord_tests.rs`, `lib/tests/store_test.rs`; create `discord/transport.rs`.

**Interfaces consumed:** `Store::audit`, event types, `discord_failure`, unchanged public store operations.

**Interfaces produced:** A narrow transport trait consumed by the actual handlers and their shared deferred-response orchestration. Use Serenity's async-trait support already used by `EventHandler`:

```rust
#[serenity::async_trait]
pub trait InteractionTransport: Send + Sync {
    async fn acknowledge(&self) -> serenity::Result<()>;
    async fn edit(&self, response: EditInteractionResponse) -> serenity::Result<()>;
    async fn respond(&self, response: CreateInteractionResponse) -> serenity::Result<()>;
}
```

Concrete command/modal/component wrappers borrow their interaction and `Http`; implement methods using the existing Serenity calls. Components retain initial-response semantics and are never deferred. Expose only the minimal orchestration entrypoint needed for the PostgreSQL integration target, not `Handler` internals. Its inputs are the transport, `Arc<Store>`, guild, actor, command, and numeric interaction ID; its implementation is the same one used for slash and modal writes. Keep query/rendering helpers real.

- [x] Preserve and run existing acknowledgement and safe-response tests before extraction. Add a test-local transport that returns a chosen acknowledgement result and records delivered response builders; it must not emulate economic behavior.
- [x] Exercise the shared write orchestration with real PostgreSQL and a transport whose edit fails. Use canonical numeric interaction IDs. Assert the committed balance, persisted receipt, `CommandCompleted(Succeeded)`, and `InteractionCompleted(Failed)` with matching correlation. Repeat the same interaction and verify no second charge. Assert no mutation and an acknowledgement-failure event when acknowledgement fails. Existing 40060 recovery remains accepted and is not reported as a terminal failure.

Representative outcome assertion:

```rust
assert!(events.iter().any(|event| matches!(event,
    AuditEvent::InteractionCompleted {
        interaction_id: 123, stage: Stage::Deliver,
        outcome: Outcome::Failed(_), ..
    }
)));
assert!(events.iter().any(|event| matches!(event,
    AuditEvent::CommandCompleted {
        key: Some(key), outcome: Outcome::Succeeded, ..
    } if key == "discord:123"
)));
```

- [x] Run `store_test` and `discord_test` to show the missing event or correlation before implementing emissions. Report extraction-only compile failures separately.
- [x] Route acknowledgement/edit/initial-response calls through the real adapter and shared orchestration. Remove `execute_request`'s unconditional ERROR log; the store now owns that outcome. Emit sanitized acknowledgement/delivery facts separately from store results, including on modal and component paths.
- [x] Emit final query events around the actual `store.view` calls in slash and component handlers. Report component timeout explicitly. Extract the bounded-read orchestration to accept its read future and timeout duration if needed for deterministic testing, rather than substituting a store repository. Production supplies the real view future and existing two-second limit. A pending future with a zero test timeout exercises the timeout branch without sleeps; do not assert elapsed duration.
- [x] Test failed reads and timeouts retain the existing safe responses and identify guild/interaction/query stage. Test the normal composition path with the recorder; do not create a separate test-only orchestration implementation. Parsing and form validation rejections should also use a safe rejection event at their boundary, with existing input messages preserved and no copied input fields.
- [x] Replace registration, grant discovery, and lifecycle application logs with typed events using the store's listener. Preserve existing recovery/shutdown semantics. Do not add an independent worker supervision redesign.
- [x] Inspect the concrete Serenity wrappers and, if the pinned client's endpoint configuration permits, exercise them against a controlled HTTP endpoint. Explicitly record any remaining transport/live-provider fidelity gap; injected transport tests alone do not establish live Discord compatibility.
- [x] Run `discord_test` and `store_test`, format, and commit `feat(prediction_bot): audit Discord request and delivery outcomes`.

## Task 4: Wire JSON logging and complete repository verification

**Files:** Modify `bin/src/main.rs`, `Cargo.toml`, necessary generated lockfiles/BUILD declarations, and `prediction_bot/README.md`.

**Interfaces consumed:** `logging_listener`, `Store::connect_with_audit`, existing `discord::run` using `Store::audit`.

- [x] Enable the existing dependency's JSON feature without changing its version requirement:

```toml
tracing-subscriber = { version = "0.3.20", features = ["json"] }
```

Use `nix develop --command bazel run //tools:cargo_lock` if lockfile resolution needs regeneration. Inspect generated dependency diffs and avoid unrelated version changes. Never invoke Cargo directly.

- [x] Configure the production subscriber and listener:

```rust
tracing_subscriber::fmt()
    .json()
    .with_max_level(tracing::Level::INFO)
    .init();
let audit = prediction_bot::audit::logging_listener();
```

Pass a clone to `Store::connect_with_audit`. Use the same listener for migration/startup/lifecycle records, emitting safe failure categories before preserving fatal exit results. Keep pre-initialization fatal diagnostics available. No captured writer, JSON parse test, severity snapshot, or log-field assertion is permitted. Review subscriber configuration and exhaustive listener mapping directly.

- [x] Update README with the event families, shared correlation keys, best-effort inline delivery, possible latency/loss, JSON records, safe-data policy, and future metrics/tracing extension boundary. State that emitted logs do not imply configured retention or alert delivery. Document successful invocations versus new economic effects so redelivery is not mistaken for a new grant/bet.
- [x] Run focused verification:

```bash
nix develop --command aspect test //prediction_bot/lib:domain_test //prediction_bot/lib:events_test //prediction_bot/lib:discord_test //prediction_bot/lib:store_test
nix develop --command aspect format --scope=all
nix develop --command aspect build //...
git diff --check
```

- [x] Review every original application tracing call in `prediction_bot` against the event mapping. Verify raw error objects and payloads never enter events; verify reply, commit, retry, and shutdown behavior is preserved. Check one final command event per invocation and no worker/handler duplicates.
- [x] Review test quality: meaningful regression triggers, semantic rather than internal-call assertions, measured feedback time, and understandable private fixtures. Record commands/results, original-failure evidence, PostgreSQL execution evidence, and any production adapter gap. Do not label a proposed check as executed.
- [x] Commit `feat(prediction_bot): enable JSON audit logging` with only related code, generated dependency changes, and documentation. Report final state and remaining verification limitations.

## Plan self-review

Coverage: Task 1 defines typed/sanitized facts and severity mapping; Task 2 covers command and grant audit defects plus retry/commit semantics; Task 3 covers query, acknowledgement, delivery, registration, and lifecycle paths plus the Discord seam; Task 4 covers JSON composition, documentation, and repository checks. Existing public entrypoints delegate to injected composition. Tests observe listener events and economic behavior, never diagnostic JSON output. Live-provider verification is explicitly separate from test-double evidence.

## Execution evidence (2026-09-18)

Implemented in commits `7ec05dab`, `8c2f0815`, `eab30354`, and `960377e8`, with gateway startup reporting corrected in `66d2db67`. The temporary execution report was removed from tracking in `e8b4ab61`.

- All four prediction-bot targets passed: domain, events, Discord, and PostgreSQL store tests. The final combined run took 50.9 seconds; PostgreSQL ran fresh in 37.1 seconds and the other three targets used cached results.
- After the startup-reporting correction, all 50 Discord tests passed. The 20 PostgreSQL tests had already passed against the real isolated database, including commit followed by failed delivery, idempotent redelivery, rejection without balance changes, and continuation after grant failures.
- `nix develop --command aspect build //...` passed after the final source correction in 26.7 seconds. Full formatting, Gazelle, and `git diff --check` passed.
- Task 1's initial red evidence was a missing-contract compilation failure. Task 2's attempted runtime-red result was not captured and remains unverified; its later passing integration evidence is not presented as a demonstrated before/after regression. Task 3 demonstrated missing query, acknowledgement, and delivery events with runtime RED/GREEN. The startup correction likewise demonstrated a missing lifecycle event through actual `discord::run` with a closed pool before the fix.
- Local HTTP endpoint tests exercise the actual Serenity adapters. No live Discord gateway, process signal/timeout integration, external log collection, retention, or alert delivery was verified. Rendered diagnostic JSON is intentionally outside the test contract.

### Execution decisions

1. JSON configuration and severity mapping were reviewed directly without output tests, as instructed. The tradeoff is no test assertion over rendered diagnostic records.
2. Existing build/tool warnings were recorded without unrelated cleanup. Those warnings remain outside this feature's scope.
3. Acknowledgement failures retain warning severity, as specified. A future consumer requiring error severity would need an explicit policy change.
4. Typed events are tested through classification and application behavior, not constructor-only enum assertions. Rust type checking covers enum structure; tests focus on meaningful outcomes.
5. The repository-wide build ran after integration rather than after every task. This delays detection of cross-project build issues; the final build passed.
6. Added distinct shutdown stages so requested shutdown, gateway timeout, worker timeout, and lock release remain distinguishable. This extends the event vocabulary. Creation-menu reads use the existing Component query kind; a future consumer needing finer query breakdown would require a new kind.

### Final review corrections (2026-09-18)

- Corrected severity selection in this revision: only acknowledgement/delivery failures and explicit bounded read/shutdown timers use WARN. Operational command, registration, grant, and fatal lifecycle timeouts use ERROR. Query cancellation carrying SQLSTATE `57014` also remains ERROR; a bounded read timer has no SQLSTATE. Verified this mapping by code review, without rendered-output or severity tests.
- Discord startup/shutdown and gateway-lock-release events now retain the application ID already held by the store. The existing closed-pool test exercises actual `discord::run` and requires the configured ID, `Some(42)`. It failed on that correlation assertion before the wiring fix (49 passed, 1 failed) and passed afterward.
- The final four-target command passed: `nix develop --command aspect test //prediction_bot/lib:domain_test //prediction_bot/lib:events_test //prediction_bot/lib:discord_test //prediction_bot/lib:store_test`. Discord and PostgreSQL tests executed fresh; domain and events used cached results. Bazel reported 45.842 seconds overall and 29.6 seconds for the PostgreSQL target. Gazelle passed immediately after every source edit.
- After those corrections, `nix develop --command aspect format --scope=all` passed without modifying files, `nix develop --command aspect build //...` passed (23.280 seconds reported by Bazel), and `git diff --check` passed. The remaining live Discord, shutdown integration, and external log collection limitations above still apply.

- Final correction commit: `3c682b91`. Scoped final review approved both severity and application-ID corrections with no new findings. Temporary execution reports were removed after preserving the evidence and decisions here.
