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
4. Resolve focused test labels with the [focused-test guidance](#focused-tests).
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

Read the applicable project's README test instructions before selecting focused tests.
For Book Smartz web tests, use the [web app test instructions](../../book_smartz/web/README.md).
Confirm each selected label in the applicable `BUILD.bazel` file.
A source filename is not an executable Bazel label.
If the mapping remains unclear, resolve it with `bazel query` before execution.
For example, `book_smartz/web/catalog_test.rs` belongs to `//book_smartz/web:http_test`.

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

## Hosted review polling

Use the [shared Rust command](../../tools/hosted_review/README.md) to request
and poll hosted reviews for an exact PR head.
Retain its private request state across resumed runs.
An incomplete review needs another request only after more than one hour.
Keep the current head, review findings, and required CI checks as separate merge gates.

## Merge verification

Before an authorized merge, use the [merge verification command](../../tools/merge_verification/README.md).
Use its before result for the exact reviewed head and file list.
After merge, use the same command to verify the merged file contents.
The reference owns its command examples, evidence format, and acceptance policy.
Keep ownership, validation, source reviews, and authorized merge execution in the delivery workflow.

## Pool command output

Use the [pool receipt wrapper](../../tools/pool_receipt/README.md) for bounded pool output and complete private command receipts.
Read lossless path bytes from the selected record before using a checkout path.
Keep inherited ownership, capacity, and exact recovery approval rules in effect.

For scheduled implementation waits, use the single-observation `wake` action in
[hosted review polling](../../tools/hosted_review/README.md#scheduled-wakes).
Return its heartbeat decision when only unchanged external work remains.
Retain the same state directory and exact acquisition receipt across wakes.
