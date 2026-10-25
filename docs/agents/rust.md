# Rust (Nicknamer)

Read this reference before you edit or review Rust code.
Use the [development workflow](development.md) for commands and verification.
These rules are the language-specific standards referenced by [CODING_STANDARDS.md](../../CODING_STANDARDS.md).

**Edition:** 2024

**Imports:**

- Group imports: `std`, external crates, internal modules
- Use explicit paths, avoid glob imports

**Types & Error Handling:**

- Prefer newtypes for distinct domain concepts where practical, such as `struct UserId(Uuid);`.
- Use these newtypes instead of interchangeable primitives or type aliases.
- Keep newtype fields private when they enforce invariants.
- Validate values in constructors or `TryFrom` implementations.
- Expose only traits and conversions appropriate to the domain.
- Unwrap newtypes at integration boundaries as needed.
- Use explicit error handling with `anyhow::Result<T>` for applications/binaries
- Use `thiserror` for library error types
- Prefer `?` operator for error propagation

**Async & Runtime:**

- Use `tokio` runtime exclusively
- Prefer async/await patterns
- Use `tracing` for structured logging. Do not use `println!` or `dbg!`.

**Naming Conventions:**

- `snake_case` for functions, variables, modules
- `PascalCase` for types, traits, enums
- `SCREAMING_SNAKE_CASE` for constants

**Database:**

- Use `SeaORM` entities and migrations
- Migrations in `nicknamer/migration/`

**Testing:**

- Use `googletest` as the test library for all Rust projects and its matchers for assertions
- Use `insta` for snapshot testing alongside `googletest`
- Integration tests use `testcontainers` for PostgreSQL
