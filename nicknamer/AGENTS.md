# Nicknamer server: AGENTS.md

A web service for managing names with authentication, built using Axum, SeaORM, and PostgreSQL.

## Setup commands

- Start local development with dependencies (PostgreSQL + server):

  ```bash
  bazel run //nicknamer:run_locally
  ```

- Build the server:

  ```bash
  aspect build //nicknamer/server/bin
  ```

- Run tests:

  ```bash
  aspect test //nicknamer/server/lib/tests:tests
  ```

- Build Docker image:

  ```bash
  aspect build //nicknamer/server/bin:image
  ```

## Build system

This subproject uses Bazel with the workspace-managed toolchains.

Key targets:

- `//nicknamer:run_locally`: local dev (DB + server)
- `//nicknamer/server/bin`: server binary
- `//nicknamer/server/bin:image`: container image

## Testing Instructions

- Run all tests:

  ```bash
  aspect test //nicknamer/server/lib/tests:tests
  ```

- Run a subset of tests:

  ```bash
  aspect test //nicknamer/server/lib/tests:tests --test_filter="test_name_pattern"
  ```

For test libraries and Rust rules, read [the Rust standards](../docs/agents/rust.md).
Use [the development workflow](../docs/agents/development.md) for the required verification sequence.

## Architecture patterns

- Web layer: Axum for HTTP handling
- Database: SeaORM for type-safe database interactions
- Templates: askama for HTML templating
- Authentication: JWT tokens with bearer auth
- API Documentation: utoipa for OpenAPI specs

## Dependencies

- Keep dependencies minimal and well-justified
- Use workspace dependencies from `MODULE.bazel`

## Database

- Database: PostgreSQL
- ORM: SeaORM with migrations
- Migrations: `nicknamer/migration/`
- Local development uses Docker Compose (`nicknamer/compose.yaml`)

Migration commands:

```bash
# Run migrations (run_locally does this automatically)
bazel run //nicknamer/migration/bin -- up

# Create a new migration (via sea-orm-cli)
sea-orm-cli migrate generate <migration_name>
```

## Security considerations

- Provide JWT secrets via environment variables
- Never hardcode database credentials
- Admin credentials are configured via env vars
- Use HTTPS in production
- Validate all user inputs

Required environment variables (local dev):

- `DB_URL`: PostgreSQL connection string
- `ADMIN_USERNAME`: administrator user
- `ADMIN_PASSWORD`: administrator password
- `JWT_SECRET`: secret for JWT token signing

## Docker and deployment

- Base image: `gcr.io/distroless/cc-debian12`
- Build image:

  ```bash
  aspect build //nicknamer/server/bin:image
  ```

- Push to registry:

  ```bash
  bazel run //nicknamer/server/bin:push_image
  ```

- Local run (DB + server):

  ```bash
  bazel run //nicknamer:run_locally
  ```

The app serves on port 8080 by default.

## Troubleshooting

- Docker issues: ensure Docker daemon is running and accessible
- Database connection: inspect `docker compose ps` in `nicknamer/`
- Build failures: run `bazel clean` and retry
- Port conflicts: ensure ports 8080 (app) and 5432 (DB) are available
