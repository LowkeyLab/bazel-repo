# Task 4: JSON audit composition and repository verification

## Result

Enabled `tracing-subscriber`'s existing JSON feature and configured the production subscriber for newline-delimited JSON at INFO and above. The executable constructs one logging listener, passes the same `Arc` to `Store::connect_with_audit`, and uses it for sanitized migration and reconstructed-startup lifecycle outcomes. Discord readiness and shutdown continue through the store listener, so no second listener or duplicate lifecycle path was introduced.

Fatal setup behavior remains visible through the executable's returned `anyhow::Error`. Audit events receive only typed categories and allowlisted SQLSTATE/Discord status fields; raw environment values, SQLx/Serenity errors, tokens, URLs, and payload content never cross the listener boundary. `required` now converts non-Unicode environment failures to a fixed safe diagnostic instead of retaining raw environment bytes in an error source chain.

## Dependency generation

- `Cargo.toml` retains requirement `0.3.20` and enables only `features = ["json"]`.
- `nix develop --command bazel run //tools:cargo_lock` passed. `Cargo.lock` added only `tracing-serde 0.2.0` plus the expected `serde`, `serde_json`, and `tracing-serde` edges on the already resolved `tracing-subscriber 0.3.23`; no package versions changed.
- Gazelle added only the matching `tracing-serde 0.2.0` crate metadata to `MODULE.bazel.lock`. The binary BUILD target already declared its direct tracing dependencies.

## Composition and mapping review

- The subscriber calls `.json().with_max_level(tracing::Level::INFO).init()` before argument/configuration processing, then constructs `logging_listener()`.
- Normal startup uses `Store::connect_with_audit(..., Arc::clone(&audit))`; `discord::run` therefore observes the same listener through `Store::audit`.
- Migration and store/bootstrap failures emit safe `Lifecycle` outcomes before returning the existing fixed fatal result. Migration and reconstructed startup successes replace the two original free-form application tracing calls.
- The listener contract remains synchronous and best effort. No queue, mutable global listener, remote output, metrics exporter, trace exporter, or rendered diagnostic-output test was added.

Direct review of the exhaustive `LoggingListener` match confirmed:

- command success/rejection is INFO; exhausted operational failures are ERROR, with the store emitting one final completion after bounded retries;
- query success is INFO, database/history failure is ERROR, and bounded timeout is WARN;
- acknowledgement and delivery failures are WARN and remain separate from committed command outcomes;
- grant discovery/reconstruction, registration, and fatal lifecycle failures are ERROR;
- readiness, migration/startup success, normal shutdown, and expected rejection are INFO.

`rg` found no remaining application tracing calls outside `audit/logging.rs`. Producers construct typed fields only. Store retry/commit/receipt recovery, private reply behavior, grant continuation, and shutdown flow are unchanged; worker/handler emissions represent distinct boundaries rather than duplicate final command outcomes.

## Verification

- Focused tests: `nix develop --command aspect test //prediction_bot/lib:domain_test //prediction_bot/lib:events_test //prediction_bot/lib:discord_test //prediction_bot/lib:store_test` passed in 50.9 seconds.
- PostgreSQL evidence: `store_test` executed fresh against its isolated PostgreSQL test environment and passed in 37.1 seconds. Domain, events, and Discord targets passed from cache in this final invocation.
- Focused binary build: `nix develop --command aspect build //prediction_bot/bin:bin` passed in 24.3 seconds.
- Formatting: `nix develop --command aspect format --scope=all` passed in 9.3 seconds and formatted only `prediction_bot/bin/src/main.rs`.
- Post-format Gazelle: `nix develop --command bazel run //:gazelle` passed.
- Full build: `nix develop --command aspect build //...` passed in 1 minute 12 seconds (17,013 actions, 8,167 processes reported by Bazel).
- `git diff --check` passed. No `.orig` or `.rej` files remain.

Command output is retained in the `task-4-cargo-lock.log`, `task-4-bin-build.log`, `task-4-focused-tests.log`, `task-4-format.log`, `task-4-gazelle-final.log`, and `task-4-full-build.log` artifacts.

## Regression and test-quality evidence

Task 2 first established the listener seam with a compile-red fixture; its attempted runtime-red verdict was not captured and remains explicitly unverified. Its corrected integration run subsequently demonstrated 17 store tests against PostgreSQL, including rejection/balance preservation, append rollback, replay classification, grant continuation, canonical correlation, and one final command outcome.

Task 3 recorded a runtime RED before emissions: existing economic assertions passed while the new query/delivery/acknowledgement event assertions failed. GREEN then covered 20 PostgreSQL tests and 49 Discord tests. These tests assert balances, receipts, idempotent redelivery, safe private responses, typed outcomes, and absence of duplicate semantic outcomes rather than internal helper calls or rendered logs. Private recorders and transport fixtures are small and local to their tests; zero-duration pending futures cover timeout behavior without sleeps.

Task 4 added no tests because the approved design explicitly prohibits captured-writer, JSON parse, severity snapshot, and rendered-field assertions. Production subscriber configuration and exhaustive severity/field mapping were reviewed directly instead.

## Remaining verification limits

- Serenity adapters were exercised in Task 3 against a controlled local HTTP endpoint, including status/code-only failure classification, but no live Discord API or gateway connection was made.
- Gateway signal handling, shutdown timeouts, and production process termination were not integration-tested.
- JSON rendering, external log collection, retention, and alert delivery were not tested or configured, by design.
- The final focused invocation ran only `store_test` fresh; cached domain/events/Discord results are reported as cached rather than claimed as fresh executions.
