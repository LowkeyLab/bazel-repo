# Development workflow

This file owns the common command sequence.
Use the repository's Nix development shell: `nix develop --command <command>`.
Identify the applicable project directory before you create directories or change files.

## Required sequence

1. Edit the necessary source files in the applicable project.
2. Immediately after each source edit, run `bazel run //:gazelle`.
   This includes `.rs`, `.kt`, `.go`, `.ts`, `.js`, and `.proto` files.
   Run Gazelle before formatting or manual `BUILD` edits.
3. Run `aspect format --scope=all` before you commit or complete the task.
   Use `aspect format` for additional checks of changed files during development.
4. Identify test targets in the applicable `BUILD.bazel` file.
   Run applicable tests with `aspect test //path/to/tests`.
   Use `aspect test //...` when the task requires the full suite.
5. Test locally with the applicable subproject's run commands.
6. Run `aspect build //...` to verify the full repository.
7. Run `aspect lint`.
   Correct lint failures before you finish implementation.
8. Commit only after the tests pass and formatting completes.

Documentation-only edits do not require source generation.
The formatting, test, build, and lint requirements still apply.

## Focused tests

```bash
# Rust suite
aspect test //nicknamer/server/lib/tests:tests
# Rust filter
aspect test //nicknamer/server/lib/tests:tests --test_filter="test_name_pattern"
# Rust snapshots
INSTA_UPDATE=always aspect test //nicknamer/server/lib/tests:tests
# Go target
aspect test //predix/internal/domain/circle:circle_test
# Go filter
aspect test //predix/internal/domain/circle:circle_test --test_filter="^TestCircleCreation$"
# Kotlin JUnit 5
aspect test //mindreadr/src/test/io/lowkeylab/mindreadr:MindreadrTest
```

## `BUILD` files and dependencies

Run Gazelle before manual `BUILD` edits, even without a preceding source edit.
Use `# keep` for entries that Gazelle must preserve.
Use `bazel run //tools:buildifier` to format `BUILD` files.
Check a changed package with `aspect build //path/to/package/...`.

Inspect `MODULE.bazel` before you add dependencies.
Prefer its existing libraries.
Bazel manages Node.js, Rust, Java Development Kit, and Go toolchains.
Bazelisk selects the Bazel version from `.bazelversion`.

Install or update Node.js package dependencies from the repository root:

```bash
bazel run @pnpm -- --dir $PWD install
```

For missing Go dependencies, the existing explicit repair procedure permits:

```bash
go get <package>
bazel mod tidy
```

This exception does not authorize other direct Go operations.
For Predix schema changes, use the generation command in [go.md](go.md).

## Build diagnosis

```bash
aspect build //... --keep_going
aspect build //path/to/target --verbose_failures
# Clean only when the failure requires it.
bazel clean
aspect build //...
# If Bazel is unavailable, check its executable.
which bazel
```

The Bazel executable should resolve to Bazelisk.
For stale `BUILD` files, use the Gazelle step in the required sequence.
For stale Node.js package dependencies, use the dependency command in this file.

## Repository navigation

| Path                   | Responsibility                             |
| ---------------------- | ------------------------------------------ |
| `angular/`             | Mindreadr, Nicknamer, and Predix frontends |
| `cowsay/`              | Go demonstration service                   |
| `mindreadr/`           | Kotlin/Ktor backend                        |
| `nicknamer/server/`    | Rust/Axum server                           |
| `nicknamer/migration/` | SeaORM migrations                          |
| `predix/`              | Go backend                                 |
| `predix/internal/sql/` | SQL schema and generated queries           |
| `tools/`               | Formatters, linters, and toolchains        |
| `MODULE.bazel`         | External dependencies                      |
| `go.mod`, `go.sum`     | Go dependencies                            |
| `pnpm-lock.yaml`       | Node.js dependencies                       |
| `Cargo.lock`           | Rust dependencies                          |

Read the applicable project's `AGENTS.md` for local setup and run commands.
