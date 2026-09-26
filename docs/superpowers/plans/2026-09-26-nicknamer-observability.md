# Nicknamer Observability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Observe Nicknamer's real application outcomes through safe typed events and production-wired listeners without changing business behavior.

**Architecture:** Concrete PostgreSQL services emit typed observations through a shared sink. The production router installs request context around both web and API routes. The user confirmed local logging: use a synchronous local tracing listener with no OpenTelemetry dependencies or OTLP export.

**Tech Stack:** Rust, Axum, Tokio, SeaORM/PostgreSQL, tracing, googletest, Bazel/Aspect.

**Spec:** `docs/superpowers/specs/2026-09-26-nicknamer-testability-observability-design.md`

## Global Constraints

- Preserve existing routes, response formats, authentication policy, persistence behavior, and bulk partial success.
- Keep DatabaseConnection concrete.
- Measure elapsed duration with Instant and represent it as Duration.
- Application events exclude credentials, cookies, JWTs, DB URLs, names, usernames, Discord/server IDs, raw paths, queries, request bodies, YAML, and raw error messages.
- Never assert rendered diagnostic strings or JSON.
- Add graceful signal handling to drain in-flight requests for at most 10 seconds, then terminate draining, emit the appropriate shutdown outcome, and flush the local listener.
- Run `nix develop --command bazel run //:gazelle` immediately after each source-edit batch and before formatting or manual BUILD edits.
- This plan covers observability and the composition/testing needed to prove it. The independent JWT issuance-time refactor from the combined spec is deferred.

## Review Focus

- Persisted mutation followed by rendering failure must retain committed observation; Task 2 verifies service truth independently of HTTP status.
- Repeated deletion IDs and duplicate-only imports must preserve current counts; Task 2 pins these cases.
- Concurrent requests must never inherit each other's correlation context; Task 3 exercises interleaved futures.
- Cancellation and shutdown timeout must not emit successful request completion; Tasks 3 and 4 cover these exits.
- Listener failure and sensitive input must not escape through fallback or change responses; Tasks 1 and 4 verify isolation and safe diagnostic categories.

## Confirmed delivery scope

The user selected local logging on 2026-09-26. Implement typed events, a local tracing listener, safe request correlation, and bounded application shutdown. No OpenTelemetry SDK, OTLP exporter, remote queue, or hosted destination is part of this implementation. Apply implement-observability's producer/listener separation and structured verification within this explicitly selected scope.

## Task 1: Typed contract, dispatcher, and local listener

**Files:** Create `nicknamer/server/lib/src/observations/{mod.rs,logging.rs}` and `nicknamer/server/lib/tests/observations_tests.rs`; modify library `src/lib.rs` and test `BUILD.bazel` after Gazelle.

**Interfaces:**

- `ObservationSink: Send + Sync { fn record(&self, event: &Observation); }`
- `ObservationListener: Send + Sync { fn on_event(&self, event: &Observation) -> Result<(), DeliveryError>; fn flush(&self) -> Result<(), DeliveryError>; }`
- `SharedObserver = Arc<dyn ObservationSink>`; `Dispatcher::new(Vec<Arc<dyn ObservationListener>>) -> Dispatcher`.
- `Observation { context: ObservationContext, fact: Fact }`; Fact variants and typed fields implement the spec's event table.
- `ObservationContext` contains occurrence time, opaque generated request/operation IDs, optional parent ID, and duration where applicable. Trace context is absent for the local-only delivery option.
- `LoggingListener`; `record_for(&Observation) -> Option<StructuredRecord>` maps severity and safe typed fields before formatting. Successful health requests map to None.

- [ ] Write googletest cases for mutation success severity, routine rejection severity, partial bulk warning, failure error, health suppression, and forbidden-field absence using StructuredRecord values.
- [ ] Add a failing listener followed by a recorder; assert the recorder still receives the same event and the dispatcher returns normally. Test unwind containment only under unwind panic configuration.
- [ ] Run `nix develop --command aspect test //nicknamer/server/lib/tests:observations_tests`; verify the intended new tests fail before implementation.
- [ ] Implement the typed contract and synchronous dispatcher; emit a rate-limited sanitized fallback at most once per 60 seconds for delivery failure, without recursive dispatch. Use an injectable local diagnostic recorder in tests.
- [ ] Implement the logging listener with exhaustive mappings, no raw error formatting, and no network exporter.
- [ ] Run Gazelle, formatter, and the focused target; inspect executed test counts. Commit `feat(nicknamer): add typed observation boundary`.

## Task 2: Persistence, bulk, and export facts

**Files:** Modify `name/mod.rs`, `name/web.rs`, `name/api/v1.rs`, `tests/name_service_tests.rs`; create `tests/observations_service_tests.rs` and `tests/common/observations.rs` under `nicknamer/server/lib/`.

**Interfaces:**

- Add `NameService::with_observer(db: &DatabaseConnection, observer: SharedObserver) -> NameService<'_>`; retain existing new constructor for compatibility with existing callers, but production uses the observed path.
- `NameState` carries the observer with the existing Arc database; update state construction sites.
- `Fact::NameMutationFinished`, `BulkOperationFinished`, `NamesReadFinished`, `ExportPrepared` use the exhaustive bounded enums from Task 1.

- [ ] Write real-PostgreSQL checks: create emits committed after successful write; duplicate emits rejected with no extra row; missing delete emits rejected; persistence error never emits committed.
- [ ] Write bulk checks for malformed input (zero attempts, unknown input count), duplicate-only import (success with skips), empty operation, repeated delete ID, and success plus missing ID (partial failure). Assert persisted rows and typed counts, not query calls.
- [ ] Run the new service target and confirm missing observations fail the checks.
- [ ] Emit mutation facts where writes resolve, before subsequent HTTP rendering; emit read facts at service query completion. Attach batch operation IDs to child facts and emit one batch summary on completion or parse rejection.
- [ ] Emit export facts after serialization with byte and entry counts, distinguishing query and serialization failure. Remove replaced direct logging and implicit argument capture.
- [ ] Add endpoint checks consuming a recorder to prove export composition uses the observer. Verify a later error response cannot rewrite an already captured committed fact; use an accessible response-error boundary if real template failure cannot be triggered, and document that limit.
- [ ] Run Gazelle, format, focused new target and existing service/names targets. Commit `feat(nicknamer): observe persistence and bulk outcomes`.

## Task 3: Authentication and merged request composition

**Files:** Modify `auth/mod.rs`, `auth/api/v1.rs`, `web/mod.rs`, `web/api.rs`; create `observations/http.rs` and `tests/observations_app_tests.rs` under `nicknamer/server/lib/`; update existing auth fixtures.

**Interfaces:**

- `create_app(auth: Arc<AuthState>, names: Arc<NameState>, observer: SharedObserver) -> axum::Router` in web module.
- `observe_request(State(observer): State<SharedObserver>, request: Request, next: Next) -> Response` in observations/http.rs.
- AuthState gains the shared observer, passed explicitly in production composition.
- Request context is scoped to the async request future; a completion guard records aborted on drop and exactly one response-prepared outcome on normal completion.

- [ ] Write integration checks through create_app for browser login and cookie reuse, API token reuse, denied mutation with unchanged database, public health, and `/api/v1` nesting.
- [ ] Assert authentication facts arise from actual credential evaluation, AccessDenied from protected middleware, and one RequestFinished from each request across both routers.
- [ ] Add concurrent interleaved-request and cancellation checks; assert independent correlation IDs and aborted rather than successful completion on cancellation.
- [ ] Run the new application target and verify failing tests identify absent composition/events.
- [ ] Implement the shared factory and outer observer layer, preserving all existing authentication boundaries. Use a bounded route enum including unknown, not a raw URI; map unfamiliar methods to other.
- [ ] Replace unsafe implicit instrument fields and remove duplicate HTTP tracing ownership. Keep context propagation scoped across await points, never a thread-local entered-span guard across await.
- [ ] Run Gazelle, format, new app target and existing auth/web targets. Commit `feat(nicknamer): observe authenticated application requests`.

## Task 4: Startup, shutdown, and normal composition

**Files:** Modify `nicknamer/server/bin/src/main.rs`, library `web/mod.rs`, library `observations/mod.rs`; create library `observations/lifecycle.rs`, `tests/observations_lifecycle_tests.rs`; update `nicknamer/README.md`.

**Interfaces:**

- Composition constructs one dispatcher before Config::from_env and passes it through create_app and service states.
- `ShutdownOutcome::{Drained, TimedOut}`; injectable shutdown notification future for lifecycle checks.
- Lifecycle diagnostic boundary accepts only bounded stage/category values, never the original error or configuration object.

- [ ] Establish baseline using the current binary and a disposable PostgreSQL fixture before moving startup responsibilities; report any environment limitation explicitly.
- [ ] Write checks for invalid configuration, bind/connection/migration failure, successful readiness, shutdown drain and 10-second timeout. Use controlled futures or Tokio test time instead of wall-clock sleeps.
- [ ] Assert listeners are registered before first startup event and request; flush occurs after final observed work; failing listener preserves exit/response semantics.
- [ ] Implement truthful stages and readiness while preserving bind-before-connect ordering; disable SQL statement/parameter diagnostics in the production connection options.
- [ ] Handle termination signals and bounded drain, then flush local listeners. Map startup failures to a sanitized diagnostic and failure exit status without raw anyhow error printing.
- [ ] Exercise the real binary against disposable PostgreSQL for startup, migrations, HTTP access, and shutdown. Document that this is local composition evidence, not hosted delivery evidence.
- [ ] Document local logging, omitted sensitive fields, best-effort loss and blocking limits, and unchanged health semantics.
- [ ] Run Gazelle and `nix develop --command aspect format --scope=all`; run `nix develop --command aspect test //nicknamer/server/lib/tests:tests` including all new targets and verify test counts.
- [ ] Run `nix develop --command aspect build //...` and `nix develop --command aspect lint`; resolve failures attributable to the change and report unrelated blockers precisely. Commit `feat(nicknamer): wire observation lifecycle`.

## Execution handoff

Review this plan with the confirmed local-logging scope. Native execution is recommended because the four tasks share event contracts and state-construction changes. No product code has been edited. The independent testability/JWT refactor remains deferred rather than silently included in observability work.
