# AGENTS.md (Monorepo)

This file contains instructions for AI agents working in this Bazel-based polyglot monorepo.

## 1. Build, Test, and Lint Commands

**CRITICAL:** Use `bazel` for ALL operations. Do not use `cargo`, `npm`, `mvn`, or `go` directly unless explicitly instructed.

### Core Workflows

```bash
# Build entire repository
aspect build //...

# Run all tests
aspect test //...

# Update BUILD files (MUST run immediately after ANY source file edit)
bazel run gazelle

# Format all code (Rust, Kotlin, Go, JS/TS, BUILD files)
format

# Run all linters
aspect lint

# Install/update NPM dependencies
bazel run @pnpm -- --dir $PWD install
```

### Running Single Tests

1. **Identify the test target** in the relevant `BUILD.bazel` file
2. **Run the specific test:**

```bash
# Rust (test suite)
aspect test //nicknamer/server/lib/tests:tests

# Rust (specific test with filter)
aspect test //nicknamer/server/lib/tests:tests --test_filter="test_name_pattern"

# Update Rust snapshot tests
INSTA_UPDATE=always aspect test //nicknamer/server/lib/tests:tests

# Go (single test target)
aspect test //predix/internal/domain/circle:circle_test

# Go (specific test function)
aspect test //predix/internal/domain/circle:circle_test --test_filter="^TestCircleCreation$"

# Kotlin (JUnit 5)
aspect test //mindreadr/src/test/io/lowkeylab/mindreadr:MindreadrTest
```

### Debugging Builds

```bash
# Show all errors (don't stop at first failure)
aspect build //... --keep_going

# Investigate specific failure with verbose output
aspect build //path/to/target --verbose_failures

# Clean and rebuild
bazel clean && aspect build //...
```

## 2. Code Style & Conventions

### General Rules

- **Gazelle:** MUST run `bazel run gazelle` immediately after editing ANY source file (.rs, .kt, .go, .ts, .js, .proto, etc.) and BEFORE running `format`
- **Formatting:** ALWAYS run `format` before committing or completing a task
- **Minimal changes:** Only modify what's strictly necessary
- **Dependencies:** Prefer existing libraries in `MODULE.bazel`
- **BUILD files:** Never manually edit BUILD files before running `bazel run gazelle`
- **Verification:** Run `aspect build //...` to verify changes don't break the build
- **Security:** NEVER hardcode secrets/credentials; use environment variables

### Rust (Nicknamer)

**Edition:** 2024

**Imports:**

- Group imports: `std`, external crates, internal modules
- Use explicit paths, avoid glob imports

**Types & Error Handling:**

- Use `anyhow::Result<T>` for applications/binaries
- Use `thiserror` for library error types
- Prefer `?` operator for error propagation

**Async & Runtime:**

- Use `tokio` runtime exclusively
- Prefer async/await patterns
- Use `tracing` for structured logging (not `println!` or `dbg!`)

**Naming Conventions:**

- `snake_case` for functions, variables, modules
- `PascalCase` for types, traits, enums
- `SCREAMING_SNAKE_CASE` for constants

**Database:**

- Use `SeaORM` entities and migrations
- Migrations in `nicknamer/migration/`

**Testing:**

- Use `insta` for snapshot testing
- Integration tests use `testcontainers` for PostgreSQL

### Kotlin (Mindreadr)

**Style:** Ktor framework patterns

**Imports:**

- Organized: wildcard (\*), java.**, javax.**, kotlin.\*\*, project imports (^)
- Configured in `.editorconfig`

**Formatting:**

- 2-space indentation
- Max line length: 200 characters
- Run `format` (uses ktlint)

**Types & Naming:**

- Use type inference when obvious
- `camelCase` for functions, variables, properties
- `PascalCase` for classes, interfaces, objects
- Avoid abbreviations

**Async:**

- Use Kotlin Coroutines for async operations
- Prefer `suspend` functions over callbacks

**Testing:**

- JUnit 5 framework
- Test files mirror source structure in `src/test/`

### Go (Predix, Cowsay)

**Style:** Standard Go conventions (`gofmt`)

**Imports:**

- Grouped: stdlib, external, internal
- Use `goimports` (via `format`)

**Types & Naming:**

- `camelCase` for unexported, `PascalCase` for exported
- Interfaces: `-er` suffix (e.g., `Reader`, `Handler`)
- Avoid stuttering (`user.UserService` → `user.Service`)

**Error Handling:**

- Return errors as last return value
- Check errors immediately: `if err != nil`
- Wrap errors with context: `fmt.Errorf("operation failed: %w", err)`

**Context:**

- ALWAYS propagate `context.Context` as first argument for I/O operations
- Use `ctx context.Context` parameter name

**Database:**

- Use `sqlc` for type-safe SQL code generation
- Schema in `predix/internal/sql/schema.sql`
- Run `cd predix && sqlc generate` after schema changes

**Testing:**

- Test files: `*_test.go` in same package
- Test functions: `func TestXxx(t *testing.T)`
- Use table-driven tests for multiple cases

### TypeScript / Angular

**Strictness:** All projects use `strict: true` in `tsconfig.json`

**Imports:**

- Organize: framework, third-party, internal
- Use absolute imports from `tsconfig.json` paths
- Avoid circular dependencies

**Types:**

- Use type inference when obvious
- Avoid `any`; use `unknown` if uncertain
- Prefer interfaces for object shapes
- Use `readonly` for immutable data

**Naming:**

- `camelCase` for functions, variables, properties
- `PascalCase` for classes, interfaces, types
- `SCREAMING_SNAKE_CASE` for constants

**Angular Conventions:**

- Use **standalone components** (no NgModules)
- Do NOT set `standalone: true` (it's the default)
- Use `inject()` over constructor injection
- Use signals for state: `input()`, `output()`, `signal()`, `computed()`
- Set `changeDetection: ChangeDetectionStrategy.OnPush`
- Native control flow: `@if`, `@for`, `@switch` (not `*ngIf`, `*ngFor`)
- Use `async` pipe for observables
- Prefer inline templates for small components

**Services:**

- Single responsibility
- Use `providedIn: 'root'` for singletons

**Formatting:**

- 2-space indentation (Prettier)
- Run `format`

### BUILD Files

**Style:** Run `bazel run //tools:buildifier` for formatting

**Conventions:**

- Use `# keep` comments for lines that shouldn't be auto-modified by Gazelle
- Run `bazel run gazelle` before manual edits to BUILD files
- Verify changes: `aspect build //path/to/package/...`

## 3. Repository Structure

```
bazel-repo/
├── angular/            # Angular workspace (Mindreadr, Nicknamer, Predix frontends)
├── cowsay/             # Go demo service
├── mindreadr/          # Kotlin/Ktor backend
├── nicknamer/          # Rust/Axum backend
│   ├── migration/      # SeaORM migrations
│   └── server/         # Server implementation
├── predix/             # Go backend
│   └── internal/sql/   # sqlc schema and queries
├── tools/              # Formatters, linters, toolchains
├── MODULE.bazel        # External dependencies
├── go.mod, go.sum      # Go dependencies
├── pnpm-lock.yaml      # Node.js dependencies
└── Cargo.lock          # Rust dependencies
```

## 4. Development Workflow

1. Make code changes in appropriate project directory
2. **Run Gazelle immediately:** `bazel run gazelle` (REQUIRED after ANY source file edit)
3. **Format code:** `format` (handles all languages)
4. **Run tests:** `aspect test //path/to/tests` or `aspect test //...`
5. **Test locally:** Use project-specific run commands (see subproject AGENTS.md)
6. **Verify build:** `aspect build //...` (use `--keep_going` to see all errors)
7. **Run linters** (optional but recommended): `aspect lint`
8. **Commit changes** after ensuring tests pass and code is formatted

## 5. Polyglot Environment Notes

- **Tool Management:** Bazel manages Node.js, Rust, JDK, Go toolchains—do NOT install separately
- **Bazelisk:** Manages Bazel version from `.bazelversion`
- **Context Switching:** Be aware of language-specific idioms (this is a multi-language repo)
- **Dependencies:** Check `MODULE.bazel` for available libraries before adding new ones

## 6. Subproject-Specific Guides

For detailed project documentation, see:

- **Nicknamer (Rust):** `nicknamer/AGENTS.md`
- **Angular Apps:** `angular/AGENTS.md`
- **Root Context:** `GEMINI.md` (comprehensive guide)
- **Copilot Instructions:** `.github/copilot-instructions.md`

## 7. Troubleshooting

```bash
# Build failures
bazel clean && aspect build //...
aspect build //... --keep_going  # See all errors

# NPM dependencies out of sync
bazel run @pnpm -- --dir $PWD install

# Go dependencies missing
go get <package> && bazel mod tidy

# BUILD files out of sync
bazel run gazelle

# Bazel not found
which bazel  # Should point to bazelisk

# Database migrations (Nicknamer)
bazel run //nicknamer/migration/bin -- up

# Regenerate sqlc code (Predix)
cd predix && sqlc generate
```

## 8. Common Pitfalls

- ❌ Using `cargo build/test` instead of `aspect build/test`
- ❌ Using `npm install` instead of `bazel run @pnpm -- --dir $PWD install`
- ❌ Forgetting to run `bazel run gazelle` immediately after editing source files
- ❌ Running `format` before running `bazel run gazelle`
- ❌ Editing BUILD files before running `bazel run gazelle`
- ❌ Forgetting to run `format` before committing
- ❌ Hardcoding secrets instead of using environment variables
- ❌ Creating new directories without understanding the project structure
