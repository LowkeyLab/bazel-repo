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
