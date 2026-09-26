# Nicknamer production composition and observability

Date: 2026-09-26
Status: Written spec for user review; implementation has not started.

## Intent and success criteria

Implement the two designs discussed with the user for `nicknamer/`: exercise real production composition in tests and make meaningful application outcomes observable through explicit typed events. The clients are browser users and JSON API consumers. Operators need to identify startup failures, authentication failures, failed requests, and incomplete bulk work; maintainers need to distinguish committed changes from response failures.

Success means the same router construction runs in production and integration tests; issued credentials work through that router; event assertions accompany real persistence outcomes; instrumentation cannot change application results; and sensitive inputs cannot enter the application event stream. Preserve existing routes, response formats, authentication policy, persistence behavior, and bulk partial success.

Assumptions: operational observations are best effort, not a required durable audit trail. The initial destination is local structured logging through tracing. Deployment ownership, external database consumers, remote telemetry destinations, retention, and alert thresholds were not established. This implementation adds no remote exporter or monitoring service.

## Alternatives and selected approach

1. Selected: shared concrete router factory, concrete PostgreSQL persistence, a small typed observation boundary, and a local tracing listener. This covers production wiring and meaningful outcomes with limited infrastructure.
2. Repository and clock traits throughout the application would enable extensive substitution but lose persistence fidelity and introduce abstraction without an actual production alternative.
3. A durable event transport and remote metrics/traces stack would strengthen delivery and aggregation but requires operational requirements absent from this task.

## Production composition

Extract a public application-router factory accepting the existing shared authentication and name states plus shared observation context. It builds the web router, API router, and outer request observation layer. Startup and integration tests must both use it. Keep auth middleware inside its existing route boundaries and preserve API nesting and public routes.

Keep DatabaseConnection concrete. NameService continues borrowing it; observation dependencies must not shorten the connection lifetime or introduce a connection per call. Share the observer with Arc. Introduce no repository trait.

Startup retains its current ordering: load configuration, bind listener, connect database, run migrations, construct states/router, serve. Register observation listeners before these operations. Readiness means all initialization succeeded and the listener/router are ready to enter serving; it does not assert that a client has connected. Replace the premature running announcement with truthful stage events and readiness.

Retain health response semantics: `/health` returns OK and is not a database readiness probe. Add graceful signal handling to drain in-flight requests for at most 10 seconds, then terminate draining, emit the appropriate shutdown outcome, and flush the local listener. Do not claim completion for forcibly cancelled operations.

## Observation boundary and contracts

Introduce a local observations module with a typed Observation enum, immutable context, an application-facing ObservationSink, dispatcher, and listener contract. Keep concrete tracing calls in the listener and composition diagnostics, not spread across application decisions. Do not import prediction_bot's application-specific audit types.

Context contains an occurrence timestamp, generated opaque request ID where applicable, operation ID, optional parent operation ID, and optional trace correlation when available. Use semantic wrapper types for correlation IDs. Do not accept arbitrary client strings as trusted correlation identifiers. Measure elapsed duration with Instant and represent it as Duration.

| Event                  | Emission point and required facts                                                                                                                                                                                                |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| StartupStageFinished   | Configuration, binding, connection, migration, and composition finish or fail; typed stage, outcome, duration, optional failure category.                                                                                        |
| ApplicationReady       | After all startup prerequisites succeed, immediately before serving.                                                                                                                                                             |
| AuthenticationFinished | Credential attempt completes; web/API channel, accepted/rejected/failed outcome and bounded reason. Accepted means token/cookie response construction succeeded.                                                                 |
| AccessDenied           | Protected route rejects missing, invalid, or expired authentication; channel and bounded reason where known. Do not label a valid-cookie request as a new login.                                                                 |
| NameMutationFinished   | Create/update/delete persistence resolves; committed/rejected/failed outcome, operation kind, standalone/bulk origin, category. A successful autocommit write is committed even if subsequent response rendering fails.          |
| BulkOperationFinished  | Parsing rejection or completion of the existing loop; operation, attempted/succeeded/skipped/failed counts, optional input count, outcome, duration, bounded failure-category counts.                                            |
| NamesReadFinished      | Service query resolves; filter-present flag, result count if successful, outcome, duration, category.                                                                                                                            |
| ExportPrepared         | YAML serialization finishes or fails; entry count, prepared byte count on success, outcome, stage/category. Prepared does not mean downloaded.                                                                                   |
| RequestFinished        | Response is produced; bounded route/method, status, duration and correlation. A dropped request future produces an aborted observation where observable. This measures response preparation, not completed network transmission. |
| ShutdownFinished       | Drain completes or deadline expires; outcome and duration. Flush follows this event.                                                                                                                                             |

Enumerate failure categories from typed application errors, such as duplicate, missing entry, malformed input, database, serialization, template, token, configuration, bind, and internal. Never derive categories by parsing Display strings. Unknown provider errors map to a safe bounded category.

Bulk counts describe the parsed mapping/selected IDs as processed by the current implementation, not YAML source-line counts. Malformed YAML has unknown input count and zero attempts. Duplicate create entries are skipped; missing delete targets are failures. Empty operations are successful no-ops. Partial success means at least one item succeeded and at least one failed. Duplicate-only imports are successful with skips. Preserve HashMap iteration behavior and per-item autocommit; do not add transactionality or ordering promises.

Child mutation events have the batch operation as parent. A batch summary is not an additional committed mutation. Assign one producer to each fact and avoid logging the same failure again at every propagation layer. A committed mutation followed by a failed HTTP response legitimately produces two different facts.

## Privacy and signal mappings

Application events exclude credentials, cookies, JWTs, DB URLs, names, usernames, Discord/server IDs, raw paths, queries, request bodies, YAML, and raw error messages. Replace implicit instrument argument capture with explicit safe fields. Configure request tracing around the merged router without recording raw URI or headers. Review ORM/library logging so application configuration does not enable SQL parameter/body diagnostics.

The tracing listener maps successful operations and routine authentication rejection to INFO, partial bulk failure to WARN, and unexpected operational failures to ERROR. Routine successful health checks need no log record. Preserve generated request/operation correlation in structured fields. Test mappings before formatting.

Future metric mappings are documented but not implemented here: request/operation counters and durations in seconds, batch item totals, and authentication outcome counters. Dimensions are bounded route, method, operation, origin, channel, outcome, stage, and category. IDs never become labels. Do not combine child mutation counts with batch-summary item totals. Future complete outcome counters must consume unsampled events; tracing/log sampling must remain listener-specific.

Use the process's existing local log collection destination. No new destination, retention configuration, dashboards, or alert thresholds are introduced. Operators remain responsible for deployment access and retention configuration.

## Delivery, failure, and lifecycle

Dispatch synchronously to local listeners. The listener contract reports delivery errors; dispatcher failure must not change database operations, HTTP responses, retries, or exception propagation. Continue to remaining listeners after a reported failure. Contain listener unwinding where Rust panic semantics permit; process-abort failures cannot be contained. Do not introduce network calls, asynchronous queues, telemetry retries, or background workers in this implementation.

One in-process emission attempts each listener once. Events may be lost during crashes or sink failures; post-commit observation is not durable delivery. Listener failure reporting uses a minimal rate-limited direct diagnostic rather than recursively dispatching another event. Logging may still impose local writer latency; do not claim zero overhead or bounded blocking without measurement.

Before the dispatcher is available, initialization failures use sanitized stderr stage/category diagnostics. Once registered, report configuration and startup failures normally. Avoid printing raw propagated startup errors on process exit; preserve failure exit status while emitting a single sanitized failure record. If listener setup permanently fails, use the fallback through exit rather than start silently without the selected listener.

## Testability and verification design

Use googletest for new Rust tests and assertions. Keep real PostgreSQL, migrations, cryptography, templates, and serialization. Reuse the per-test database fixture. Shared container resource contention remains possible; measure test runtime rather than claiming fast or fully isolated execution from structure alone.

Before refactoring, run existing Nicknamer tests and establish a process-level baseline for login and protected access using the current binary. If environment limitations prevent a baseline, report the exact limitation before moving responsibilities; do not claim a demonstrated before/after regression check.

After extracting composition, exercise browser login -> returned cookie -> protected route and API login -> returned token -> authenticated mutation/read. Assert missing/invalid credentials cannot mutate state. Verify public health and route nesting. Existing narrow endpoint checks remain useful for presentation contracts; avoid duplicating every service case through every route.

Extract deterministic JWT claim construction using an explicit issuance timestamp, retaining one clock read per production issuance. Assert decoded identity and 24-hour lifetime with real signing. Test wrong-secret and clearly expired rejection. Do not change JWT validation defaults or implement a clock trait merely to test exact expiry boundaries.

Producer checks capture typed observations alongside outcomes: committed mutation, duplicate rejection, malformed bulk input before writes, mixed bulk results, and response failure after persistence. Use a local recording sink in test support. If a production response-rendering failure cannot be triggered with real adapters, verify the accessible error boundary separately and report the coverage gap; do not add a broad production template interface solely to force it.

Listener checks inspect structured records and severity, safe field sets, and listener-failure isolation. Never assert rendered diagnostic strings or JSON. Composition checks use the production factory and local listener to cover registration before first work, both route families, correlation, and shutdown. One process smoke check covers environment loading, binding, migrations, and serving. Do not treat handler checks as deployed delivery evidence.

After source edits run Gazelle immediately, then repository-wide formatting, focused tests, full build, and lint through the repository toolchain. Verify nonzero intended test execution counts. Check Bazel targets actually compile any newly added inline tests; the current library BUILD exposes a rust_library, not an inline-test target.

## Scope exclusions and approval state

No schema or product-policy changes; no database mock, broker, outbox, OpenTelemetry exporter, metrics backend, provider account, or raw diagnostic snapshots. Do not change bulk success HTTP behavior as part of instrumentation; observe the underlying partial result.

The conversational designs were approved for implementation. This consolidated written spec requires review under the explicitly invoked brainstorming workflow. After written-spec approval, create the implementation plan and obtain its review/execution-method selection before product edits.

## Checks performed for this spec

Inspected current application startup, routes, auth, services, fixtures, Bazel targets, instrumentation, and local deployment files. Confirmed the repository's existing typed listener precedent. Reviewed this spec for scope, contract consistency, placeholders, and delivery claims. No application tests, build, or lint were executed while writing this spec.
