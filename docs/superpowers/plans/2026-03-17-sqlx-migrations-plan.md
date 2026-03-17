# sqlx Migrations for nicknamer2 — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace nicknamer2's manual `raw_sql()` migration runner with sqlx's built-in `Migrator` system.

**Architecture:** The `migrations` library crate is the single owner of migration logic. It invokes `sqlx::migrate!("../../migrations")` to embed SQL files at compile time. `main.rs` continues calling `migrations::run_migrations(&pool)` unchanged. If `CARGO_MANIFEST_DIR` doesn't resolve correctly under Bazel, fall back to `include_str!()` with a manual `Migrator`.

**Tech Stack:** Rust, sqlx 0.8 (with `migrate` feature), Bazel (rules_rust), testcontainers

**Spec:** `docs/superpowers/specs/2026-03-17-sqlx-migrations-design.md`

---

## Chunk 1: Setup and Migration File

### Task 1: Add `migrate` feature to sqlx and repin

**Files:**

- Modify: `Cargo.toml:31-37`

- [ ] **Step 1: Add `migrate` feature**

In `Cargo.toml`, the sqlx dependency (lines 31-37) currently reads:

```toml
sqlx = { version = "0.8", features = [
  "postgres",
  "runtime-tokio-rustls",
  "uuid",
  "chrono",
  "macros",
] }
```

Add `"migrate"` to the features list:

```toml
sqlx = { version = "0.8", features = [
  "postgres",
  "runtime-tokio-rustls",
  "uuid",
  "chrono",
  "macros",
  "migrate",
] }
```

- [ ] **Step 2: Repin crate index**

Run:

```bash
CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index
```

Expected: completes successfully, updates lock files.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "feat(nicknamer2): add sqlx migrate feature to workspace"
```

Note: if `bazel sync` modified other Bazel lock files, include those too.

---

### Task 2: Create the V1 migration SQL file

**Files:**

- Create: `nicknamer2/migrations/001_create_names_table.sql`
- Create: `nicknamer2/migrations/BUILD.bazel`

- [ ] **Step 1: Create migration directory and SQL file**

Create `nicknamer2/migrations/001_create_names_table.sql` with the schema from the current migration 003, but **without** the `DROP TABLE` statements (fresh start):

```sql
CREATE TABLE IF NOT EXISTS names (
    id UUID PRIMARY KEY,
    discord_id BIGINT NOT NULL,
    discord_server BIGINT NOT NULL,
    name VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(discord_id, discord_server)
);
```

- [ ] **Step 2: Create BUILD.bazel for migrations directory**

Create `nicknamer2/migrations/BUILD.bazel`:

```python
filegroup(
    name = "migrations",
    srcs = glob(["*.sql"]),
    visibility = ["//nicknamer2:__subpackages__"],
)
```

- [ ] **Step 3: Commit**

```bash
git add nicknamer2/migrations/
git commit -m "feat(nicknamer2): add V1 migration SQL and BUILD target"
```

---

## Chunk 2: Rewrite Migration Runner (TDD)

### Task 3: Write failing test for sqlx migration tracking

**Files:**

- Modify: `nicknamer2/src/migrations/migrations.rs:11-79`

- [ ] **Step 1: Rewrite the test module**

Replace the entire test module in `migrations.rs` (lines 11-80) with a test that verifies both the `names` table and the `_sqlx_migrations` tracking table. Remove the dead `users` table assertion and the dummy test.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers_modules::postgres;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    async fn setup_pool() -> (sqlx::PgPool, impl std::any::Any) {
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

        (pool, container)
    }

    #[tokio::test]
    async fn test_migration_creates_names_table() {
        let (pool, _container) = setup_pool().await;

        run_migrations(&pool)
            .await
            .expect("Migrations should run successfully");

        let names_exists: (bool,) = sqlx::query_as(
            r#"
            SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_schema = 'public'
                AND table_name = 'names'
            )
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to check names table");

        assert!(names_exists.0, "Names table should exist after migration");
    }

    #[tokio::test]
    async fn test_migration_tracking_table_exists() {
        let (pool, _container) = setup_pool().await;

        run_migrations(&pool)
            .await
            .expect("Migrations should run successfully");

        let migration_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM _sqlx_migrations",
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to query _sqlx_migrations");

        assert_eq!(
            migration_count.0, 1,
            "_sqlx_migrations should contain exactly one applied migration"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
aspect test //nicknamer2/src/migrations:migrations_test
```

Expected: FAIL — the current `raw_sql()` runner does not create `_sqlx_migrations`. The `test_migration_tracking_table_exists` test should fail with a "relation does not exist" error. The `test_migration_creates_names_table` test may still pass since the old runner also creates the names table.

- [ ] **Step 3: Commit failing tests**

```bash
git add nicknamer2/src/migrations/migrations.rs
git commit -m "test(nicknamer2): add migration tracking table assertion (failing)"
```

---

### Task 4: Implement the sqlx migrate!() runner

**Files:**

- Modify: `nicknamer2/src/migrations/migrations.rs:1-9`
- Modify: `nicknamer2/src/migrations/BUILD.bazel`

- [ ] **Step 1: Rewrite run_migrations to use sqlx::migrate!()**

Replace lines 1-9 of `migrations.rs` with:

```rust
use sqlx::PgPool;

/// Runs all migrations for the nicknamer2 database.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}
```

Key changes:

- Removed the `include_str!()` + `raw_sql()` approach
- Return type changed from `sqlx::Error` to `sqlx::migrate::MigrateError` (the type returned by the migrator)
- Path `"../../migrations"` is relative to `CARGO_MANIFEST_DIR` which points to `nicknamer2/src/migrations/` under rules_rust

- [ ] **Step 2: Update BUILD.bazel to reference new migrations filegroup**

Replace `nicknamer2/src/migrations/BUILD.bazel` with:

```python
load("@rules_rust//rust:defs.bzl", "rust_library", "rust_test")

rust_library(
    name = "migrations",
    srcs = ["migrations.rs"],
    compile_data = [
        "//nicknamer2/migrations",
    ],
    visibility = ["//nicknamer2:__subpackages__"],
    deps = ["@crates//:sqlx"],
)

rust_test(
    name = "migrations_test",
    timeout = "short",
    crate = ":migrations",
    compile_data = [
        "//nicknamer2/migrations",
    ],
    tags = [
        "requires-network",
    ],
    deps = [
        "@crates//:testcontainers-modules",
        "@crates//:tokio",
    ],
)
```

Changes:

- `compile_data` on `rust_library` references the `//nicknamer2/migrations` filegroup instead of the local SQL file
- `compile_data` duplicated on `rust_test` because `rules_rust` does not propagate `compile_data` from the `crate` dependency — the `sqlx::migrate!()` macro re-expands during test compilation and needs the SQL files in the sandbox

- [ ] **Step 3: Build to verify compilation**

Run:

```bash
aspect build //nicknamer2/src/migrations:migrations
```

Expected: compiles successfully. If it fails with `CARGO_MANIFEST_DIR` issues, proceed to Task 5 (fallback).

- [ ] **Step 4: Run tests**

Run:

```bash
aspect test //nicknamer2/src/migrations:migrations_test
```

Expected: both tests PASS. If they fail due to the macro path not resolving, proceed to Task 5.

- [ ] **Step 5: Commit**

```bash
git add nicknamer2/src/migrations/migrations.rs nicknamer2/src/migrations/BUILD.bazel
git commit -m "feat(nicknamer2): switch to sqlx migrate!() for migration runner"
```

---

### Task 5: Fallback — include_str!() with manual Migrator (ONLY if Task 4 fails)

**Skip this task if Task 4 succeeded.**

**Files:**

- Modify: `nicknamer2/src/migrations/migrations.rs`

- [ ] **Step 1: Replace migrate!() with manual Migrator construction**

If `sqlx::migrate!()` fails under Bazel's sandbox, replace the `run_migrations` function:

```rust
use sqlx::PgPool;
use sqlx::migrate::{Migration, MigrationType, MigrationSource, Migrator};
use std::borrow::Cow;
use std::pin::Pin;

const M001_SQL: &str = include_str!("../../migrations/001_create_names_table.sql");

/// A migration source that wraps a Vec<Migration> for manual Migrator construction.
#[derive(Debug)]
struct StaticMigrations(Vec<Migration>);

impl<'s> MigrationSource<'s> for StaticMigrations {
    fn resolve(
        self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Vec<Migration>, Box<dyn std::error::Error + Sync + Send>>> + Send + 's>> {
        Box::pin(async move { Ok(self.0) })
    }
}

/// Runs all migrations for the nicknamer2 database.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    let source = StaticMigrations(vec![
        Migration::new(
            1,
            Cow::Borrowed("create_names_table"),
            MigrationType::Simple,
            Cow::Borrowed(M001_SQL),
            false,
        ),
    ]);

    Migrator::new(source).await?.run(pool).await
}
```

- [ ] **Step 2: Run tests**

Run:

```bash
aspect test //nicknamer2/src/migrations:migrations_test
```

Expected: both tests PASS.

- [ ] **Step 3: Commit**

```bash
git add nicknamer2/src/migrations/migrations.rs
git commit -m "feat(nicknamer2): use include_str fallback for sqlx migrator"
```

---

## Chunk 3: Cleanup

### Task 6: Delete old migration SQL files

**Files:**

- Delete: `nicknamer2/src/migrations/001_create_users_table.sql`
- Delete: `nicknamer2/src/migrations/002_create_names_table.sql`
- Delete: `nicknamer2/src/migrations/003_drop_users_recreate_names.sql`

- [ ] **Step 1: Remove old SQL files**

```bash
rm nicknamer2/src/migrations/001_create_users_table.sql
rm nicknamer2/src/migrations/002_create_names_table.sql
rm nicknamer2/src/migrations/003_drop_users_recreate_names.sql
```

- [ ] **Step 2: Build to verify nothing references them**

Run:

```bash
aspect build //nicknamer2/...
```

Expected: builds successfully. The old SQL files are no longer referenced by any `compile_data` or `include_str!()`.

- [ ] **Step 3: Commit**

```bash
git add -u nicknamer2/src/migrations/
git commit -m "chore(nicknamer2): remove superseded migration SQL files"
```

---

### Task 7: Full integration verification

- [ ] **Step 1: Build everything**

Run:

```bash
aspect build //nicknamer2/...
```

Expected: all nicknamer2 targets build successfully.

- [ ] **Step 2: Run all nicknamer2 tests**

Run:

```bash
aspect test //nicknamer2/...
```

Expected: all tests pass.

- [ ] **Step 3: Lint nicknamer2**

Run:

```bash
aspect lint //nicknamer2/...
```

Expected: no lint errors. The `migrate` feature addition and code changes should not introduce unused imports or formatting issues.

- [ ] **Step 4: Run the full repo build and test (catch cross-package issues from workspace Cargo.toml change)**

Run:

```bash
aspect build //...
aspect test //...
```

Expected: clean build and all tests pass. The `migrate` feature is additive to sqlx, so other Rust crates should be unaffected.
