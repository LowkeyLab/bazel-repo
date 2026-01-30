# Nicknamer2 Test Refactor: std::OnceLock → tokio::OnceCell

**Date:** 2026-01-30  
**Status:** Approved  
**File:** `nicknamer2/src/user/service.rs`

## Goal

Refactor tests to use `tokio::sync::OnceCell` instead of `std::sync::OnceLock`, eliminating the need for `block_in_place` calls and making the test initialization fully async-native.

## Current Issues

- Uses `std::sync::OnceLock` which requires a sync closure (line 34)
- Forces use of `tokio::task::block_in_place` to work around async/sync boundary (line 68)
- Drop impl creates new runtime via `spawn_blocking` which is inefficient (lines 52-59)

## Design

### 1. Replace OnceLock with OnceCell

**Change:**

```rust
// Before
use std::sync::OnceLock;
static DB_SETUP: OnceLock<(sqlx::PgPool, testcontainers::ContainerAsync<postgres::Postgres>)> = OnceLock::new();

// After
use tokio::sync::OnceCell;
static DB_SETUP: OnceCell<(sqlx::PgPool, testcontainers::ContainerAsync<postgres::Postgres>)> = OnceCell::const_new();
```

### 2. Update DbCleanup struct

**Add runtime handle:**

```rust
struct DbCleanup {
    pool: sqlx::PgPool,
    handle: tokio::runtime::Handle,
}
```

### 3. Simplify Drop implementation

**Change:**

```rust
impl Drop for DbCleanup {
    fn drop(&mut self) {
        let pool = self.pool.clone();
        let _ = self.handle.block_on(async {
            sqlx::query("TRUNCATE TABLE users RESTART IDENTITY CASCADE")
                .execute(&pool)
                .await
        });
    }
}
```

**Benefits:**

- No more `spawn_blocking`
- No new runtime creation
- Uses existing test runtime handle

### 4. Refactor get_test_db()

**Change:**

```rust
async fn get_test_db() -> (DbCleanup, sqlx::PgPool) {
    let (pool, _container) = DB_SETUP
        .get_or_init(|| async {
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
        })
        .await;

    let handle = tokio::runtime::Handle::current();
    (DbCleanup { pool: pool.clone(), handle }, pool.clone())
}
```

**Benefits:**

- No more `block_in_place` wrapper
- Pure async initialization
- More idiomatic tokio code

## Testing

- Run `bazel test //nicknamer2/...` to verify all tests pass
- Tests should execute with same behavior but cleaner implementation
- No functional changes to test logic

## Summary

This refactor makes the test infrastructure fully async-native by:

1. Using `tokio::sync::OnceCell` for async-friendly one-time initialization
2. Storing `tokio::runtime::Handle` in cleanup guard for efficient Drop impl
3. Eliminating all `block_in_place` and `spawn_blocking` workarounds
