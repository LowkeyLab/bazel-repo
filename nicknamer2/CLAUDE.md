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

- `DB_URL` — connection string env var (not `DATABASE_URL`); format: `postgres://user:pass@host:port/db` (docker-compose default: `nicknamer2:nicknamer2@localhost:5433/nicknamer2`)
- `STATIC_DIR` — optional path to serve frontend static files (e.g., built Angular output)
- `CASDOOR_CLIENT_ID` — required for auth validation; without it, mutations reject all requests
- Backend listens on port **8080** (not 3000)

## Patterns

- **Layered architecture**: `server.rs` (Axum) → `graphql/` (Juniper) → `{name,discord_server}/service.rs` → `{name,discord_server}/repo.rs` (sqlx) → PostgreSQL
- **Modules**: `name/` (nicknames) and `discord_server/` (servers) — both follow domain → repo → service layering. `server/` is the Axum HTTP server (not a domain module).
- **Trait-based repositories**: `NameCreator`, `NameReader`, `NameUpdater`, `NameDeleter`, `NameCounter`; `ServerCreator`, `ServerReader` — enables testability
- **DAO pattern**: `NameDAO`, `ServerDAO` (sqlx `FromRow`) convert to domain models via `From`
- **Relay Global IDs**: base64-encoded `Type:components` (e.g., `Name:{discord_id}:{discord_server}`)
- **Cursor pagination**: base64-encoded JSON cursors, keyset pagination ordered by `discord_id`
- **DI via GraphQL context**: `Context` holds `Arc<Service<Repo>>` (names), `Arc<ServerService<ServerRepo>>` (servers), `Arc<dyn AuthService>`, optional auth token
- **Pool sharing**: when multiple repos use the same `PgPool`, use `pool.clone()` (it's an `Arc` internally)
- **Unit tests**: in-source `#[cfg(test)]` modules
- **Migrations**: `sqlx::migrate!()` macro in `src/migrations/migrations.rs`, SQL files in `nicknamer2/migrations/`. Bazel `compile_data` with `# keep` comment required for the migrations filegroup.
- **Integration tests**: spin up PostgreSQL via `testcontainers`, tagged `requires-network`

## E2E Testing with Casdoor

Casdoor's React SPA doesn't render in headless Chromium on the `/login/oauth/authorize` page.
To get a JWT for E2E tests, use the password grant directly:

```bash
curl -s 'http://localhost:8000/api/login/oauth/access_token' \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  -d 'grant_type=password&client_id=<CLIENT_ID>&client_secret=<CLIENT_SECRET>&username=<USER>&password=<PASS>&scope=profile'
```

Requires `password` in the application's `grantTypes` list in Casdoor.
