# Prediction bot liveness endpoint

## Purpose and contract

Expose an HTTP liveness probe from the prediction bot process. `GET /healthz`
returns status `200 OK`, content type `text/plain; charset=utf-8`, and body `ok`.
The handler has no dependencies on PostgreSQL, Discord, or worker state. A
successful response establishes only that the process can serve the probe;
it does not establish readiness to handle market commands.

## Approach

Use the existing Axum and Tokio dependencies for a small HTTP server in the bot
process. This follows the repository's Rust HTTP stack. Handwritten HTTP adds
unnecessary protocol handling, and a separate process would not establish bot
liveness.

A dedicated health module owns the router and server lifecycle. The binary owns
configuration and runs the server alongside normal initialization and the bot.
Keep economic operations, persistence, and Discord interaction handlers unchanged.

## Configuration

`HEALTH_BIND_ADDRESS` is an optional numeric IP address and port, defaulting to
`0.0.0.0:8080`. Reject invalid, empty, or non-Unicode values with a configuration
error. Operators can override the address for local-only access or port conflicts.
No authentication is required; the response contains no operational details.

## Startup, supervision, and shutdown

Parse command-line mode first. Migration mode runs without binding or validating
the health listener. In normal mode, validate configuration and bind the listener
before starting external Discord and database initialization. Serve probes while
that initialization is pending and throughout normal bot operation.

A bind failure prevents bot startup. An unexpected server exit makes the process
exit unsuccessfully so the bot cannot continue running without its health endpoint.
Bot initialization failure or bot termination stops the health server. Preserve
the existing gateway and worker cleanup on Ctrl-C and SIGTERM; health server
shutdown must be bounded and must not leave a detached listener task. During
startup, process termination must also close the listener.

The handler performs no network calls and emits no per-probe logs. Configuration,
bind, and server failures must surface with useful context without exposing
credentials. Existing readiness audit events retain their current meaning.

## Files and validation

Add a focused health module and tests under `prediction_bot`, wire it into the
binary, and update Bazel dependencies after running Gazelle. Document the probe
contract, default bind address, and an example request in `prediction_bot/README.md`
and add the setting to `.env.example`.

Use googletest assertions and the repository's Bazel test workflow. Verify the HTTP
status, body, and content type; confirm probes work while bot initialization is
pending; verify configuration defaults and invalid input; and cover bind failure,
bot completion/failure cleanup, and unexpected server completion. Exercise the
actual local listener where needed to establish lifecycle behavior without live
Discord or PostgreSQL dependencies. Verify migration mode opens no listener.

After source edits, run `bazel run //:gazelle` immediately. Run
`aspect format --scope=all`, relevant prediction bot tests, `aspect build //...`,
and `aspect lint` before completing implementation.

## Testability and production composition

The client is an HTTP probe caller. Its public observations are the response,
listener availability, and process termination. Keep the router and Axum server
real; the handler is deterministic and needs no injected collaborator.

Read `HEALTH_BIND_ADDRESS` once in normal-mode configuration, then pass its
explicit value to parsing and binding. Parsing tests supply absent, invalid,
empty, and non-Unicode values directly rather than mutating the test process's
shared environment. Production must use the same parser at the same startup
point. Migration mode branches before this read.

Pass a concrete bound Tokio `TcpListener` to the server. Tests bind
`127.0.0.1:0` and use its assigned address, avoiding shared ports and the race
between choosing a free port and binding it. Keep the real router, socket, and
HTTP adapter together in listener tests. No listener interface or health-service
mock is needed. A held local listener provides a real port-conflict test.

The lifecycle boundary accepts the bot operation as a future, including external
initialization. Production supplies the real initialization followed by
`discord::run`; lifecycle tests supply a future whose completion they control.
That substitution controls bot progress while retaining the actual health
server and supervision logic. Use explicit synchronization to hold initialization
pending, issue a real HTTP request, then release success or failure and observe
listener closure and the returned outcome. Avoid sleep-based coordination and
assertions on internal helper calls.

Keep any seam for unexpected server completion private to the supervisor and use
it only to supply a completing or failing server future. Such a check proves the
supervisor's reaction, not that Axum produces that failure. Pair it with the real
listener tests. Do not introduce public debug controls, test-mode branches, or
interfaces for deterministic helpers.

Preserve the current Discord-owned signal handling, worker shutdown deadline,
and gateway-lock release. An outer supervisor must not race an independent signal
handler against `discord::run` and drop that future before its cleanup completes.
Tests of the new supervisor with a substituted bot future do not establish worker
cleanup fidelity; retain the existing Discord lifecycle tests as regression
coverage before moving any existing responsibilities. No baseline has been run
at the design stage.

Cover normal composition separately: launch the built binary with child-local
environment values and an occupied health address, and observe the bind failure
before external initialization. Launch migration mode with that address and an
invalid health setting and observe migration validation instead of health
configuration or binding. These checks exercise real mode selection, environment
acquisition, parsing, and binding. They do not establish successful full Discord
startup; live provider readiness remains outside the liveness contract.

Discord is an unmanaged external boundary. Health tests need no Discord calls.
PostgreSQL is also absent from the health request path; preserve existing store
integration coverage using the real isolated database. This design does not
assume exclusive schema ownership or change database contracts. Lifecycle tests
using a controlled bot future establish health behavior independently of those
dependencies, not full bot integration.

All checks in this section are proposed. At this stage, repository inspection
supports the seam choices; endpoint behavior, shutdown behavior, composition,
and test runtime remain unverified until implementation and execution.
