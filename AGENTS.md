# AGENTS.md (Monorepo)

This file contains instructions for AI agents working in this Bazel-based monorepo.

## 1. Build, Test, and Lint Commands

**Crucial:** Use `bazel` for all operations. Do not use `cargo`, `npm`, `mvn`, or `go` directly unless specified.

### Core Workflows

- **Build Everything:** `bazel build //...`
- **Run All Tests:** `bazel test //...`
- **Format Code (All languages & BUILD files):** `bazel run format`
- **Lint Code (All languages):** `bazel lint`
- **Update BUILD files (Go, TS, Proto):** `bazel run gazelle` (Run this _before_ editing BUILD files manually)
- **Install NPM Deps:** `bazel run @pnpm -- --dir $PWD install`

### Running Single Tests

To run a specific test target, you first need to identify its label.

1.  **Find the target:** Look at the `BUILD.bazel` file in the directory where the test file resides.
2.  **Run the test:** `bazel test //path/to/package:target_name`

_Example (Nicknamer Rust):_ `bazel test //nicknamer/server/lib/tests:tests`
_Example (Mindreadr Kotlin):_ `bazel test //mindreadr/src/test/io/lowkeylab/mindreadr:MindreadrTest`

### Debugging Builds

- **Show all errors (don't stop at first fail):** `bazel build //... --keep_going`
- **Investigate specific failure:** `bazel build //path/to/target --verbose_failures`

## 2. Code Style & Conventions

Adhere strictly to these guidelines to ensure consistency across the monorepo.

### General

- **Formatting:** ALWAYS run `bazel run format` before finishing a task. This handles Rust (rustfmt), Kotlin (ktlint), Go (gofmt), JS/TS (prettier), and BUILD files (buildifier).
- **Filesystem:** Do not create new directories or files outside of the established project structure without good reason.
- **Bazel:**
  - Prefer existing libraries in `MODULE.bazel`.
  - If you add a file in Go, TS, or Proto, run `bazel run gazelle` immediately.
  - If you must manually edit a `BUILD.bazel` file, verify your changes with `bazel build //path/to/package/...`.

### Rust (Nicknamer)

- **Edition:** 2024
- **Error Handling:** Use `anyhow::Result` for apps/binaries, `thiserror` for libraries.
- **Async:** Use `tokio` runtime.
- **Database:** Use `SeaORM` entities and migrations (`nicknamer/migration`).
- **Testing:** Use `insta` for snapshot testing (`INSTA_UPDATE=always` to update).

### Kotlin (Mindreadr)

- **Style:** Ktor idiomatic.
- **Async:** Kotlin Coroutines.
- **Build:** Use `rules_kotlin`.
- **Testing:** JUnit 5.

### Go (Predix, Cowsay)

- **Style:** Standard `gofmt`.
- **Database:** Use `sqlc` for type-safe SQL (`predix/internal/sql`).
- **Context:** Always propagate `context.Context` as the first argument in functions performing I/O.

### TypeScript / Angular

- **Style:** Standard Angular style guide.
- **Strictness:** `strict: true` in `tsconfig.json`.
- **Linter:** ESLint (via `bazel lint`).

## 3. GitHub Copilot / Cursor Rules (Summary)

_Derived from `.github/copilot-instructions.md`_

- **Polyglot Monorepo:** Be aware you are in a mixed environment (Rust, Kotlin, Go, TS). Context switch appropriately.
- **Tool Management:** Bazel manages _everything_ (Node, Rust toolchain, JDK). Do not try to install system-level versions of these tools.
- **Secrets:** NEVER hardcode secrets. Use environment variables (e.g., `DB_URL`, `JWT_SECRET`).
- **Minimal Changes:** Only modify what is strictly necessary.
- **Safety:** Verify `bazel build //...` passes after changes.

## 4. Subproject Specifics

For detailed instructions on specific subprojects, refer to their local `AGENTS.md`:

- **Nicknamer (Rust):** `nicknamer/AGENTS.md`
- **Angular Apps:** `angular/AGENTS.md`

## 5. Troubleshooting Common Issues

- **"Bazel not found":** Ensure `bazelisk` is installed.
- **NPM deps missing:** `bazel run @pnpm -- --dir $PWD install`
- **Go deps missing:** `go get <pkg> && bazel mod tidy`
- **Build fails after update:** `bazel clean && bazel build //...`
