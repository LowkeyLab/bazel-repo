# Add Server Feature Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Discord servers first-class entities with a dedicated database table, domain model, GraphQL mutation, and Angular form.

**Architecture:** New `discord_server/` backend module (domain, repo, service) parallel to existing `name/` module. New migration creates `servers` table. GraphQL `Server` type gains `displayName` field and `createServer` mutation. Frontend gets `AddServerComponent` with form UI.

> **Note:** The spec says `server/` module but we use `discord_server/` to avoid collision with the existing Axum HTTP `server/` module at `nicknamer2/src/server/`.

**Tech Stack:** Rust (sqlx, Juniper, Axum), Angular (signals, Apollo, DaisyUI), Bazel

---

## File Structure

### Backend — New Files

- `nicknamer2/src/discord_server/server.rs` — Server domain model (reuses `DiscordServerId` from `name`)
- `nicknamer2/src/discord_server/repo.rs` — Repository traits (`ServerCreator`, `ServerReader`) + `Repo` impl
- `nicknamer2/src/discord_server/service.rs` — Service layer for server operations
- `nicknamer2/src/discord_server/BUILD.bazel` — Bazel targets for the new module
- `nicknamer2/src/migrations/004_create_servers_table.sql` — Migration SQL

### Backend — Modified Files

- `nicknamer2/src/migrations/migrations.rs:3,6-8` — Add migration 004
- `nicknamer2/src/migrations/BUILD.bazel:6-8` — Add SQL to compile_data
- `nicknamer2/src/graphql/context.rs:5,9-10` — Add `server_service` to Context
- `nicknamer2/src/graphql/model.rs:16-18,95-106` — Add `display_name` to Server struct, add `displayName` field, add `From` impl
- `nicknamer2/src/graphql/mutation.rs` — Add `createServer` mutation
- `nicknamer2/src/graphql/query.rs:14-79,82-153` — Update `server`, `node`, and `servers` queries to use server_service
- `nicknamer2/src/graphql/BUILD.bazel` — Add discord_server deps to relevant targets
- `nicknamer2/src/graphql/tests.rs:48-54,84-90` — Update test helpers for new `create_router` signature
- `nicknamer2/src/server/server.rs:22-23,32-36,43-44,55-56` — Thread server_service through handler + router
- `nicknamer2/src/server/BUILD.bazel` — Add discord_server dep
- `nicknamer2/src/bin/main.rs:27-28,55-58` — Create server_service, pass to router
- `nicknamer2/src/bin/BUILD.bazel:9-17` — Add discord_server deps

### Frontend — New Files

- `angular/projects/nicknamer2-web/src/app/graphql/create-server.graphql` — Mutation definition
- `angular/projects/nicknamer2-web/src/app/servers/add-server.component.ts` — Form component

### Frontend — Modified Files

- `angular/projects/nicknamer2-web/src/generated/graphql.ts` — Add CreateServer types + GQL service (hand-edited to match codegen patterns; run codegen after backend is deployed to verify)
- `angular/projects/nicknamer2-web/src/app/graphql/get-servers.graphql` — Add `displayName` to query
- `angular/projects/nicknamer2-web/src/app/servers/server-list.component.ts` — Add "Add Server" button, show display name
- `angular/projects/nicknamer2-web/src/app/servers/server-list.component.spec.ts` — Add `displayName` to mocked data, test "Add Server" button
- `angular/projects/nicknamer2-web/src/app/app.routes.ts` — Add `/servers/new` route

> **Note:** The dashboard (`get-dashboard.graphql`, `dashboard.component.ts`) intentionally does NOT include `displayName` — it shows server IDs only, keeping it lightweight. This can be added later if needed.

---

## Chunk 1: Database & Domain Model

### Task 1: Create migration SQL

**Files:**

- Create: `nicknamer2/src/migrations/004_create_servers_table.sql`

- [ ] **Step 1: Write the migration SQL**

```sql
CREATE TABLE IF NOT EXISTS servers (
    id UUID PRIMARY KEY,
    discord_server BIGINT NOT NULL UNIQUE,
    display_name VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
```

- [ ] **Step 2: Register migration in runner**

Modify `nicknamer2/src/migrations/migrations.rs`:

```rust
use sqlx::PgPool;

const MIGRATION_003: &str = include_str!("003_drop_users_recreate_names.sql");
const MIGRATION_004: &str = include_str!("004_create_servers_table.sql");

/// Runs all migrations for the nicknamer2 database.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(MIGRATION_003).execute(pool).await?;
    sqlx::raw_sql(MIGRATION_004).execute(pool).await?;
    Ok(())
}
```

Keep the existing `#[cfg(test)] mod tests` block unchanged.

- [ ] **Step 3: Add SQL file to BUILD.bazel compile_data**

Modify `nicknamer2/src/migrations/BUILD.bazel` — add `"004_create_servers_table.sql"` to `compile_data`:

```starlark
rust_library(
    name = "migrations",
    srcs = ["migrations.rs"],
    compile_data = [
        "003_drop_users_recreate_names.sql",
        "004_create_servers_table.sql",
    ],
    visibility = ["//nicknamer2:__subpackages__"],
    deps = ["@crates//:sqlx"],
)
```

- [ ] **Step 4: Verify migration test still passes**

Run: `aspect test //nicknamer2/src/migrations:migrations_test`
Expected: PASS (existing test checks names table; new migration is additive)

- [ ] **Step 5: Commit**

```bash
git add nicknamer2/src/migrations/
git commit -m "feat(nicknamer2): add servers table migration"
```

### Task 2: Create Server domain model

**Files:**

- Create: `nicknamer2/src/discord_server/server.rs`
- Create: `nicknamer2/src/discord_server/BUILD.bazel`

- [ ] **Step 1: Write the domain model**

Create `nicknamer2/src/discord_server/server.rs`:

```rust
use chrono::prelude::*;
use name::DiscordServerId;

/// A Discord server registered in the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub id: DiscordServerId,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Server {
    /// Creates a new Server instance.
    pub fn new(id: DiscordServerId, display_name: String) -> Self {
        let now = Utc::now();
        Server {
            id,
            display_name,
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let id = DiscordServerId(123456789);
        let display_name = "My Server".to_string();
        let server = Server::new(id, display_name.clone());

        assert_eq!(server.id, id);
        assert_eq!(server.display_name, display_name);
    }
}
```

- [ ] **Step 2: Write BUILD.bazel**

Create `nicknamer2/src/discord_server/BUILD.bazel`:

```starlark
load("@rules_rust//rust:defs.bzl", "rust_library", "rust_test")

rust_library(
    name = "discord_server",
    srcs = ["server.rs"],
    visibility = ["//nicknamer2:__subpackages__"],
    deps = [
        "//nicknamer2/src/name",
        "@crates//:chrono",
    ],
)

rust_test(
    name = "discord_server_test",
    size = "small",
    crate = ":discord_server",
)
```

- [ ] **Step 3: Run test**

Run: `aspect test //nicknamer2/src/discord_server:discord_server_test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add nicknamer2/src/discord_server/
git commit -m "feat(nicknamer2): add Server domain model"
```

### Task 3: Create Server repository

**Files:**

- Create: `nicknamer2/src/discord_server/repo.rs`
- Modify: `nicknamer2/src/discord_server/BUILD.bazel`

- [ ] **Step 1: Write the repository traits and implementation**

Create `nicknamer2/src/discord_server/repo.rs`:

```rust
use discord_server::Server;
use name::DiscordServerId;
use sqlx::PgPool;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use std::future::Future;

/// Data Access Object for servers table mapping
#[derive(Debug, sqlx::FromRow)]
struct ServerDAO {
    #[allow(dead_code)]
    id: Uuid,
    discord_server: i64,
    display_name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<ServerDAO> for Server {
    fn from(dao: ServerDAO) -> Self {
        Server {
            id: DiscordServerId(dao.discord_server as u64),
            display_name: dao.display_name,
            created_at: dao.created_at,
            updated_at: dao.updated_at,
        }
    }
}

/// Creates servers in the database.
pub trait ServerCreator {
    fn save(&self, server: Server) -> impl Future<Output = anyhow::Result<Uuid>> + Send;
}

/// Reads servers from the database.
pub trait ServerReader {
    fn get(
        &self,
        discord_server: DiscordServerId,
    ) -> impl Future<Output = anyhow::Result<Option<Server>>> + Send;

    fn list(
        &self,
        limit: i64,
        cursor: Option<DiscordServerId>,
    ) -> impl Future<Output = anyhow::Result<Vec<Server>>> + Send;

    fn count(&self) -> impl Future<Output = anyhow::Result<i64>> + Send;
}

#[derive(Debug)]
pub struct Repo {
    pool: PgPool,
}

impl Repo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl ServerCreator for Repo {
    async fn save(&self, server: Server) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO servers (id, discord_server, display_name, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(id)
        .bind(server.id.0 as i64)
        .bind(&server.display_name)
        .bind(server.created_at)
        .bind(server.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }
}

impl ServerReader for Repo {
    async fn get(&self, discord_server: DiscordServerId) -> anyhow::Result<Option<Server>> {
        let dao = sqlx::query_as::<_, ServerDAO>(
            r#"
            SELECT id, discord_server, display_name, created_at, updated_at
            FROM servers
            WHERE discord_server = $1
            "#,
        )
        .bind(discord_server.0 as i64)
        .fetch_optional(&self.pool)
        .await?;

        Ok(dao.map(Into::into))
    }

    async fn list(
        &self,
        limit: i64,
        cursor: Option<DiscordServerId>,
    ) -> anyhow::Result<Vec<Server>> {
        let daos = if let Some(last_server_id) = cursor {
            sqlx::query_as::<_, ServerDAO>(
                r#"
                SELECT id, discord_server, display_name, created_at, updated_at
                FROM servers
                WHERE discord_server > $1
                ORDER BY discord_server ASC
                LIMIT $2
                "#,
            )
            .bind(last_server_id.0 as i64)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, ServerDAO>(
                r#"
                SELECT id, discord_server, display_name, created_at, updated_at
                FROM servers
                ORDER BY discord_server ASC
                LIMIT $1
                "#,
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(daos.into_iter().map(Into::into).collect())
    }

    async fn count(&self) -> anyhow::Result<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM servers")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use migrations::run_migrations;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use testcontainers_modules::{postgres, testcontainers};

    #[test]
    fn dummy() {
        // Dummy test to help Gazelle discover this test module
    }

    async fn setup_test_db() -> (
        sqlx::PgPool,
        testcontainers::ContainerAsync<postgres::Postgres>,
    ) {
        let container = postgres::Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let host = container.get_host().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();

        let db_url = format!("postgres://postgres:postgres@{}:{}/postgres", host, port);

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect(&db_url)
            .await
            .expect("Failed to connect to database");

        run_migrations(&pool)
            .await
            .expect("Failed to run migrations");

        (pool, container)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_save_and_get_server() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        let server = Server::new(DiscordServerId(12345), "Test Server".to_string());
        let _uuid = repo.save(server).await.unwrap();

        let found = repo.get(DiscordServerId(12345)).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.id, DiscordServerId(12345));
        assert_eq!(found.display_name, "Test Server");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_nonexistent_server() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        let found = repo.get(DiscordServerId(99999)).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_save_duplicate_server_fails() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        let server1 = Server::new(DiscordServerId(12345), "Server One".to_string());
        repo.save(server1).await.unwrap();

        let server2 = Server::new(DiscordServerId(12345), "Server Two".to_string());
        let result = repo.save(server2).await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        repo.save(Server::new(DiscordServerId(333), "Third".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(111), "First".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(222), "Second".to_string()))
            .await
            .unwrap();

        let servers = repo.list(10, None).await.unwrap();
        assert_eq!(servers.len(), 3);
        assert_eq!(servers[0].id, DiscordServerId(111));
        assert_eq!(servers[1].id, DiscordServerId(222));
        assert_eq!(servers[2].id, DiscordServerId(333));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers_with_cursor() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        repo.save(Server::new(DiscordServerId(111), "First".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(222), "Second".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(333), "Third".to_string()))
            .await
            .unwrap();

        let servers = repo.list(10, Some(DiscordServerId(111))).await.unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].id, DiscordServerId(222));
        assert_eq!(servers[1].id, DiscordServerId(333));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_servers_with_limit() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        repo.save(Server::new(DiscordServerId(111), "First".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(222), "Second".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(333), "Third".to_string()))
            .await
            .unwrap();

        let servers = repo.list(2, None).await.unwrap();
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].id, DiscordServerId(111));
        assert_eq!(servers[1].id, DiscordServerId(222));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_count_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);

        assert_eq!(repo.count().await.unwrap(), 0);

        repo.save(Server::new(DiscordServerId(111), "First".to_string()))
            .await
            .unwrap();
        repo.save(Server::new(DiscordServerId(222), "Second".to_string()))
            .await
            .unwrap();

        assert_eq!(repo.count().await.unwrap(), 2);
    }
}
```

- [ ] **Step 2: Add repo targets to BUILD.bazel**

Append to `nicknamer2/src/discord_server/BUILD.bazel`:

```starlark
rust_library(
    name = "discord_server_repo",
    srcs = ["repo.rs"],
    visibility = ["//nicknamer2:__subpackages__"],
    deps = [
        ":discord_server",
        "//nicknamer2/src/name",
        "@crates//:anyhow",
        "@crates//:sqlx",
    ],
)

rust_test(
    name = "discord_server_repo_test",
    timeout = "short",
    crate = ":discord_server_repo",
    tags = ["requires-network"],
    deps = [
        "//nicknamer2/src/migrations",
        "@crates//:testcontainers-modules",
        "@crates//:tokio",
    ],
)
```

- [ ] **Step 3: Run tests**

Run: `aspect test //nicknamer2/src/discord_server:discord_server_repo_test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add nicknamer2/src/discord_server/
git commit -m "feat(nicknamer2): add Server repository with CRUD operations"
```

### Task 4: Create Server service

**Files:**

- Create: `nicknamer2/src/discord_server/service.rs`
- Modify: `nicknamer2/src/discord_server/BUILD.bazel`

- [ ] **Step 1: Write the service**

Create `nicknamer2/src/discord_server/service.rs`:

```rust
use discord_server::Server;
use discord_server_repo::{ServerCreator, ServerReader};
use name::DiscordServerId;

pub struct Service<T>
where
    T: ServerCreator + ServerReader,
{
    repo: T,
}

impl<T> Service<T>
where
    T: ServerCreator + ServerReader,
{
    pub fn new(repo: T) -> Self {
        Self { repo }
    }

    pub async fn create_server(
        &self,
        discord_server: DiscordServerId,
        display_name: String,
    ) -> anyhow::Result<DiscordServerId> {
        if discord_server.0 == 0 {
            return Err(anyhow::anyhow!("Server ID must be greater than 0"));
        }
        if display_name.is_empty() {
            return Err(anyhow::anyhow!("Display name must not be empty"));
        }
        let server = Server::new(discord_server, display_name);
        self.repo.save(server).await?;
        Ok(discord_server)
    }

    pub async fn get_server(
        &self,
        discord_server: DiscordServerId,
    ) -> anyhow::Result<Option<Server>> {
        self.repo.get(discord_server).await
    }

    pub async fn list_servers(
        &self,
        limit: i64,
        cursor: Option<DiscordServerId>,
    ) -> anyhow::Result<Vec<Server>> {
        self.repo.list(limit, cursor).await
    }

    pub async fn count_servers(&self) -> anyhow::Result<i64> {
        self.repo.count().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use discord_server_repo::Repo;
    use migrations::run_migrations;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    use testcontainers_modules::{postgres, testcontainers};

    #[test]
    fn dummy() {
        assert_eq!(true, true);
    }

    async fn setup_test_db() -> (
        sqlx::PgPool,
        testcontainers::ContainerAsync<postgres::Postgres>,
    ) {
        let container = postgres::Postgres::default()
            .start()
            .await
            .expect("Failed to start PostgreSQL container");

        let host = container.get_host().await.unwrap();
        let port = container.get_host_port_ipv4(5432).await.unwrap();

        let db_url = format!("postgres://postgres:postgres@{}:{}/postgres", host, port);

        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(std::time::Duration::from_secs(30))
            .connect(&db_url)
            .await
            .expect("Failed to connect to database");

        run_migrations(&pool)
            .await
            .expect("Failed to run migrations");

        (pool, container)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_server() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        let id = service
            .create_server(DiscordServerId(12345), "Test Server".to_string())
            .await
            .unwrap();

        assert_eq!(id, DiscordServerId(12345));

        // Verify it was persisted
        let server = service.get_server(DiscordServerId(12345)).await.unwrap();
        assert!(server.is_some());
        assert_eq!(server.unwrap().display_name, "Test Server");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_server_zero_id_fails() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        let result = service
            .create_server(DiscordServerId(0), "Test".to_string())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("greater than 0"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_server_empty_name_fails() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        let result = service
            .create_server(DiscordServerId(12345), "".to_string())
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not be empty"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_duplicate_server_fails() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        service
            .create_server(DiscordServerId(12345), "Server One".to_string())
            .await
            .unwrap();

        let result = service
            .create_server(DiscordServerId(12345), "Server Two".to_string())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_and_count_servers() {
        let (pool, _container) = setup_test_db().await;
        let repo = Repo::new(pool);
        let service = Service::new(repo);

        service
            .create_server(DiscordServerId(111), "First".to_string())
            .await
            .unwrap();
        service
            .create_server(DiscordServerId(222), "Second".to_string())
            .await
            .unwrap();

        let servers = service.list_servers(10, None).await.unwrap();
        assert_eq!(servers.len(), 2);

        let count = service.count_servers().await.unwrap();
        assert_eq!(count, 2);
    }
}
```

- [ ] **Step 2: Add service target to BUILD.bazel**

Append to `nicknamer2/src/discord_server/BUILD.bazel`:

```starlark
rust_library(
    name = "discord_server_service",
    srcs = ["service.rs"],
    visibility = ["//nicknamer2:__subpackages__"],
    deps = [
        ":discord_server",
        ":discord_server_repo",
        "//nicknamer2/src/name",
        "@crates//:anyhow",
    ],
)

rust_test(
    name = "discord_server_service_test",
    timeout = "short",
    crate = ":discord_server_service",
    tags = ["requires-network"],
    deps = [
        "//nicknamer2/src/migrations",
        "@crates//:sqlx",
        "@crates//:testcontainers-modules",
        "@crates//:tokio",
    ],
)
```

- [ ] **Step 3: Run tests**

Run: `aspect test //nicknamer2/src/discord_server:discord_server_service_test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add nicknamer2/src/discord_server/
git commit -m "feat(nicknamer2): add Server service layer with validation"
```

---

## Chunk 2: GraphQL Integration

### Task 5: Update GraphQL Context

**Files:**

- Modify: `nicknamer2/src/graphql/context.rs`
- Modify: `nicknamer2/src/graphql/BUILD.bazel` (graphql_context target)

- [ ] **Step 1: Add server_service to Context**

Replace `nicknamer2/src/graphql/context.rs` contents:

```rust
use std::sync::Arc;

use auth_claims::{AuthError, AuthService};
use discord_server_repo::Repo as ServerRepo;
use discord_server_service::Service as ServerService;
use juniper::FieldResult;
use name_repo::Repo;
use name_service::Service;

/// GraphQL context providing access to services and authentication.
pub struct Context {
    pub name_service: Arc<Service<Repo>>,
    pub server_service: Arc<ServerService<ServerRepo>>,
    pub jwks_validator: Arc<dyn AuthService>,
    pub auth_token: Option<String>,
}

impl juniper::Context for Context {}

/// Validates the auth token from the context. Returns Ok(()) if valid.
pub async fn require_auth(context: &Context) -> FieldResult<()> {
    let header_value = context
        .auth_token
        .as_deref()
        .ok_or(AuthError::MissingToken)?;

    context
        .jwks_validator
        .validate_auth_header(header_value)
        .await
        .map_err(juniper::FieldError::from)?;

    Ok(())
}
```

- [ ] **Step 2: Update graphql_context BUILD target deps**

In `nicknamer2/src/graphql/BUILD.bazel`, update the `graphql_context` target:

```starlark
rust_library(
    name = "graphql_context",
    srcs = ["context.rs"],
    visibility = ["//nicknamer2:__subpackages__"],
    deps = [
        "//nicknamer2/src/auth:auth_claims",
        "//nicknamer2/src/discord_server:discord_server_repo",
        "//nicknamer2/src/discord_server:discord_server_service",
        "//nicknamer2/src/name:name_repo",
        "//nicknamer2/src/name:name_service",
        "@crates//:juniper",
    ],
)
```

- [ ] **Step 3: Verify build**

Run: `aspect build //nicknamer2/src/graphql:graphql_context`
Expected: BUILD SUCCESS

- [ ] **Step 4: Commit**

```bash
git add nicknamer2/src/graphql/context.rs nicknamer2/src/graphql/BUILD.bazel
git commit -m "feat(nicknamer2): add server_service to GraphQL context"
```

### Task 6: Update Server GraphQL model

**Files:**

- Modify: `nicknamer2/src/graphql/model.rs`
- Modify: `nicknamer2/src/graphql/BUILD.bazel` (graphql_model target)

- [ ] **Step 1: Update Server struct**

In `nicknamer2/src/graphql/model.rs`, replace the `Server` struct (lines 16-18):

Old:

```rust
/// A Discord server
pub struct Server {
    pub id: DiscordServerId,
}
```

New:

```rust
/// A Discord server
pub struct Server {
    pub id: DiscordServerId,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

- [ ] **Step 2: Update the Server GraphQL impl**

Replace the `#[graphql_object] impl Server` block (lines 95-180). Keep the existing `names()` method body unchanged — only add the new fields:

```rust
#[graphql_object(context = Context)]
#[graphql(impl = NodeValue, description = "A Discord server")]
impl Server {
    /// The server ID (global Relay ID)
    fn id(&self) -> ID {
        RelayId::encode_server(self.id.0)
    }

    /// The Discord server ID
    fn server_id(&self) -> String {
        self.id.0.to_string()
    }

    /// The display name of the server
    fn display_name(&self) -> &str {
        &self.display_name
    }

    /// When the server was created
    fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// When the server was last updated
    fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Paginated list of names in this server
    async fn names(
        &self,
        context: &Context,
        first: Option<i32>,
        after: Option<String>,
    ) -> FieldResult<NameConnection> {
        // Validate and apply pagination limits
        let requested = first.unwrap_or(DEFAULT_PAGE_SIZE);
        if requested < MIN_PAGE_SIZE {
            return Err(format!(
                "Argument 'first' must be at least {}, got {}",
                MIN_PAGE_SIZE, requested
            )
            .into());
        }
        let limit = requested.min(MAX_PAGE_SIZE);

        // Decode cursor if provided
        let cursor_value = if let Some(after_cursor) = after {
            let cursor = Cursor::decode(&after_cursor)?;
            Some(cursor.discord_id_value())
        } else {
            None
        };

        // Track if we have a cursor for pagination info
        let has_cursor = cursor_value.is_some();

        // Request one extra item to determine if there's a next page
        let fetch_limit = (limit + 1) as i64;

        // Fetch total count and names from the service
        let total_count = context.name_service.count_names_by_server(self.id).await? as i32;

        let mut names = context
            .name_service
            .list_names(self.id, fetch_limit, cursor_value)
            .await?;

        // Determine if there's a next page
        let has_next_page = names.len() > limit as usize;
        if has_next_page {
            names.pop(); // Remove the extra item
        }

        // Build edges with cursors
        let edges: Vec<NameEdge> = names
            .into_iter()
            .map(|name| {
                let cursor = Cursor::new(name.id.discord_id);
                NameEdge {
                    cursor: cursor.encode(),
                    node: Name::from(name),
                }
            })
            .collect();

        // Build page info
        let page_info = PageInfo {
            has_next_page,
            has_previous_page: has_cursor,
            start_cursor: edges.first().map(|e| e.cursor.clone()),
            end_cursor: edges.last().map(|e| e.cursor.clone()),
        };

        Ok(NameConnection {
            edges,
            page_info,
            total_count,
        })
    }
}
```

- [ ] **Step 3: Add From impl for domain → GraphQL conversion**

Add after the existing `impl From<NameEntity> for Name` block (at the end of the file before the closing):

```rust
impl From<discord_server::Server> for Server {
    fn from(s: discord_server::Server) -> Self {
        Self {
            id: s.id,
            display_name: s.display_name,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}
```

- [ ] **Step 4: Update graphql_model BUILD target deps**

Add `discord_server` dep:

```starlark
rust_library(
    name = "graphql_model",
    srcs = ["model.rs"],
    visibility = ["//nicknamer2:__subpackages__"],
    deps = [
        ":graphql_context",
        ":graphql_relay",
        "//nicknamer2/src/discord_server",
        "//nicknamer2/src/name",
        "@crates//:chrono",
        "@crates//:juniper",
    ],
)
```

- [ ] **Step 5: Verify build**

Run: `aspect build //nicknamer2/src/graphql:graphql_model`
Expected: BUILD SUCCESS

- [ ] **Step 6: Commit**

```bash
git add nicknamer2/src/graphql/model.rs nicknamer2/src/graphql/BUILD.bazel
git commit -m "feat(nicknamer2): add displayName field to Server GraphQL type"
```

### Task 7: Update queries to use server_service

**Files:**

- Modify: `nicknamer2/src/graphql/query.rs`
- Modify: `nicknamer2/src/graphql/BUILD.bazel` (graphql_query target)

- [ ] **Step 1: Update server() query**

Replace the `server` query method (lines 14-32) to fetch from `server_service`:

```rust
    /// Fetch a Discord server by its ID
    async fn server(
        context: &Context,
        #[graphql(description = "The Discord server ID")] id: ID,
    ) -> FieldResult<Server> {
        require_auth(context).await?;

        let server_id: &str = &id;
        let server_id_u64 = server_id
            .parse::<u64>()
            .map_err(|_| "Invalid server ID format")?;

        if server_id_u64 == 0 {
            return Err("Server ID must be greater than 0".into());
        }

        let server = context
            .server_service
            .get_server(DiscordServerId(server_id_u64))
            .await?
            .ok_or("Server not found")?;

        Ok(Server::from(server))
    }
```

> **Breaking change:** Previously `server(id:)` always succeeded for any valid u64 (it just constructed a bare `Server` struct). Now it returns "Server not found" if the server hasn't been created via `createServer`. This is intentional — servers are now first-class entities.

- [ ] **Step 2: Update node() query for Server case**

Replace the `"Server"` arm in the `node()` method (lines 64-76). The old code constructs a bare `Server { id }` which no longer compiles since `Server` now has additional required fields:

Old:

```rust
            "Server" => {
                let discord_server = relay_id
                    .as_server()
                    .map_err(|e| format!("Invalid Server ID: {}", e))?;

                if discord_server == 0 {
                    return Err("Server ID must be greater than 0".into());
                }

                Ok(Some(NodeValue::Server(Server {
                    id: DiscordServerId(discord_server),
                })))
            }
```

New:

```rust
            "Server" => {
                let discord_server = relay_id
                    .as_server()
                    .map_err(|e| format!("Invalid Server ID: {}", e))?;

                if discord_server == 0 {
                    return Err("Server ID must be greater than 0".into());
                }

                let server = context
                    .server_service
                    .get_server(DiscordServerId(discord_server))
                    .await
                    .map_err(|e| format!("{e}"))?;

                Ok(server.map(|s| NodeValue::Server(Server::from(s))))
            }
```

- [ ] **Step 3: Update servers() query**

Replace the `servers` query method (lines 82-153):

```rust
    /// Paginated list of all servers
    async fn servers(
        context: &Context,
        #[graphql(description = "Number of servers to return")] first: Option<i32>,
        #[graphql(description = "Cursor to paginate after")] after: Option<String>,
    ) -> FieldResult<ServerConnection> {
        require_auth(context).await?;

        let requested = first.unwrap_or(DEFAULT_PAGE_SIZE);
        if requested < MIN_PAGE_SIZE {
            return Err(format!(
                "Argument 'first' must be at least {}, got {}",
                MIN_PAGE_SIZE, requested
            )
            .into());
        }
        let limit = requested.min(MAX_PAGE_SIZE);

        let cursor_value = if let Some(after_cursor) = after {
            let cursor = ServerCursor::decode(&after_cursor)?;
            Some(DiscordServerId(cursor.server_id_value()))
        } else {
            None
        };

        let has_cursor = cursor_value.is_some();
        let fetch_limit = (limit + 1) as i64;

        let total_count = context.server_service.count_servers().await? as i32;

        let mut servers = context
            .server_service
            .list_servers(fetch_limit, cursor_value)
            .await?;

        let has_next_page = servers.len() > limit as usize;
        if has_next_page {
            servers.pop();
        }

        let edges: Vec<ServerEdge> = servers
            .into_iter()
            .map(|server| {
                let cursor = ServerCursor::new(server.id.0);
                ServerEdge {
                    cursor: cursor.encode(),
                    node: Server::from(server),
                }
            })
            .collect();

        let page_info = PageInfo {
            has_next_page,
            has_previous_page: has_cursor,
            start_cursor: edges.first().map(|e| e.cursor.clone()),
            end_cursor: edges.last().map(|e| e.cursor.clone()),
        };

        Ok(ServerConnection {
            edges,
            page_info,
            total_count,
        })
    }
```

- [ ] **Step 4: Update graphql_query BUILD target deps**

Add `discord_server` dep to the `graphql_query` target (the `From` impl needs the domain type in scope):

```starlark
rust_library(
    name = "graphql_query",
    srcs = ["query.rs"],
    visibility = ["//nicknamer2:__subpackages__"],
    deps = [
        ":graphql_context",
        ":graphql_model",
        ":graphql_relay",
        "//nicknamer2/src/discord_server",
        "//nicknamer2/src/name",
        "@crates//:juniper",
    ],
)
```

- [ ] **Step 5: Verify build**

Run: `aspect build //nicknamer2/src/graphql:graphql_query`
Expected: BUILD SUCCESS

- [ ] **Step 6: Commit**

```bash
git add nicknamer2/src/graphql/query.rs nicknamer2/src/graphql/BUILD.bazel
git commit -m "feat(nicknamer2): use server_service for server queries"
```

### Task 8: Add createServer mutation

**Files:**

- Modify: `nicknamer2/src/graphql/mutation.rs`

- [ ] **Step 1: Add imports**

Add to the top of `nicknamer2/src/graphql/mutation.rs`:

```rust
use graphql_model;
use name::DiscordServerId;
```

(The existing file already imports `use name::{DiscordId, DiscordServerId};` — just verify `DiscordServerId` is included.)

- [ ] **Step 2: Add CreateServerInput and CreateServerPayload**

Add after the existing `CreateNamesPayload` impl block (after line 80):

```rust
/// Input for the createServer mutation.
#[derive(GraphQLInputObject)]
#[graphql(description = "Input for creating a Discord server")]
pub struct CreateServerInput {
    /// An opaque identifier for the client performing the mutation.
    pub client_mutation_id: Option<String>,
    /// The Discord server ID.
    pub discord_server_id: String,
    /// The display name for this server.
    pub display_name: String,
}

/// Payload returned by the createServer mutation.
pub struct CreateServerPayload {
    pub client_mutation_id: Option<String>,
    pub server: graphql_model::Server,
}

#[graphql_object]
#[graphql(context = Context)]
impl CreateServerPayload {
    /// The client mutation ID that was passed in.
    fn client_mutation_id(&self) -> Option<&str> {
        self.client_mutation_id.as_deref()
    }

    /// The newly created server.
    fn server(&self) -> &graphql_model::Server {
        &self.server
    }
}
```

- [ ] **Step 3: Add create_server method to MutationRoot**

Add to the `impl MutationRoot` block (after the `create_names` method, before the closing `}`):

```rust
    /// Create a new Discord server.
    async fn create_server(
        context: &Context,
        input: CreateServerInput,
    ) -> FieldResult<CreateServerPayload> {
        require_auth(context).await?;

        let discord_server_id: u64 = input
            .discord_server_id
            .parse()
            .map_err(|_| "Invalid server ID format")?;

        if discord_server_id == 0 {
            return Err("Server ID must be greater than 0".into());
        }

        let id = context
            .server_service
            .create_server(
                DiscordServerId(discord_server_id),
                input.display_name,
            )
            .await?;

        let created = context
            .server_service
            .get_server(id)
            .await?
            .ok_or("Failed to retrieve created server")?;

        Ok(CreateServerPayload {
            client_mutation_id: input.client_mutation_id,
            server: graphql_model::Server::from(created),
        })
    }
```

- [ ] **Step 4: Update graphql_mutation BUILD target deps**

Add `graphql_model` dep (needed for `graphql_model::Server`):

```starlark
rust_library(
    name = "graphql_mutation",
    srcs = ["mutation.rs"],
    visibility = ["//nicknamer2:__subpackages__"],
    deps = [
        ":graphql_context",
        ":graphql_model",
        "//nicknamer2/src/name",
        "@crates//:juniper",
    ],
)
```

- [ ] **Step 5: Verify build**

Run: `aspect build //nicknamer2/src/graphql:graphql_mutation`
Expected: BUILD SUCCESS

- [ ] **Step 6: Commit**

```bash
git add nicknamer2/src/graphql/mutation.rs nicknamer2/src/graphql/BUILD.bazel
git commit -m "feat(nicknamer2): add createServer GraphQL mutation"
```

### Task 9: Update Axum server and main.rs

**Files:**

- Modify: `nicknamer2/src/server/server.rs`
- Modify: `nicknamer2/src/server/BUILD.bazel`
- Modify: `nicknamer2/src/bin/main.rs`
- Modify: `nicknamer2/src/bin/BUILD.bazel`

- [ ] **Step 1: Update server.rs to accept and thread server_service**

Replace `nicknamer2/src/server/server.rs`:

```rust
use std::path::Path;
use std::sync::Arc;

use auth_claims::AuthService;
use axum::Extension;
use axum::http::HeaderMap;
use axum::routing::{MethodFilter, get, on};
use discord_server_repo::Repo as ServerRepo;
use discord_server_service::Service as ServerService;
use juniper_axum::extract::JuniperRequest;
use juniper_axum::graphiql;
use juniper_axum::response::JuniperResponse;
use name_repo::Repo;
use name_service::Service;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use graphql_context::Context;
use graphql_schema::Schema;

/// Custom GraphQL handler that creates a per-request context with auth info.
async fn graphql_handler(
    Extension(schema): Extension<Arc<Schema>>,
    Extension(name_service): Extension<Arc<Service<Repo>>>,
    Extension(server_service): Extension<Arc<ServerService<ServerRepo>>>,
    Extension(jwks_validator): Extension<Arc<dyn AuthService>>,
    headers: HeaderMap,
    JuniperRequest(request): JuniperRequest,
) -> JuniperResponse {
    let auth_token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let context = Context {
        name_service,
        server_service,
        jwks_validator,
        auth_token,
    };

    JuniperResponse(request.execute(&*schema, &context).await)
}

/// Creates the axum Router with GraphQL, GraphiQL, and optional static file serving.
pub fn create_router(
    schema: Arc<Schema>,
    name_service: Arc<Service<Repo>>,
    server_service: Arc<ServerService<ServerRepo>>,
    jwks_validator: Arc<dyn AuthService>,
    static_dir: Option<&str>,
) -> axum::Router {
    let router = axum::Router::new()
        .route(
            "/graphql",
            on(MethodFilter::GET.or(MethodFilter::POST), graphql_handler),
        )
        .route("/graphiql", get(graphiql("/graphql", None)))
        .layer(CorsLayer::permissive())
        .layer(Extension(schema))
        .layer(Extension(name_service))
        .layer(Extension(server_service))
        .layer(Extension(jwks_validator));

    match static_dir {
        Some(dir) => {
            let index = Path::new(dir).join("index.html");
            let serve_dir = ServeDir::new(dir).fallback(ServeFile::new(index));
            router.fallback_service(serve_dir)
        }
        None => router,
    }
}
```

- [ ] **Step 2: Update server BUILD.bazel deps**

Replace `nicknamer2/src/server/BUILD.bazel`:

```starlark
load("@rules_rust//rust:defs.bzl", "rust_library")

rust_library(
    name = "server",
    srcs = ["server.rs"],
    visibility = ["//nicknamer2:__subpackages__"],
    deps = [
        "//nicknamer2/src/auth:auth_claims",
        "//nicknamer2/src/discord_server:discord_server_repo",
        "//nicknamer2/src/discord_server:discord_server_service",
        "//nicknamer2/src/graphql:graphql_context",
        "//nicknamer2/src/graphql:graphql_schema",
        "//nicknamer2/src/name:name_repo",
        "//nicknamer2/src/name:name_service",
        "@crates//:axum",
        "@crates//:juniper_axum",
        "@crates//:tower-http",
    ],
)
```

> **Note:** Use `@crates//:juniper_axum` (underscore), matching the existing crate naming convention.

- [ ] **Step 3: Update main.rs**

Replace `nicknamer2/src/bin/main.rs`:

```rust
use std::sync::Arc;

use auth_claims::AuthService;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let config = nicknamer2_config::Config::from_env()?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.db_url)
        .await?;

    migrations::run_migrations(&pool).await?;
    tracing::info!("Database migrations applied successfully");

    let name_repo = name_repo::Repo::new(pool.clone());
    let name_service = Arc::new(name_service::Service::new(name_repo));

    let server_repo = discord_server_repo::Repo::new(pool);
    let server_service = Arc::new(discord_server_service::Service::new(server_repo));

    let jwks_validator: Arc<dyn AuthService> = match &config.casdoor_client_id {
        Some(client_id) => {
            let v = auth::JwksValidator::new(
                &config.casdoor_issuer_url,
                client_id,
                config.casdoor_jwks_url.as_deref(),
            )
            .await?;
            let jwks_source = config
                .casdoor_jwks_url
                .as_deref()
                .unwrap_or(&config.casdoor_issuer_url);
            tracing::info!("JWKS keys loaded from {}", jwks_source);
            Arc::new(v)
        }
        None => {
            tracing::warn!(
                "CASDOOR_CLIENT_ID not set — mutations will reject all requests as unauthenticated"
            );
            Arc::new(auth_claims::AlwaysDeny)
        }
    };

    let schema = Arc::new(graphql_schema::create_schema());

    let app = server::create_router(
        schema,
        name_service,
        server_service,
        jwks_validator,
        config.static_dir.as_deref(),
    );

    let address = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!("GraphQL server running on http://{}", address);
    tracing::info!("GraphiQL IDE available at http://{}/graphiql", address);
    if let Some(ref dir) = config.static_dir {
        tracing::info!("Serving frontend from {}", dir);
    }

    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 4: Update bin BUILD.bazel deps**

In `nicknamer2/src/bin/BUILD.bazel`, add discord_server deps to the `rust_binary` target:

```starlark
rust_binary(
    name = "nicknamer2",
    srcs = ["main.rs"],
    visibility = ["//visibility:public"],
    deps = [
        "//nicknamer2/src/auth",
        "//nicknamer2/src/auth:auth_claims",
        "//nicknamer2/src/config:nicknamer2_config",
        "//nicknamer2/src/discord_server:discord_server_repo",
        "//nicknamer2/src/discord_server:discord_server_service",
        "//nicknamer2/src/graphql:graphql_schema",
        "//nicknamer2/src/migrations",
        "//nicknamer2/src/name:name_repo",
        "//nicknamer2/src/name:name_service",
        "//nicknamer2/src/server",
        "@crates//:anyhow",
        "@crates//:axum",
        "@crates//:sqlx",
        "@crates//:tokio",
        "@crates//:tracing",
        "@crates//:tracing-subscriber",
    ],
)
```

Keep all the OCI/Docker targets unchanged.

- [ ] **Step 5: Update integration tests**

Modify `nicknamer2/src/graphql/tests.rs`. Update both `setup_test_context()` and `setup_test_context_with_auth_denial()` to create a `server_service` and pass it to `create_router`.

In `setup_test_context()` (around lines 48-54), change:

Old:

```rust
    let repo = name_repo::Repo::new(pool.clone());
    let service = Arc::new(name_service::Service::new(repo));

    let schema = Arc::new(create_schema());
    let jwks_validator: Arc<dyn AuthService> = Arc::new(auth_claims::AlwaysAllow);

    let app = server::create_router(schema, service, jwks_validator, None);
```

New:

```rust
    let repo = name_repo::Repo::new(pool.clone());
    let service = Arc::new(name_service::Service::new(repo));

    let server_repo = discord_server_repo::Repo::new(pool.clone());
    let server_service = Arc::new(discord_server_service::Service::new(server_repo));

    let schema = Arc::new(create_schema());
    let jwks_validator: Arc<dyn AuthService> = Arc::new(auth_claims::AlwaysAllow);

    let app = server::create_router(schema, service, server_service, jwks_validator, None);
```

Apply the same pattern to `setup_test_context_with_auth_denial()` (around lines 84-90).

Also update the `graphql_integration_test` BUILD target in `nicknamer2/src/graphql/BUILD.bazel` to add the new deps:

```starlark
rust_test(
    name = "graphql_integration_test",
    timeout = "short",
    srcs = ["tests.rs"],
    tags = ["requires-network"],
    deps = [
        ":graphql_relay",
        ":graphql_schema",
        "//nicknamer2/src/auth:auth_claims",
        "//nicknamer2/src/discord_server:discord_server_repo",
        "//nicknamer2/src/discord_server:discord_server_service",
        "//nicknamer2/src/migrations",
        "//nicknamer2/src/name",
        "//nicknamer2/src/name:name_repo",
        "//nicknamer2/src/name:name_service",
        "//nicknamer2/src/server",
        "@crates//:axum",
        "@crates//:base64",
        "@crates//:chrono",
        "@crates//:serde_json",
        "@crates//:sqlx",
        "@crates//:testcontainers-modules",
        "@crates//:tokio",
        "@crates//:tower",
        "@crates//:uuid",
    ],
)
```

- [ ] **Step 6: Verify full backend build and tests**

Run: `aspect build //nicknamer2/...`
Expected: BUILD SUCCESS

Run: `aspect test //nicknamer2/...`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add nicknamer2/src/server/ nicknamer2/src/bin/ nicknamer2/src/graphql/tests.rs nicknamer2/src/graphql/BUILD.bazel
git commit -m "feat(nicknamer2): wire server_service through Axum to GraphQL"
```

---

## Chunk 3: Frontend

### Task 10: Add create-server GraphQL mutation

**Files:**

- Create: `angular/projects/nicknamer2-web/src/app/graphql/create-server.graphql`

- [ ] **Step 1: Write the mutation file**

```graphql
mutation CreateServer($input: CreateServerInput!) {
  createServer(input: $input) {
    clientMutationId
    server {
      id
      serverId
      displayName
      createdAt
      updatedAt
    }
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add angular/projects/nicknamer2-web/src/app/graphql/create-server.graphql
git commit -m "feat(nicknamer2-web): add createServer GraphQL mutation"
```

### Task 11: Update generated GraphQL types

**Files:**

- Modify: `angular/projects/nicknamer2-web/src/generated/graphql.ts`

> **Note:** These are hand-written changes matching codegen output patterns. Run `graphql-codegen --config angular/projects/nicknamer2-web/codegen.ts` after the backend is deployed to verify/regenerate.

- [ ] **Step 1: Add `displayName` to the `Server` type**

In `graphql.ts`, update the `Server` type (around lines 118-126):

Old:

```typescript
/** A Discord server */
export type Server = Node & {
  __typename?: "Server";
  /** The server ID (global Relay ID) */
  id: Scalars["ID"]["output"];
  /** Paginated list of names in this server */
  names: NameConnection;
  /** The Discord server ID */
  serverId: Scalars["String"]["output"];
};
```

New:

```typescript
/** A Discord server */
export type Server = Node & {
  __typename?: "Server";
  /** When the server was created */
  createdAt: Scalars["DateTime"]["output"];
  /** The display name of the server */
  displayName: Scalars["String"]["output"];
  /** The server ID (global Relay ID) */
  id: Scalars["ID"]["output"];
  /** Paginated list of names in this server */
  names: NameConnection;
  /** The Discord server ID */
  serverId: Scalars["String"]["output"];
  /** When the server was last updated */
  updatedAt: Scalars["DateTime"]["output"];
};
```

- [ ] **Step 2: Add CreateServer types**

Add after the existing `CreateNameInput`/`CreateNamePayload` types (around line 210):

```typescript
export type CreateServerInput = {
  clientMutationId?: InputMaybe<Scalars["String"]["input"]>;
  discordServerId: Scalars["String"]["input"];
  displayName: Scalars["String"]["input"];
};

export type CreateServerPayload = {
  __typename?: "CreateServerPayload";
  clientMutationId?: Maybe<Scalars["String"]["output"]>;
  server: {
    __typename?: "Server";
    id: string;
    serverId: string;
    displayName: string;
    createdAt: any;
    updatedAt: any;
  };
};

export type CreateServerMutationVariables = Exact<{
  input: CreateServerInput;
}>;

export type CreateServerMutation = {
  __typename?: "MutationRoot";
  createServer: CreateServerPayload;
};
```

- [ ] **Step 3: Update GetServersQuery to include displayName**

Update the `GetServersQuery` type (around line 267) — add `displayName` to node:

Old:

```typescript
      node: { __typename?: 'Server'; id: string; serverId: string };
```

New:

```typescript
      node: { __typename?: 'Server'; id: string; serverId: string; displayName: string };
```

Update the `GetServersDocument` gql template (around line 348) — add `displayName`:

Old:

```typescript
id;
serverId;
```

New:

```typescript
id;
serverId;
displayName;
```

- [ ] **Step 4: Add CreateServer GQL document and service**

Add after the existing `CreateNamesGQL` class (at end of file):

```typescript
export const CreateServerDocument = gql`
  mutation CreateServer($input: CreateServerInput!) {
    createServer(input: $input) {
      clientMutationId
      server {
        id
        serverId
        displayName
        createdAt
        updatedAt
      }
    }
  }
`;

@Injectable({
  providedIn: "root",
})
export class CreateServerGQL extends Apollo.Mutation<
  CreateServerMutation,
  CreateServerMutationVariables
> {
  document = CreateServerDocument;

  constructor(apollo: Apollo.Apollo) {
    super(apollo);
  }
}
```

- [ ] **Step 5: Commit**

```bash
git add angular/projects/nicknamer2-web/src/generated/graphql.ts
git commit -m "feat(nicknamer2-web): add CreateServer types and displayName to generated GraphQL"
```

### Task 12: Update get-servers.graphql to include displayName

**Files:**

- Modify: `angular/projects/nicknamer2-web/src/app/graphql/get-servers.graphql`

- [ ] **Step 1: Add displayName to the query**

Replace with:

```graphql
query GetServers($first: Int!, $after: String) {
  servers(first: $first, after: $after) {
    edges {
      cursor
      node {
        id
        serverId
        displayName
      }
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add angular/projects/nicknamer2-web/src/app/graphql/get-servers.graphql
git commit -m "feat(nicknamer2-web): add displayName to GetServers query"
```

### Task 13: Create AddServerComponent

**Files:**

- Create: `angular/projects/nicknamer2-web/src/app/servers/add-server.component.ts`

- [ ] **Step 1: Write the component**

```typescript
import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  inject,
  signal,
} from "@angular/core";
import { takeUntilDestroyed } from "@angular/core/rxjs-interop";
import { FormsModule } from "@angular/forms";
import { Router } from "@angular/router";
import { CreateServerGQL } from "../../generated/graphql";

@Component({
  selector: "app-add-server",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule],
  template: `
    <div class="p-4 max-w-xl">
      <h1 class="text-2xl font-bold mb-4">Add Server</h1>

      <form
        class="flex flex-col gap-4"
        (ngSubmit)="onSubmit()"
        data-testid="add-server-form"
      >
        <label class="form-control w-full">
          <span class="label-text">Discord Server ID</span>
          <input
            type="text"
            class="input input-bordered w-full"
            [ngModel]="serverId()"
            (ngModelChange)="serverId.set($event)"
            name="serverId"
            required
            data-testid="server-id-input"
          />
        </label>
        <label class="form-control w-full">
          <span class="label-text">Display Name</span>
          <input
            type="text"
            class="input input-bordered w-full"
            [ngModel]="displayName()"
            (ngModelChange)="displayName.set($event)"
            name="displayName"
            required
            data-testid="display-name-input"
          />
        </label>
        <button
          type="submit"
          class="btn btn-primary"
          [disabled]="submitting() || !serverId() || !displayName()"
          data-testid="submit-server"
        >
          @if (submitting()) {
            <span class="loading loading-spinner loading-sm"></span>
          }
          Create Server
        </button>
      </form>

      @if (error()) {
        <div class="alert alert-error mt-4" data-testid="submit-error">
          {{ error() }}
        </div>
      }
    </div>
  `,
})
export class AddServerComponent {
  private readonly createServerGQL = inject(CreateServerGQL);
  private readonly router = inject(Router);
  private readonly destroyRef = inject(DestroyRef);

  protected readonly serverId = signal("");
  protected readonly displayName = signal("");
  protected readonly submitting = signal(false);
  protected readonly error = signal<string | null>(null);

  protected onSubmit(): void {
    this.submitting.set(true);
    this.error.set(null);

    this.createServerGQL
      .mutate({
        variables: {
          input: {
            discordServerId: this.serverId(),
            displayName: this.displayName(),
          },
        },
      })
      .pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe({
        next: (result) => {
          this.submitting.set(false);
          const serverId = result.data?.createServer?.server?.serverId;
          if (serverId) {
            this.router.navigate(["/servers", serverId, "names"]);
          }
        },
        error: (err: Error) => {
          this.error.set(err.message);
          this.submitting.set(false);
        },
      });
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add angular/projects/nicknamer2-web/src/app/servers/add-server.component.ts
git commit -m "feat(nicknamer2-web): add AddServerComponent with form UI"
```

### Task 14: Add route and update server list

**Files:**

- Modify: `angular/projects/nicknamer2-web/src/app/app.routes.ts`
- Modify: `angular/projects/nicknamer2-web/src/app/servers/server-list.component.ts`

- [ ] **Step 1: Add /servers/new route**

Replace `angular/projects/nicknamer2-web/src/app/app.routes.ts`:

```typescript
import { Routes } from "@angular/router";
import { authGuard } from "./auth/auth.guard";

export const routes: Routes = [
  {
    path: "",
    loadComponent: () =>
      import("./landing/landing.component").then((m) => m.LandingComponent),
  },
  {
    path: "dashboard",
    loadComponent: () =>
      import("./dashboard/dashboard.component").then(
        (m) => m.DashboardComponent,
      ),
    canActivate: [authGuard],
  },
  {
    path: "servers/new",
    loadComponent: () =>
      import("./servers/add-server.component").then(
        (m) => m.AddServerComponent,
      ),
    canActivate: [authGuard],
  },
  {
    path: "servers",
    loadComponent: () =>
      import("./servers/server-list.component").then(
        (m) => m.ServerListComponent,
      ),
    canActivate: [authGuard],
  },
  {
    path: "servers/:serverId/names/batch",
    loadComponent: () =>
      import("./servers/batch-add-names.component").then(
        (m) => m.BatchAddNamesComponent,
      ),
    canActivate: [authGuard],
  },
  {
    path: "servers/:serverId/names",
    loadComponent: () =>
      import("./servers/server-names.component").then(
        (m) => m.ServerNamesComponent,
      ),
    canActivate: [authGuard],
  },
  {
    path: "callback",
    loadComponent: () =>
      import("./auth/callback.component").then((m) => m.CallbackComponent),
  },
];
```

> **Note:** `servers/new` must come before `servers` to prevent Angular from matching `new` as a `:serverId` parameter.

- [ ] **Step 2: Update ServerListComponent template**

Replace the template in `server-list.component.ts`:

```typescript
  template: `
    <div class="p-4">
      <div class="flex items-center justify-between mb-4">
        <h1 class="text-2xl font-bold">Servers</h1>
        <a routerLink="/servers/new" class="btn btn-primary" data-testid="add-server-btn">
          Add Server
        </a>
      </div>

      @if (loading() && edges().length === 0) {
        <span class="loading loading-spinner loading-md"></span>
      }

      @if (error()) {
        <div class="alert alert-error">{{ error() }}</div>
      }

      <ul class="menu bg-base-200 rounded-box w-full max-w-xl">
        @for (edge of edges(); track edge.node.id) {
          <li data-testid="server-row">
            <a [routerLink]="['/servers', edge.node.serverId, 'names']">
              {{ edge.node.displayName }} ({{ edge.node.serverId }})
            </a>
          </li>
        }
      </ul>

      @if (hasNextPage()) {
        <button
          class="btn btn-outline mt-4"
          data-testid="load-more"
          [disabled]="loading()"
          (click)="loadMore()"
        >
          @if (loading()) {
            <span class="loading loading-spinner loading-sm"></span>
          }
          Load more
        </button>
      }
    </div>
  `,
```

The TypeScript class body remains unchanged — the `GetServersQuery` type update propagates through the existing `ServerEdge` type alias automatically.

- [ ] **Step 3: Commit**

```bash
git add angular/projects/nicknamer2-web/src/app/app.routes.ts angular/projects/nicknamer2-web/src/app/servers/server-list.component.ts
git commit -m "feat(nicknamer2-web): add server creation route and update server list"
```

### Task 15: Update frontend tests

**Files:**

- Modify: `angular/projects/nicknamer2-web/src/app/servers/server-list.component.spec.ts`

- [ ] **Step 1: Update mock data to include displayName**

In `server-list.component.spec.ts`, update all mocked server nodes to include `displayName`. For example:

Old:

```typescript
            { cursor: 'c1', node: { id: 'relay-1', serverId: '111' } },
            { cursor: 'c2', node: { id: 'relay-2', serverId: '222' } },
```

New:

```typescript
            { cursor: 'c1', node: { id: 'relay-1', serverId: '111', displayName: 'Server One' } },
            { cursor: 'c2', node: { id: 'relay-2', serverId: '222', displayName: 'Server Two' } },
```

Apply this to ALL `op.flush()` calls in the spec file (there are 3 tests with mocked data).

- [ ] **Step 2: Update assertions to check displayName**

In the 'should display servers after loading' test, update assertions:

Old:

```typescript
expect(rows[0].textContent).toContain("111");
expect(rows[1].textContent).toContain("222");
```

New:

```typescript
expect(rows[0].textContent).toContain("Server One");
expect(rows[0].textContent).toContain("111");
expect(rows[1].textContent).toContain("Server Two");
expect(rows[1].textContent).toContain("222");
```

- [ ] **Step 3: Add test for "Add Server" button**

Add a new test:

```typescript
it('should show "Add Server" button', () => {
  fixture.detectChanges();

  const op = apolloController.expectOne(GetServersDocument);
  op.flush({
    data: {
      servers: {
        edges: [],
        pageInfo: { hasNextPage: false, endCursor: null },
      },
    },
  });

  fixture.detectChanges();

  const btn = fixture.nativeElement.querySelector(
    '[data-testid="add-server-btn"]',
  );
  expect(btn).toBeTruthy();
  expect(btn.textContent).toContain("Add Server");
  expect(btn.getAttribute("href")).toBe("/servers/new");
});
```

- [ ] **Step 4: Commit**

```bash
git add angular/projects/nicknamer2-web/src/app/servers/server-list.component.spec.ts
git commit -m "test(nicknamer2-web): update server list tests for displayName and Add Server button"
```

### Task 16: Run Gazelle and format

- [ ] **Step 1: Regenerate BUILD files**

Run: `bazel run gazelle`

- [ ] **Step 2: Format all code**

Run: `bazel run //tools/format`

- [ ] **Step 3: Build everything**

Run: `aspect build //nicknamer2/... //angular/projects/nicknamer2-web/...`
Expected: BUILD SUCCESS

- [ ] **Step 4: Run all nicknamer2 tests**

Run: `aspect test //nicknamer2/...`
Expected: PASS

- [ ] **Step 5: Run frontend tests**

Run: `aspect test //angular/projects/nicknamer2-web:test`
Expected: PASS

- [ ] **Step 6: Final commit (if Gazelle/format changed anything)**

```bash
git add nicknamer2/ angular/projects/nicknamer2-web/
git commit -m "chore: regenerate BUILD files and format"
```
