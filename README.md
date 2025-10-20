# Bazel Monorepo

A Bazel-based monorepo with Rust backend services and Angular frontend applications, using centralized dependency management.

## 📋 Table of Contents

- [Overview](#overview)
- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [NPM Projects](#-npm-projects)
- [Rust Projects](#-rust-projects)
- [Common Tasks](#common-tasks)
- [License](#license)

## 🎯 Overview

This monorepo uses Bazel for builds and supports:
- **Rust** - Backend services with Axum, SeaORM, and PostgreSQL


## 📦 Prerequisites

Install using [mise](https://mise.run):
```bash
curl https://mise.run | sh
mise install
```

Or install manually: **Bazel** 8.4.2+, **Node.js** 20.18.0, **PNPM** 9+, **Rust** 2024 edition, **Docker**

## 🏁 Quick Start

```bash
# Clone repository
git clone https://github.com/LowkeyLab/bazel-repo.git
cd bazel-repo

# Install dependencies
mise install
pnpm install

# Build all
bazel build //...

# Run tests
bazel test //...
```

---

## 🦀 Rust Projects

### Nicknamer Server

Full-featured web service for managing names with authentication and REST API.

**Location:** `nicknamer/`

**Commands:**
```bash
# Start server with PostgreSQL
bazel run //nicknamer:run_locally

# Build binary
bazel build //nicknamer/server/bin

# Run tests
bazel test //nicknamer/server/lib/tests:tests

# Update snapshot tests
INSTA_UPDATE=always bazel test //nicknamer/server/lib/tests:tests
```

**Tech Stack:** Axum, SeaORM, PostgreSQL, JWT, OpenAPI/Swagger, Askama Templates

**Environment Variables:**
- `DB_URL` - PostgreSQL connection string
- `ADMIN_USERNAME` - Admin credentials
- `ADMIN_PASSWORD` - Admin credentials
- `JWT_SECRET` - JWT token signing secret

📖 See [AGENTS.md](./AGENTS.md) for detailed documentation.

### Managing Rust Dependencies

**Modify dependencies:**
1. Edit `Cargo.toml` in the relevant crate
2. Regenerate Bazel files:
   ```bash
   CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index
   ```

---

## Common Tasks

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

# Verbose output
bazel test //... --test_output=all
```

### Docker Images

```bash
# Build Nicknamer image
bazel build //nicknamer/server/bin:image

# Push to registry
bazel run //nicknamer/server/bin:push_image
```

### Troubleshooting

```bash
# Clean build cache
bazel clean

# Repin Rust dependencies
CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index

# Verbose build output
bazel build //... --verbose_failures
```

## 📄 License

GNU Affero General Public License v3.0 (AGPLv3) - See [LICENSE](./LICENSE)

---

**Documentation:**
- [AGENTS.md](./AGENTS.md) - Comprehensive project overview
- [nicknamer/README.md](./nicknamer/README.md) - Nicknamer server details
