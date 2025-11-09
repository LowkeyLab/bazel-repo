# GitHub Copilot Instructions

This repository uses Bazel as the primary build system for a monorepo containing multiple applications, primarily focused on a Rust-based web service called Nicknamer Server.

## Repository Context

For detailed project documentation, refer to [AGENTS.md](../AGENTS.md) in the root directory.

## Key Technologies

- **Build System**: Bazel 8.4.2 (managed via Bazelisk)
- **Languages**: Rust (2024 edition), TypeScript/JavaScript (Node.js 20.18.0)
- **Web Framework**: Axum
- **Database**: PostgreSQL with SeaORM
- **Package Manager**: PNPM (managed via Bazel)
- **Frontend**: Angular

## Essential Commands

### Building

```bash
# Build everything
bazel build //...

# Build specific target
bazel build //nicknamer/server/bin
```

### Testing

```bash
# Run all tests
bazel test //...

# Run specific test suite
bazel test //nicknamer/server/lib/tests:tests

# Update snapshot tests (when making intentional changes)
INSTA_UPDATE=always bazel test //nicknamer/server/lib/tests:tests
```

### Formatting

```bash
# Format all code (Rust, BUILD files, etc.)
bazel run format

# Format BUILD files only
bazel run //tools:buildifier
```

### Development

```bash
# Start local development environment (PostgreSQL + server)
bazel run //nicknamer:run_locally

# Install NPM dependencies
bazel run @pnpm -- --dir $PWD install
```

## Coding Standards

### Rust

- Use Rust 2024 edition
- Follow rustfmt formatting standards
- Use `anyhow::Result` for error handling
- Use `tracing` for structured logging
- Prefer async/await patterns with Tokio runtime

### General

- Keep dependencies minimal and well-justified
- Always format code before committing: `bazel run format`
- Write tests for new functionality
- Use existing libraries when available

## File Organization

- `nicknamer/server/bin/` - Main executable
- `nicknamer/server/lib/` - Library code
- `nicknamer/migration/` - Database migrations
- `3rdparty/` - Third-party dependencies
- `tools/` - Development tooling (managed via Bazel)
- `angular/` - Angular frontend application

## Development Workflow

1. Make code changes
2. Format code: `bazel run format`
3. Run relevant tests: `bazel test //path/to/tests`
4. Test locally if needed: `bazel run //nicknamer:run_locally`
5. Ensure all tests pass before committing

## Important Notes

- Tools like Node.js, PNPM, and Rust are managed by Bazel - do not install separately
- Bazelisk automatically manages the correct Bazel version
- Database migrations are in `nicknamer/migration/` using SeaORM
- Tests include snapshot tests with the `insta` crate
- Use `--keep_going` flag for debugging build issues: `bazel build //... --keep_going`
- Disable pagers in git commands to avoid timeouts: `git --no-pager <command>`

## When Making Changes

- **Minimal changes**: Only modify what's necessary to address the issue
- **Test early and often**: Run tests after each significant change
- **Format before commit**: Always run `bazel run format`
- **Verify builds**: Ensure `bazel build //...` succeeds
- **Check existing patterns**: Look at similar code before implementing new features
- **Update tests**: If behavior changes, update or add tests accordingly

## Security Considerations

- Never hardcode credentials or secrets
- Use environment variables for sensitive configuration
- Validate all user inputs
- JWT secrets must be provided via environment variables
- Required env vars: `DB_URL`, `ADMIN_USERNAME`, `ADMIN_PASSWORD`, `JWT_SECRET`

## Getting Help

- Refer to `AGENTS.md` for comprehensive documentation
- Check `README.md` for project overview
- Look at existing code for patterns and examples
- Build files (`BUILD.bazel`) show available targets
- `MODULE.bazel` defines external dependencies
