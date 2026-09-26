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

## Local operational observations

The server registers one synchronous local tracing listener before configuration loading.
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
was prepared. Explicit shutdown cancellation records aborted status `503` for the
shutdown rejection response; it does not claim the handler completed.

SIGINT/SIGTERM stops acceptance and allows up to ten seconds for graceful draining.
At the deadline the observation middleware cancels remaining handler futures, waits for
their drop observations, emits `shutdown_finished` with `timed_out`, then flushes listeners.
Cancelled work is not reported as completed. Cancellation is cooperative: synchronous
handler work or a blocking local writer can delay runtime scheduling and final cleanup.
The deadline bounds the graceful-drain phase, not arbitrary synchronous blocking.

Delivery is best effort and in-process: each event attempts each listener once, and a
failing listener does not change application responses or persistence outcomes. Listener
failures use a rate-limited, sanitized stderr fallback independent of tracing. Local
logging has writer latency and no SDK queue to flush; flush does not promise OS or log
collector persistence. Crashes, process aborts, and sink failures can lose records.
No remote exporter, OpenTelemetry backend, durable audit trail, retention policy, or
monitoring service is configured. Tests and the disposable-PostgreSQL process smoke
exercise local composition; hosted collection and retention require deployment evidence.
