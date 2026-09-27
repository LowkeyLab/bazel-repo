# Nicknamer Server

This directory contains the nicknamer server application.

## Building and Running

### Local Development

To run the server locally with dependencies:

```bash
bazel run //nicknamer:run_locally
```

### Docker Image

#### Building the Docker Image

To build the nicknamer server as a Docker image:

```bash
# Build the image (convenience alias)
aspect build //nicknamer:build_image

# Or using the direct target
aspect build //nicknamer/server/bin:image

# Or using the full target name
aspect build //nicknamer/server/bin:nicknamer_image
```

#### Pushing to GitHub Container Registry

To push the image to GitHub's Container Registry (ghcr.io):

```bash
# Make sure you're logged in to ghcr.io
docker login ghcr.io

# Push the image (convenience alias)
bazel run //nicknamer:push_image

# Or using the direct target
bazel run //nicknamer/server/bin:push_image
```

**Note**: You'll need to have proper authentication set up for GitHub Container Registry. Make sure you have:

1. A GitHub Personal Access Token with `write:packages` permission
2. Docker logged in to ghcr.io: `echo $GITHUB_TOKEN | docker login ghcr.io -u <username> --password-stdin`

#### Using the Built Image

After building, you can load the image into your local Docker daemon:

```bash
# Build and load the image
bazel run //nicknamer/server/bin:nicknamer_image
docker run --rm -p 8080:8080 nicknamer-server:latest
```

## Project Structure

- `server/bin/` - Main server binary
- `server/lib/` - Server library code
- `migration/` - Database migration utilities
- `compose.yaml` - Docker Compose configuration for local development

## Operational observations and OTLP stdout logs

The server registers its existing structured-event listener and an OpenTelemetry tracing
bridge before configuration loading. The SDK batches records on a dedicated thread; a
small stdout exporter uses `opentelemetry-proto` conversions and Serde support to write
one compact OTLP JSON `LogsData` object per batch,
using the [OTLP JSON encoding](https://opentelemetry.io/docs/specs/otlp/#json-protobuf-encoding).
`OTEL_SERVICE_NAME` sets the resource's `service.name` (default: `nicknamer`).
`RUST_LOG` controls tracing filtering (default: `info`); an invalid filter produces a
plain stdout diagnostic and falls back to `info`. No network endpoint is needed.
The pipeline uses `opentelemetry-appender-tracing`, `opentelemetry_sdk`, and
`opentelemetry-proto`. No network exporter is installed. Resource identity is configured
explicitly; other `OTEL_*` settings are not interpreted.
Records use the bridge's standard mapping for severity, instrumentation scope, and
timestamps. Application `event` and `occurred_at` fields remain ordinary attributes,
alongside request and operation IDs; no OpenTelemetry trace or span IDs are fabricated.
Infrastructure diagnostics also go to stdout and may be plain text, so consumers must
handle a mixed-format stream rather than assume every line is OTLP JSON.
Startup reports configuration, binding, database connection, migration, and composition
outcomes; `application_ready` follows successful initialization immediately before serving.
Binding still precedes database connection. Startup failures return a failure exit status
with a bounded stage/category diagnostic rather than printing raw configuration or errors.

Application records use individually typed structured fields for bounded route, method,
operation, outcome, counts, durations, and generated correlation identifiers. They omit
credentials, cookies, tokens, DB URLs, usernames, names, Discord/server IDs, raw paths,
queries, bodies, SQL statements/parameters, and raw error messages. Successful health
requests are omitted by the logging listener. `/health` still returns `OK`; it is not a
database readiness probe. Request completion measures response preparation, not delivery
to the client. Externally dropped requests use aborted status `0` because no response
was prepared. Shutdown cancellation records aborted status `503` when middleware prepares a rejection
response, or `0` when the connection drops the request first. Neither claims handler
completion or delivery to the client.

SIGINT/SIGTERM stops acceptance and allows up to ten seconds for graceful draining.
At the deadline the server cancels remaining handlers and accepted connection IO,
including stalled response writes. It stops connection processing and waits for tracked
request cleanup and drop observations before emitting `shutdown_finished` with `timed_out` and
flushing listeners. Requests reaching observation middleware after cancellation emit
one aborted `503` observation; tasks dropped before entering it produce no request event.
Cancelled work is not reported as completed. Cancellation is cooperative: synchronous
handler work or a blocking local writer can delay runtime scheduling and final cleanup.
The deadline bounds the graceful-drain phase, not arbitrary synchronous blocking.
After application cleanup (also on startup failure), SDK shutdown drains pending records
and waits at most two seconds for the exporter worker. No separate unbounded flush is
performed. A stalled stdout write may outlive this wait on the detached SDK thread;
remaining records can be lost on process exit. Telemetry failure does not change the
application exit status.

Delivery is best effort and in-process: each event attempts each listener once, and a
failing listener does not change application responses or persistence outcomes. Listener
failures use a rate-limited, sanitized stdout fallback independent of tracing. The SDK
queue holds at most 2,048 records, exports batches of at most 512 records, and schedules
export every second. Queue overflow drops records. SDK internal logging is disabled to
avoid recursively exporting its own errors. The exporter reports its first failure with
a fixed stdout diagnostic; diagnostics are skipped while the telemetry writer is busy.
Normal request threads enqueue records instead of waiting for stdout writes. Successful
SDK shutdown does not promise OS or collector persistence. Crashes, process aborts,
queue overflow, and sink failures can lose records.
No remote exporter, OpenTelemetry backend, durable audit trail, retention policy, or
monitoring service is configured. Existing tests cover structured application facts and
lifecycle behavior; stdout delivery and serialization are not asserted as external
contracts. Hosted collection and retention require deployment evidence.
