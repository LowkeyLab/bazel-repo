# Prediction bot operational audit events

Date: 2026-09-17
Status: Design approved in conversation; written specification awaiting review.

## Purpose and scope

Introduce typed operational audit events and a directly invoked listener in `prediction_bot`. The first production listener writes structured logs using the existing tracing infrastructure. Later listeners may derive metrics and traces without changing producers. Metrics exporters, tracing backends, queues, and durable audit delivery are outside this increment.

The consumers are operators diagnosing rejected commands, unavailable queries, missing grants, and lost Discord responses. Events must distinguish these outcomes and identify the relevant guild and interaction or grant command. The existing immutable economy events and command receipts remain the durable recovery record. Operational audit events are best effort and do not replace that history.

This addresses four reviewed defects: expected rejections logged as errors, query failures without an application diagnostic, delivery warnings without receipt correlation, and grant failures without actionable categories.

## Architecture and composition

Add an audit module containing the event contract, a synchronous `AuditListener: Send + Sync` boundary, and the structured logging listener. An emission accepts an event by reference and returns no business result. Production constructs one shared listener and injects it into the store and Discord orchestration. Startup and shutdown reporting use the same listener once it is constructed. Existing public construction and run entrypoints remain available and delegate to the same implementation with the production listener; explicitly injected entrypoints support composition and tests. Do not use global mutable listener registration or a test-mode bypass.

The flow is:

`startup / store / Discord / grant worker -> typed AuditEvent -> AuditListener -> structured log`

Direct invocation avoids a queue, overflow policy, background worker, and shutdown drain. Logging may add latency and may lose output on process failure. Listener implementations must not perform remote I/O, panic, or turn reporting failures into command failures. This is a listener contract, not a promise to recover from arbitrary Rust panics or process aborts. The initial listener uses the existing tracing writer behavior and cannot return an error to economic operations. Do not recursively audit audit-output failures.

Future listeners can own their own buffering and exporter lifecycle. This increment does not add speculative fan-out infrastructure.

## Event contract

Use typed variants and enums rather than raw strings or arbitrary payload maps for operation, outcome, stage, and failure category. Supply fields appropriate to each variant rather than requiring meaningless identifiers on process-wide events.

Event families:

| Family                               | Outcomes and context                                                                                                                        |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| Command completion                   | Success, expected rejection, or operational failure; command kind, guild, command key, safe reason/category, elapsed duration               |
| Query completion                     | Success or failure; query kind, guild, interaction ID, safe category, elapsed duration; component read timeout is an explicit failure       |
| Interaction acknowledgement/delivery | Success or failure; guild when available, interaction ID, acknowledgement or response stage, safe Discord status/code or transport category |
| Grant discovery/reconstruction       | Failure with safe category; guild when known                                                                                                |
| Command registration                 | Success or failure; guild and safe category                                                                                                 |
| Lifecycle                            | Migration/startup/readiness/shutdown outcomes with stage and application ID when known                                                      |

Command keys already distinguish Discord requests from grant attempts. Grant keys retain the selected schedule boundary. Interaction IDs are identifiers, never interaction tokens. Prefer these existing keys to a second correlation-ID system.

A command completion describes an invocation, not a count of newly committed economic effects. Redelivery may return an existing receipt successfully. Future economic metrics must not interpret successful invocation counts as new bets or grants.

Durations use monotonic elapsed time and are descriptive diagnostics. They do not alter domain timestamps or scheduling. Tests assert meaningful fields and nonnegative duration behavior without sleeps or exact elapsed-time assertions. Logging supplies event timestamps.

## Placement and failure semantics

The store emits one final command completion per invocation after bounded transaction retries finish. Success follows commit or successful receipt recovery. Expected command validation failures retain their private rejection response and do not become server errors. Validation of corrupt stored history remains an operational failure even when the underlying domain error resembles a user rejection; classification must preserve the stage where the failure occurred.

Query orchestration emits the outcome of the actual requested read, including component timeouts. Internal replay reads do not emit duplicate user-query events. Grant discovery and reconstruction report their own failures; grant execution uses the command completion event, avoiding a second report of the same command failure. Continue processing unaffected accounts and guilds, and retain subsequent scheduled retries.

Discord acknowledgement and response delivery emit separate outcomes. Failure to acknowledge still prevents execution except for the existing already-acknowledged recovery case. A failed response after a successful command does not undo or relabel the command. Its interaction ID matches the `discord:<interaction_id>` receipt key, allowing operators to locate committed state.

Commit errors may leave commit status unknown. Do not claim rollback from a generic database failure. If receipt recovery commits its read transaction and a following projection refresh fails, report that refresh stage rather than suggesting a new economic command was rolled back. Preserve receipt-based retry behavior.

Migrate existing justified application logs for registration, startup, grant processing, delivery, and shutdown to the listener. Keep fatal startup errors visible at the executable boundary, including failures before normal initialization; do not require a remote reporting service. Third-party library logs remain outside this application event contract.

## Structured output and data minimization

The logging listener chooses severity and a stable event name. Producers supply facts rather than formatting text or choosing severity.

- Informational: successful outcomes, expected command rejections, and normal lifecycle transitions.
- Warning: failed acknowledgements/delivery and bounded read/shutdown timeouts.
- Error: exhausted operational command failures, database/replay failures, failed grant discovery/reconstruction, registration failures, and fatal lifecycle failures.

Configure the executable tracing subscriber to emit newline-delimited JSON: one valid JSON object per log record. Each audit record includes its timestamp, severity, stable event name, and typed event fields as structured JSON fields, not a serialized payload inside a message string. Use the existing tracing infrastructure with its JSON formatting support. New log collection infrastructure is not required. Error classification uses an allowlist of stable categories, database SQLSTATE where available, and Discord status/error codes where available. Distinguish connection/pool failures, database constraints, replay/history failures, arithmetic overflow, timeouts, and transport failures. Unknown errors receive a safe fallback category.

Never pass raw SQLx/Serenity error objects, URLs, credentials, interaction tokens, SQL parameters, market questions, outcome labels, or response content into audit events. Do not infer redaction safety from Display or Debug implementations. Only explicitly selected safe fields cross the listener boundary. High-cardinality correlation fields are for diagnosis, not future metric labels.

The repository establishes local and container execution but not production collection, retention, or alert ownership. Document that limitation without adding infrastructure or assuming logs are centrally retained.

## Test seams and verification

The immediate testability obstacles are inline tracing calls, Discord I/O coupled to orchestration, and failures whose causes disappear before reporting. The smallest useful seams are the approved listener injection and a narrow acknowledgement/delivery adapter used by the actual handlers. Keep economic decisions, retry logic, and storage real. Do not introduce a repository interface solely to mock PostgreSQL.

A test-local recording listener observes the public event contract. Assertions compare event meaning, correlation, and outcomes, not private helper calls or constructor order. Duplicate final outcomes are a contract defect, so relevant tests may assert their absence. Keep recorders and fake Discord adapters in test support.

Use the existing isolated PostgreSQL fixtures for storage integration checks. Treat PostgreSQL as the application's managed persistence boundary for these tests; no additional external database consumer contract is established by this work. Discord is an external boundary: a controlled adapter can simulate response failure without calling the live service. Such tests do not establish live Discord compatibility.

Required checks:

1. Insufficient balance produces an expected rejection event and preserves balances; the logging listener does not render it at ERROR.
2. A forced append failure produces a categorized operational event and leaves no partial economic changes.
3. A failed query/replay and a component read timeout produce correlated failure events while preserving safe private responses. Corrupt history is not classified as user rejection.
4. A committed bet followed by Discord delivery failure produces separate correlated command-success and delivery-failure events; balance and receipt remain committed, and redelivery cannot charge twice.
5. A rejected acknowledgement prevents mutation; the existing already-acknowledged case still permits receipt recovery.
6. Grant discovery, reconstruction, and execution failures retain safe categories and relevant schedule keys. Failure for one account does not suppress an otherwise valid account's grant.
7. Capture output from the real logging listener using the production JSON formatter. Parse each nonempty line as a JSON object and assert required event names, levels, and correlation fields with their intended JSON types. Assert that sentinel secrets carried by representative raw input errors are absent. Do not compare whole rendered lines, timestamps, or JSON key order.
8. A focused composition check exercises the same construction path used in production with the real logging listener and a captured writer. Verify observable output rather than constructor calls. Exercise the actual Discord transport adapter with a controlled endpoint where the existing client permits it; report any untested adapter or live-provider boundary explicitly.

Capture existing behavior before changing the relevant paths. Demonstrate the reviewed regressions failing before their fixes when the current seams permit it; if a minimal seam must first be introduced, state that limitation. No test claiming to exercise production behavior may bypass the new production wiring.

Tests should protect semantic outcomes, tolerate internal refactoring, and use direct fixtures. Runtime and feedback speed remain unverified until execution. Run focused Bazel tests before broader checks and report actual results and fidelity gaps. After every source edit run `bazel run //:gazelle` immediately, then format with `aspect format --scope=all`. Complete relevant tests and `aspect build //...` according to repository instructions.

## Acceptance

The four audit findings are covered by typed, safely classified, correlated events through the direct listener. The production executable installs the structured logging listener and emits newline-delimited JSON records. Existing economic transactions, receipt recovery, retry policy, private user responses, and grant scheduling remain correct. Tests verify both the event contract and real logging composition. No durable operational event store, metrics backend, tracing exporter, or unrelated refactor is introduced.
