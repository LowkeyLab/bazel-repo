# sqlx Migrations for nicknamer2

## Problem

nicknamer2 uses a manual migration approach: `sqlx::raw_sql()` executes a single SQL file on every server startup. There is no tracking table, no checksum validation, and no support for incremental migrations. Adding a new migration requires modifying Rust code rather than dropping in a SQL file.

## Goal

Replace the manual runner with sqlx's built-in `Migrator` system. Fresh start — the current schema (from migration 003) becomes the sole V1 migration. sqlx manages the `_sqlx_migrations` tracking table automatically, enabling incremental migrations going forward.

## Migration Strategy

- **Primary**: `sqlx::migrate!()` macro, which embeds migration SQL at compile time
- **Fallback**: If Bazel's sandbox breaks `CARGO_MANIFEST_DIR` resolution, switch to `include_str!()` with a manually constructed `Migrator`
- **Tracking**: sqlx's `_sqlx_migrations` table provides versioning, checksums, and idempotency

## Architecture Decision: Single Owner of Migration Logic

The `migrations` library crate (`nicknamer2/src/migrations/`) owns all migration logic. `main.rs` continues calling `migrations::run_migrations(&pool)` — it does not invoke the `sqlx::migrate!()` macro directly. This preserves the current layering and keeps migration logic in one place, testable via the existing `rust_test` target.

The `sqlx::migrate!()` macro is invoked only inside `migrations.rs`. The `compile_data` attribute therefore goes on the `rust_library` target in `nicknamer2/src/migrations/BUILD.bazel`, not on the binary.

## Bazel Sandbox: `CARGO_MANIFEST_DIR` Resolution

Under `rules_rust`, `CARGO_MANIFEST_DIR` is set to the package directory of the `rust_library` target that contains the macro invocation. For the `migrations` library at `nicknamer2/src/migrations/BUILD.bazel`, this resolves to `nicknamer2/src/migrations/` in the sandbox.

The `sqlx::migrate!()` path must therefore be relative to that directory:

```rust
sqlx::migrate!("../../migrations")
```

This resolves to `nicknamer2/migrations/` — the new migrations directory. The SQL files must be available in the sandbox via `compile_data` on the library target.

If `CARGO_MANIFEST_DIR` does not resolve as expected (verified during implementation), the fallback uses `include_str!()` to embed SQL at compile time and constructs a `Migrator` manually.

## Naming Convention

Migration files use zero-padded sequential numbers: `001_create_names_table.sql`. This is compatible with sqlx's non-reversible migration format (`{version}_{description}.sql` where version is any positive integer). Timestamp-based naming is not needed given the fresh start with a single migration.

## File Changes

### New

| File                                               | Purpose                                                                                                         |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `nicknamer2/migrations/001_create_names_table.sql` | V1 migration — `CREATE TABLE IF NOT EXISTS names` only (no `DROP TABLE` statements since this is a fresh start) |
| `nicknamer2/migrations/BUILD.bazel`                | `filegroup` for Bazel visibility                                                                                |

### Modified

| File                                      | Change                                                                                           |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `Cargo.toml` (workspace root)             | Add `"migrate"` to sqlx features, then run `CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index`   |
| `nicknamer2/src/migrations/migrations.rs` | Rewrite `run_migrations` to use `sqlx::migrate!("../../migrations")` instead of `raw_sql()`      |
| `nicknamer2/src/migrations/BUILD.bazel`   | Update `compile_data` to reference `//nicknamer2/migrations` filegroup; remove old SQL file deps |

### Deleted

| File                                                          | Reason                                                         |
| ------------------------------------------------------------- | -------------------------------------------------------------- |
| `nicknamer2/src/migrations/001_create_users_table.sql`        | Superseded — dead code                                         |
| `nicknamer2/src/migrations/002_create_names_table.sql`        | Superseded — dead code                                         |
| `nicknamer2/src/migrations/003_drop_users_recreate_names.sql` | Replaced by `nicknamer2/migrations/001_create_names_table.sql` |

### Unchanged

| File                             | Note                                                                       |
| -------------------------------- | -------------------------------------------------------------------------- |
| `nicknamer2/src/bin/main.rs`     | Keeps calling `migrations::run_migrations(&pool)` — no change needed       |
| `nicknamer2/src/bin/BUILD.bazel` | No change — binary depends on `migrations` library, not SQL files directly |

## Code Shape

### migrations.rs

```rust
use sqlx::PgPool;

pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}
```

### Fallback (if Bazel breaks macro)

```rust
use sqlx::migrate::{Migration, MigrationType, Migrator};

const M001: &str = include_str!("../../migrations/001_create_names_table.sql");

// Build Migrator from embedded SQL strings with version + checksum
```

## Testing

Migration tests stay in `nicknamer2/src/migrations/` as a `rust_test` target using testcontainers (tagged `requires-network`).

**Existing assertions** (updated to use new runner):

- `names` table exists with correct columns after migration

**New assertion:**

- `_sqlx_migrations` table contains exactly one entry with version 1

## Out of Scope

- **sqlx-cli integration** (offline query checking, `sqlx prepare`)
- **Reversible migrations** (up/down SQL files)
- **Custom migration table name**
- **Migration of existing production data** (fresh start — databases are wiped)
