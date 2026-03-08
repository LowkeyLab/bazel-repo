# nicknamer2

## Commands

```bash
# Local dev environment (PostgreSQL on :5433, Casdoor on :8000)
docker-compose -f nicknamer2/docker-compose.yml up -d

# Build
aspect build //nicknamer2/...

# Test (integration tests use testcontainers — need Docker)
aspect test //nicknamer2/...

# Run server
bazel run //nicknamer2/src/bin:nicknamer2

# Single test
aspect test //nicknamer2/src/name:name_repo_test --test_filter="test_name"
```

## Environment

- `DB_URL` — connection string env var (not `DATABASE_URL`); format: `postgres://user:pass@host:port/db`
- `STATIC_DIR` — optional path to serve frontend static files (e.g., built Angular output)
- `CASDOOR_CLIENT_ID` — required for auth validation; without it, mutations reject all requests

## Patterns

- **Layered architecture**: `server.rs` (Axum) → `graphql/` (Juniper) → `name/service.rs` → `name/repo.rs` (sqlx) → PostgreSQL
- **Trait-based repositories**: `NameCreator`, `NameReader`, `NameUpdater`, `NameDeleter`, `NameCounter` — enables testability
- **DAO pattern**: `NameDAO` (sqlx `FromRow`) converts to domain `Name` via `From`
- **Relay Global IDs**: base64-encoded `Type:components` (e.g., `Name:{discord_id}:{discord_server}`)
- **Cursor pagination**: base64-encoded JSON cursors, keyset pagination ordered by `discord_id`
- **DI via GraphQL context**: `Context` holds `Arc<Service<Repo>>`, `Arc<dyn AuthService>`, optional auth token
- **Unit tests**: in-source `#[cfg(test)]` modules
- **Integration tests**: spin up PostgreSQL via `testcontainers`, tagged `requires-network`
