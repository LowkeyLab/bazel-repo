# Save User Implementation Plan (SQLx)

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement functionality to save users to a PostgreSQL database using `sqlx` directly.

**Architecture:** Use `sqlx` directly to interact with the database. We will add `sqlx` as a dependency and update `repo.rs` to include the `save` method implementation using `sqlx` queries.

**Tech Stack:** Rust, SQLx, PostgreSQL, Tokio.

---

### Task 1: Add SQLx Dependency

**Files:**

- Modify: `Cargo.toml`
- Modify: `MODULE.bazel` (likely not needed as we use crate_universe, but we need to run repin)

**Step 1: Update Cargo.toml**

Add `sqlx` to `[dependencies]`.

```toml
sqlx = { version = "0.8", features = ["postgres", "runtime-tokio-rustls", "uuid", "chrono", "macros"] }
```

**Step 2: Update lockfile**

Run `bazel run @crates//:crate_index_repin` (or equivalent for this repo, likely just `bazel mod tidy` or `bazel sync` depending on setup, but usually `CARGO_BAZEL_REPIN=1 bazel sync --only=crates` or similar. In this repo, it seems `bazel mod tidy` might be enough or `bazel run @rules_rust//crate_universe:extensions.bzl%crate_index_repin`? actually `AGENTS.md` says "Update BUILD files (Go, TS, Proto): bazel run gazelle". For Rust deps, we modify `Cargo.toml`. `rules_rust` handles the rest usually via `crate_universe`. I'll try running a build which triggers fetch/repin if needed, or explicitly repin).

_Correction:_ I'll use `CARGO_BAZEL_REPIN=1 bazel sync --only=crates` to ensure lockfile is updated.

---

### Task 2: Implement Save in Repo

**Files:**

- Modify: `nicknamer2/src/user/repo.rs`
- Modify: `nicknamer2/src/user/BUILD.bazel`

**Step 1: Update BUILD.bazel**

Add `sqlx` to `deps`.

```python
rust_library(
    name = "repo",
    srcs = ["repo.rs"],
    deps = [
        ":user",
        "@crates//:thiserror",
        "@crates//:sqlx", # Add this
    ],
)
```

**Step 2: Implement Save**

Update `repo.rs` to use `sqlx::PgPool`.

```rust
use sqlx::PgPool;
use user::User;

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("Database error: {0}")]
    DbError(String),
}

trait UserSaver {
    async fn save(user: User) -> Result<(), Error>;
}

pub struct PostgresUserRepo {
    pool: PgPool,
}

impl PostgresUserRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl UserSaver for PostgresUserRepo {
    async fn save(user: User) -> Result<(), Error> {
        sqlx::query!(
            r#"
            INSERT INTO users (id, discord_id, created_at, updated_at, valid_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            user.id,
            user.discord_id as i64, // Postgres doesn't have u64
            user.created_at,
            user.updated_at,
            user.valid_at
        )
        .execute(&self.pool)
        .await
        .map_err(|e| Error::DbError(e.to_string()))?;

        Ok(())
    }
}
```

**Step 3: Verify**

Run `bazel build //nicknamer2/src/user:repo`.
