# sqlx Migrations for nicknamer2

## Problem

nicknamer2 uses a manual migration approach: `sqlx::raw_sql()` executes a single SQL file on every server startup. There is no tracking table, no checksum validation, and no support for incremental migrations. Adding a new migration requires modifying Rust code rather than dropping in a SQL file.

## Goal

Replace the manual runner with sqlx's built-in `Migrator` system. Fresh start — the current schema (from migration 003) becomes the sole V1 migration. sqlx manages the `_sqlx_migrations` tracking table automatically, enabling incremental migrations going forward.

## Migration Strategy

- **Primary**: `sqlx::migrate!()` macro, which embeds migration SQL at compile time
- **Fallback**: If Bazel's sandbox breaks `CARGO_MANIFEST_DIR` resolution, switch to `include_str!()` with a manually constructed `Migrator`
- **Tracking**: sqlx's `_sqlx_migrations` table provides versioning, checksums, and idempotency

## File Changes

### New

| File | Purpose |
|------|---------|
| `nicknamer2/migrations/001_create_names_table.sql` | V1 migration — contents of current 003 SQL |
| `nicknamer2/migrations/BUILD.bazel` | `filegroup` or `exports_files` for Bazel visibility |

### Modified

| File | Change |
|------|--------|
| `Cargo.toml` (workspace root) | Add `"migrate"` to sqlx features |
| `nicknamer2/src/bin/main.rs` | Replace `migrations::run_migrations(&pool)` with `sqlx::migrate!().run(&pool).await?` |
| `nicknamer2/src/bin/BUILD.bazel` | Add `compile_data` pointing to `//nicknamer2/migrations` |
| `nicknamer2/src/migrations/migrations.rs` | Rewrite to use `sqlx::migrate!()` instead of `raw_sql()` |
| `nicknamer2/src/migrations/BUILD.bazel` | Update `compile_data` to reference new migrations location; remove old SQL deps |

### Deleted

| File | Reason |
|------|--------|
| `nicknamer2/src/migrations/001_create_users_table.sql` | Superseded — dead code |
| `nicknamer2/src/migrations/002_create_names_table.sql` | Superseded — dead code |
| `nicknamer2/src/migrations/003_drop_users_recreate_names.sql` | Replaced by `nicknamer2/migrations/001_create_names_table.sql` |

## Code Shape

### main.rs

```rust
sqlx::migrate!("migrations").run(&pool).await?;
```

The path is relative to `CARGO_MANIFEST_DIR` (i.e., `nicknamer2/`).

### migrations.rs (test host)

Rewritten to expose a `run_migrations` function that calls `sqlx::migrate!()` and retains the existing testcontainers-based tests verifying that the `names` table is created correctly.

### Fallback (if Bazel breaks macro)

```rust
use sqlx::migrate::{Migration, MigrationType, Migrator};

const M001: &str = include_str!("../../migrations/001_create_names_table.sql");

// Build Migrator from embedded SQL strings
```

## Testing

- Migration tests stay in `nicknamer2/src/migrations/` as a `rust_test` target
- Tests use testcontainers (tagged `requires-network`) to spin up PostgreSQL
- Tests verify: `names` table exists with correct schema after migration runs
- The `_sqlx_migrations` table should contain exactly one applied migration entry

## Out of Scope

- **sqlx-cli integration** (offline query checking, `sqlx prepare`)
- **Reversible migrations** (up/down SQL files)
- **Custom migration table name**
- **Migration of existing production data** (fresh start approach — databases are wiped)
