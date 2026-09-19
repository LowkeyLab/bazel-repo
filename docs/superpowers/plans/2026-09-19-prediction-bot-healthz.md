# Prediction Bot Healthz Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Serve a dependency-free HTTP liveness endpoint from prediction_bot.

**Architecture:** A private binary health module parses an explicit environment value and runs an Axum server alongside the existing bot future. The binary selects migration mode before health configuration and supplies the real bot initialization future. Discord retains signal and worker cleanup ownership.

**Tech Stack:** Rust, Tokio, Axum, googletest, Bazel/Aspect.

**Spec:** `docs/superpowers/specs/2026-09-19-prediction-bot-healthz-design.md`

## Global Constraints

- `GET /healthz` returns `200 OK`, `text/plain; charset=utf-8`, and `ok`.
- `HEALTH_BIND_ADDRESS` defaults to `0.0.0.0:8080`.
- No database or Discord calls in the probe handler.
- Migration mode opens no health listener and ignores health configuration.
- Preserve Discord signal handling, worker cleanup, and gateway-lock release.
- Run `bazel run //:gazelle` immediately after every source edit, before formatting.
- Use only Bazel/Aspect for builds and tests, through `nix develop --command`.

## Task 1: Health configuration, HTTP server, and supervision

**Files:** Create `prediction_bot/bin/src/health.rs` and `prediction_bot/bin/src/health_tests.rs`; modify `prediction_bot/bin/BUILD.bazel` after Gazelle.

**Interfaces:** `address(value: Result<String, std::env::VarError>) -> anyhow::Result<std::net::SocketAddr>`; `run(listener: tokio::net::TcpListener, bot: impl Future<Output = anyhow::Result<()>>) -> anyhow::Result<()>`. Private supervisor takes bot and server futures; the production server receives a shutdown receiver.

- [x] Add a standalone `health_test` Rust test target rooted at `src/health.rs`, with Axum, Tokio, anyhow, googletest, and reqwest dependencies. Tests live in the sibling test module.
- [x] Write failing configuration tests with explicit values:

```rust
assert_that!(address(Err(std::env::VarError::NotPresent)).unwrap().to_string(), eq("0.0.0.0:8080"));
assert_that!(address(Ok(String::new())).is_err(), eq(true));
assert_that!(address(Ok("127.0.0.1:8081".into())).unwrap().to_string(), eq("127.0.0.1:8081"));
```

Also cover malformed addresses and non-Unicode input. Add real HTTP tests: bind `127.0.0.1:0`, spawn `run` with a oneshot-controlled bot future, request `/healthz`, assert status/content-type/body, complete the bot, await the task, then assert a new connection fails. Repeat with bot failure and assert its error is preserved. Use timeouts only as test watchdogs.

- [x] Run Gazelle then `aspect test //prediction_bot/bin:health_test`; record expected failure for missing behavior.
- [x] Implement parsing by matching `VarError::NotPresent` to the default; reject every other environment error and parse using `SocketAddr`. Implement the route as:

```rust
axum::Router::new().route("/healthz", axum::routing::get(|| async { "ok" }))
```

Run the real server with graceful shutdown. Select between the bot and server futures. Bot completion signals server shutdown, drains it with a five-second limit, and returns the bot outcome. Unexpected server completion returns an error. Keep both futures owned by the supervisor so none can survive its return.

- [x] Add supervisor tests with controlled server completion and failure; assert error return. Pair these with the real socket tests, which establish adapter fidelity.
- [x] Run Gazelle immediately after source writes, then format and rerun the health tests.
- [x] Include this task in the final feature commit with binary wiring (see execution note below).

## Task 2: Binary composition and operational documentation

**Files:** Modify `prediction_bot/bin/src/main.rs`, `prediction_bot/bin/BUILD.bazel`, `prediction_bot/README.md`, and `prediction_bot/.env.example`; create `prediction_bot/bin/tests/composition_test.rs`.

**Interfaces:** Consume Task 1's `address` and `run`. Extract the existing normal-mode initialization and `discord::run` into `async fn run_bot(audit: SharedAudit, token: String, url: String, defaults: Policy) -> anyhow::Result<()>`, preserving its ordering and audit events. Read and validate the existing bot configuration before binding; pass those owned values into `run_bot`.

- [x] Add a binary composition test target with the binary in runfiles. Launch it with child-local environment and a held TCP address; set required bot configuration to dummy valid values and assert a bind error before external initialization. Launch `--migrate` with invalid health configuration and missing migration configuration; assert migration validation is reached. Resolve the binary through Bazel runfiles rather than an ambient checkout path.
- [x] Run Gazelle and the composition test; verify the expected missing-binding failure before implementation.
- [x] Wire normal-mode execution after the existing migration branch:

```rust
let address = configured(&audit, LifecycleKind::Startup, health::address(env::var("HEALTH_BIND_ADDRESS")))?;
let listener = tokio::net::TcpListener::bind(address).await
    .map_err(|error| anyhow!("health listener bind failed: {error}"))?;
health::run(listener, Box::pin(run_bot(audit, token, url, defaults))).await
```

Keep migration mode's early return. Preserve all existing bot initialization behavior inside `run_bot`. No new independent signal handler is added.

- [x] Run Gazelle, update binary source/dependency declarations if needed, and rerun health, composition, and existing Discord tests.
- [x] Document `HEALTH_BIND_ADDRESS=0.0.0.0:8080`, `curl http://localhost:8080/healthz`, migration behavior, and the distinction between process liveness and dependency readiness. Containers need an explicitly published port to probe from the host.
- [x] Run `aspect format --scope=all`, `aspect test //prediction_bot/...`, `aspect build //...`, and `aspect lint`. Investigate failures and report environmental limits accurately.
- [x] Review the final diff for scope, shutdown lifetime, secret exposure, and production/test wiring; commit with `feat(prediction-bot): expose liveness endpoint from bot process`.

## Verification evidence

The initial `aspect test //prediction_bot/lib:discord_test` baseline passed. Health
contract tests then failed for missing configuration, listener behavior, and fatal
server supervision; after implementation they passed. The binary composition test
failed because startup reached Discord instead of rejecting an occupied health
port; after wiring it passed. Migration bypass passed before and after wiring.
Additional drain failure and timeout coverage passed in the complete seven-target prediction bot suite.
Independent review found no actionable correctness issues. The full monorepo build passed. Initial lint identified a large bot future; boxing that future resolves its stack footprint. The affected tests and full monorepo build passed again after that adjustment; final `aspect lint` reported no findings. `aspect format --scope=all` passed. The initial lint run also emitted unrelated website ESLint formatter messages about a missing `chalk` module; no website files were changed.

Implementation tasks are being committed together so the new module and binary
wiring arrive as one working feature.

Final checks: seven prediction bot test targets passed (existing library targets were cached); both affected binary test targets passed after the lint adjustment; `aspect build //...` passed; `aspect lint` passed with no findings; independent review reported no actionable issues.
