# GitHub Copilot Instructions

This is a **polyglot monorepo** powered by **Bazel**, containing full-stack applications and services across multiple languages and frameworks.

## Repository Context

For comprehensive project documentation:

- **Root Documentation**: [AGENTS.md](../AGENTS.md) and [GEMINI.md](../GEMINI.md)
- **Nicknamer Server**: `nicknamer/AGENTS.md`
- **Angular Apps**: `angular/AGENTS.md`

## Core Projects

| Project                  | Type        | Stack                       | Location                      |
| :----------------------- | :---------- | :-------------------------- | :---------------------------- |
| **Mindreadr** (Backend)  | API Service | Kotlin, Ktor, Netty         | `mindreadr/`                  |
| **Mindreadr** (Frontend) | Web App     | Angular, TypeScript         | `angular/projects/mindreadr/` |
| **Nicknamer**            | API Service | Rust, Axum, SeaORM          | `nicknamer/`                  |
| **Personal Website**     | Static Site | Astro, Tailwind, TypeScript | `personal_website/`           |
| **Predix**               | API Service | Go, PostgreSQL, sqlc        | `predix/`                     |

## Key Technologies

- **Build System**: Bazel 8.4.2+ (managed via Bazelisk)
- **Package Management**:
  - **Bazel**: Manages workspace-level toolchains (Rust, Java/Kotlin, Node.js, Go)
  - **PNPM**: Node.js dependencies (via Bazel rules)
  - **Cargo**: Rust crates (via `MODULE.bazel`)
  - **Maven**: JVM dependencies (via `MODULE.bazel`)
  - **Go Modules**: Go dependencies (via `go.mod` and `MODULE.bazel`)
- **Languages**: Rust (2024 edition), Kotlin, TypeScript, Go, Python

## Essential Commands

**⚠️ Critical**: Always use Bazel commands over native tools (`cargo`, `npm`, `mvn`, `go`) unless specifically instructed by subproject documentation.

### Global Build & Test

```bash
# Build entire repository
bazel build //...

# Run all tests
bazel test //...

# Build with error details (debugging)
bazel build //... --keep_going
```

### Code Quality

```bash
# Format all code (Rust, Kotlin, JS/TS, BUILD files)
bazel run format

# Run all linters (via aspect_rules_lint)
bazel lint

# Format BUILD files only
bazel run //tools:buildifier
```

### Bazel Maintenance

```bash
# Generate/update BUILD files (Go, TypeScript, Proto)
# ⚠️ Run this BEFORE manually writing BUILD files
bazel run gazelle

# Add Go dependencies
go get <package> && bazel mod tidy

# Install/update NPM dependencies
bazel run @pnpm -- --dir $PWD install

# Run arbitrary PNPM commands
bazel run @pnpm -- <args>
```

## Project-Specific Commands

### Nicknamer (Rust)

```bash
# Run locally with PostgreSQL
bazel run //nicknamer:run_locally

# Build specific target
bazel build //nicknamer/server/bin

# Run specific tests
bazel test //nicknamer/server/lib/tests:tests

# Update snapshot tests
INSTA_UPDATE=always bazel test //nicknamer/server/lib/tests:tests

# Build Docker image
bazel build //nicknamer:build_image
```

### Mindreadr (Kotlin)

```bash
# Run server
bazel run //mindreadr/src/main/io/lowkeylab/mindreadr/app:Application

# Watch mode (auto-rebuild)
ibazel run //mindreadr/src/main/io/lowkeylab/mindreadr/app:Application

# Run tests
bazel test //mindreadr/...
```

### Predix (Go)

```bash
# Build all targets
bazel build //predix/...

# Run tests
bazel test //predix/...

# Regenerate sqlc code (after schema changes)
cd predix && sqlc generate
```

### Angular Apps

```bash
# Serve Mindreadr frontend
ng serve --project mindreadr

# Note: Run `bazel run @pnpm -- --dir $PWD install` if node_modules is out of sync
```

### Cowsay (Go Demo)

```bash
# Run server
bazel run //cowsay/cmd/hello:hello
```

## Coding Standards

### Language-Specific Guidelines

#### Rust (Nicknamer)

- Use Rust 2024 edition
- Follow rustfmt formatting standards
- Use `anyhow::Result` for error handling
- Use `tracing` for structured logging
- Prefer async/await patterns with Tokio runtime

#### Kotlin (Mindreadr)

- Follow ktlint standards (enforced via Bazel)
- Use Ktor framework patterns
- Use coroutines for async operations

#### Go (Predix, Cowsay)

- Follow standard Go formatting (gofmt)
- Use sqlc for database code generation
- Use context.Context for request scoping

#### TypeScript/JavaScript (Angular, Astro)

- Use ESLint configuration (enforced via Bazel)
- Follow Angular style guide for Angular projects
- Use strict TypeScript settings

### General Conventions

- Keep dependencies minimal and well-justified
- Always format code before committing: `bazel run format`
- Write tests for new functionality
- Use existing libraries when available
- Add `# keep` comments to BUILD file lines that shouldn't be modified by Gazelle

## Repository Structure

```
bazel-repo/
├── 3rdparty/           # Vendored code and patches
├── angular/            # Angular workspace (Mindreadr frontend, etc.)
├── bzl/                # Custom Bazel Starlark macros
├── cowsay/             # Go demo service
├── ktor_tutorial/      # Kotlin/Ktor demo
├── mindreadr/          # Kotlin backend service
├── nicknamer/          # Rust backend service
│   ├── migration/      # Database migrations (SeaORM)
│   └── server/         # Server implementation
├── personal_website/   # Astro static site
├── predix/             # Go backend service
│   └── internal/sql/   # Database schema and queries (sqlc)
├── tools/              # Shared toolchains and configurations
│   ├── format/         # Formatter configurations
│   └── lint/           # Linter definitions (aspect_rules_lint)
├── MODULE.bazel        # Bazel external dependencies
├── AGENTS.md           # Root agent instructions
└── GEMINI.md           # Comprehensive context guide
```

Key Files:

- `MODULE.bazel`: External dependencies (Rust crates, Maven, Go modules, npm packages)
- `BUILD.bazel`: Build rules for each package
- `.bazelversion`: Bazel version (managed by Bazelisk)
- `go.mod`, `go.sum`: Go dependencies
- `pnpm-lock.yaml`: Node.js dependencies
- `sqlc.yaml`: sqlc configuration (in relevant projects)

## Development Workflow

1. **Make code changes** in the appropriate project directory
2. **Run Gazelle** if adding Go, TypeScript, or Proto files: `bazel run gazelle`
3. **Format code**: `bazel run format` (formats all languages)
4. **Run tests**: `bazel test //path/to/tests` or `bazel test //...`
5. **Test locally**: Use project-specific run commands (e.g., `bazel run //nicknamer:run_locally`)
6. **Verify build**: `bazel build //...` (or use `--keep_going` to see all errors)
7. **Run linters**: `bazel lint` (optional but recommended)
8. **Commit changes** after ensuring tests pass and code is formatted

## Tooling & Linters

The repository uses `aspect_rules_lint` for code quality:

- **Kotlin**: ktlint
- **Java**: pmd, checkstyle
- **TypeScript/JS**: eslint
- **Python**: ruff
- **Shell**: shellcheck
- **Protobuf**: buf
- **Go**: nogo
- **Rust**: rustfmt, clippy (via Bazel)

All linters can be run with: `bazel lint`

## Important Notes

- **All tools are Bazel-managed**: Node.js, PNPM, Rust toolchains, Java/Kotlin, Go, etc. Do not install separately.
- **Bazelisk manages Bazel**: Automatically uses the correct version from `.bazelversion`
- **Use Bazel commands first**: Prefer `bazel build/test/run` over native tools (`cargo`, `npm`, `mvn`)
- **Gazelle for BUILD files**: Run `bazel run gazelle` before manually editing BUILD files for Go/TS/Proto
- **Database migrations**:
  - Nicknamer: SeaORM migrations in `nicknamer/migration/`
  - Predix: SQL schema in `predix/internal/sql/schema.sql`, use sqlc for code generation
- **Snapshot tests**: Use `INSTA_UPDATE=always` to update Rust snapshot tests
- **Debug builds**: Use `--keep_going` to see all errors: `bazel build //... --keep_going`
- **Git commands**: Disable pagers to avoid timeouts: `git --no-pager <command>`
- **Watch mode**: Use `ibazel` for auto-rebuild on file changes (Kotlin projects)

## Common Issues & Solutions

### Build Failures

```bash
# Clean and retry
bazel clean
bazel build //...

# See all errors
bazel build //... --keep_going
```

### Missing Dependencies

```bash
# NPM dependencies out of sync
bazel run @pnpm -- --dir $PWD install

# Go dependencies
go get <package> && bazel mod tidy

# Regenerate BUILD files
bazel run gazelle
```

### Toolchain Issues

```bash
# Ensure Bazelisk is on PATH
which bazel  # Should point to bazelisk

# Check Bazel version
bazel version
```

## When Making Changes

- **Formatting:** Always run `bazel run format` to format all code before sharing or committing changes.
- **Minimal changes**: Only modify what's necessary to address the issue
- **Follow conventions**: Check existing patterns in similar code
- **Use Bazel**: Prefer Bazel commands over native tooling
- **Test thoroughly**: Run relevant tests after each significant change
- **Update tests**: Modify or add tests when behavior changes
- **Verify builds**: Ensure `bazel build //...` succeeds
- **Update BUILD files**: Always run `bazel run gazelle` after adding or moving code to regenerate BUILD files before manual edits
- **Keep comments**: Add `# keep` to BUILD file lines that shouldn't be auto-modified

## Security Best Practices

- Never hardcode credentials, secrets, or API keys
- Use environment variables for sensitive configuration
- Validate all user inputs
- Use secure defaults
- Follow project-specific security requirements (see subproject AGENTS.md files)

### Project-Specific Environment Variables

**Nicknamer**:

- `DB_URL`: PostgreSQL connection string
- `ADMIN_USERNAME`, `ADMIN_PASSWORD`: Admin credentials
- `JWT_SECRET`: JWT signing secret

## Additional Resources

- **Root Documentation**: `AGENTS.md`, `GEMINI.md`, `README.md`
- **Subproject Guides**: Check `AGENTS.md` in each project directory
- **Build Rules**: Review `BUILD.bazel` files to see available targets
- **Dependencies**: Check `MODULE.bazel` for external dependencies
- **Bazel Docs**: https://bazel.build/docs
