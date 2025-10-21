# AGENTS.md

## Project Overview

This is a Bazel-based monorepo containing multiple applications:

### Nicknamer Server

A web service for managing names with authentication, built using Axum, SeaORM, and PostgreSQL.

Key components:

- `nicknamer/server/` - Main web server with REST API and web UI
- `nicknamer/migration/` - Database migration utilities using SeaORM
- `3rdparty/` - Third-party dependencies and build configurations
- `tools/` - Development tooling and scripts

## Setup Commands

### Prerequisites

1. **Install mise** (tool version manager):
   ```bash
   curl https://mise.run | sh
   ```
2. **Install Bazel** using mise:

   ```bash
   mise install
   ```

   This installs Bazel 8.4.2 as defined in `mise.toml`.

3. **Install NPM dependencies** (if working with frontend code):

   ```bash
   bazel run @pnpm -- --dir $PWD install
   ```

   **Note:** Development tools including Node.js, PNPM, and Rust are managed by Bazel through the `tools/` directory. They do not need to be installed separately via mise.

### Development Environment

- **Start local development with dependencies**:

  ```bash
  bazel run //nicknamer:run_locally
  ```

  This starts PostgreSQL in Docker and runs the server with proper environment variables.

- **Build the project**:

  ```bash
  bazel build //nicknamer/server/bin
  ```

- **Run tests**:

  ```bash
  bazel test //nicknamer/server/lib/tests:tests
  ```

- **Build Docker image**:
  ```bash
  bazel build //nicknamer/server/bin:image
  ```

## Build System

This project uses **Bazel** as the primary build system with:

- Rust toolchain (2024 edition)
- Node.js toolchain (20.18.0)
- PNPM package manager (managed via Bazel)
- Rules for Rust, OCI, Shell, packaging, and JavaScript/TypeScript
- Crate universe for Rust dependency management
- Container image building capabilities

Key Bazel concepts:

- `MODULE.bazel` - Main module definition with dependencies
- `BUILD.bazel` files - Build targets in each package
- `tools/` - Development tooling managed through Bazel
- `//nicknamer:run_locally` - Convenience target for local development

### Development Tools

Tools are managed centrally in the `tools/` directory through Bazel:

- **PNPM**: Run via `bazel run @pnpm -- <args>`
- **Angular CLI**: Run via `bazel run //tools:ng -- <args>`
- **Buildifier**: Run via `bazel run //tools:buildifier`

The `tools/BUILD.bazel` file defines available tools and their configurations.

## Testing Instructions

- **Run all tests**:

  ```bash
  bazel test //...
  ```

- **Update snapshot tests** (when making intentional changes):

  ```bash
  INSTA_UPDATE=always bazel test //nicknamer/server/lib/tests:tests
  ```

- **Test specific modules**:
  ```bash
  bazel test //nicknamer/server/lib/tests:tests --test_filter="test_name_pattern"
  ```

Test structure:

- Unit tests in `nicknamer/server/lib/tests/`
- Snapshot testing with `insta` crate
- Integration tests with `testcontainers` for database testing
- Tests run against PostgreSQL containers automatically

## Code Style Guidelines

### Rust Standards

- Use Rust 2024 edition
- Follow standard Rust formatting (rustfmt)
- Use `cargo clippy` recommendations
- Prefer explicit error handling with `anyhow::Result`
- Use structured logging with `tracing`

### Formatting

Format all code using the centralized formatting tool:

```bash
bazel run //tools/format
```

This will format Rust code, BUILD files, and other supported file types.

### Architecture Patterns

- **Web layer**: Axum for HTTP handling
- **Database**: SeaORM for type-safe database interactions
- **Templates**: Askama for HTML templating
- **Authentication**: JWT tokens with bearer auth
- **API Documentation**: utoipa for OpenAPI specs

### Dependencies

- Keep dependencies minimal and well-justified
- Use workspace dependencies defined in `MODULE.bazel`
- Prefer async/await patterns with Tokio runtime

## Database

- **Database**: PostgreSQL
- **ORM**: SeaORM with migrations
- **Migrations**: Located in `nicknamer/migration/`
- **Local development**: Uses Docker Compose (`nicknamer/compose.yaml`)

Migration commands:

```bash
# Run migrations (done automatically by run_locally.sh)
bazel run //nicknamer/migration/bin -- up

# Create new migration
sea-orm-cli migrate generate <migration_name>
```

## Security Considerations

- JWT secrets must be provided via environment variables
- Database credentials should never be hardcoded
- Admin credentials are configurable via environment variables
- Use HTTPS in production deployments
- Validate all user inputs through proper deserialization

Required environment variables for local development:

- `DB_URL`: PostgreSQL connection string
- `ADMIN_USERNAME`: Admin user credentials
- `ADMIN_PASSWORD`: Admin user credentials
- `JWT_SECRET`: Secret for JWT token signing

## Docker and Deployment

- **Base image**: Google's distroless/cc-debian12
- **Build**: `bazel build //nicknamer/server/bin:image`
- **Push to registry**: `bazel run //nicknamer/server/bin:push_image`
- **Local run**: `bazel run //nicknamer:run_locally`

The application serves on port 8080 by default.

## Development Workflow

1. Make code changes
2. Format code: `bazel run //tools/format`
3. Run tests: `bazel test //nicknamer/server/lib/tests:tests`
4. Test locally: `bazel run //nicknamer:run_locally`
5. Build image: `bazel build //nicknamer/server/bin:image`
6. Ensure all tests pass before committing

## Common Commands

```bash
# Quick development cycle
bazel run //nicknamer:run_locally

# Run tests
bazel test //...

# Build everything
bazel build //...

# Format all code
bazel run //tools/format

# Format BUILD files only
bazel run //tools:buildifier

# Check for build issues
bazel build //... --keep_going

# Install NPM dependencies
bazel run @pnpm -- --dir $PWD install

# Run pnpm commands
bazel run @pnpm -- <args>
```

## Troubleshooting

- **Docker issues**: Ensure Docker daemon is running and accessible
- **Database connection**: Check PostgreSQL container is healthy via `docker compose ps`
- **Build failures**: Run `bazel clean` and retry
- **Missing Bazel**: Run `mise install` to ensure Bazel is installed
- **NPM dependencies**: Run `bazel run @pnpm -- --dir $PWD install` to reinstall packages
- **Port conflicts**: Check if port 8080 or 5432 are already in use
- **Tool issues**: Development tools (Node.js, PNPM, Rust) are managed by Bazel; no manual installation needed

## File Structure Notes

- Server code is split between `bin/` (main executable) and `lib/` (library code)
- Templates are in `server/lib/templates/` using Askama
- API routes are organized by version (`api/v1.rs`)
- Database entities are in `entities/` module
- Tests include snapshot files for consistent API response testing
