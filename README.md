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
- **NPM** - Angular applications with PNPM workspaces
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

## 📦 NPM Projects

All NPM projects are self-contained with their own dependencies and tooling.

### Angular Test Application

Modern Angular 20 application with Tailwind CSS, demonstrating a standalone Angular project integrated with Bazel.

**Location:** `angular-ngc/`

**Setup:**
```bash
cd angular-ngc
pnpm install
```

**Commands:**
```bash
# Build production bundle
bazel build //angular-ngc/applications/demo:app

# Start dev server
bazel run //angular-ngc/applications/demo:serve

# Run tests
bazel test //angular-ngc/applications/demo:test
```

**Tech Stack:** Angular 20, Tailwind CSS 4, esbuild, TypeScript 5.8

**Key Features:**
- Self-contained project with own `package.json` and `pnpm-workspace.yaml`
- Custom Bazel build rules in `angular-ngc/tools/`
- Demo application in `angular-ngc/applications/demo/`
- TypeScript configurations in `angular-ngc/tsconfig*.json`

📖 See [angular-ngc/README.md](./angular-ngc/README.md) for details.

### Managing NPM Dependencies

Each NPM project manages its own dependencies independently.

**For Angular project:**
```bash
cd angular-ngc
pnpm add <package-name>
pnpm install
```

**Adding a new NPM project:**
1. Create project directory: `new-project/`
2. Create `new-project/package.json`:
   ```json
   {
     "name": "@bazel-repo/new-project",
     "version": "0.0.0",
     "private": true
   }
   ```
3. Create `new-project/pnpm-workspace.yaml` if needed
4. Create `new-project/BUILD.bazel` with build rules
5. Run `pnpm install` in the project directory

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
bazel build //angular-ngc/applications/demo:app
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

# Re-install NPM dependencies (for Angular project)
cd angular-ngc
pnpm install

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
- [angular-ngc/README.md](./angular-ngc/README.md) - Angular app details
- [nicknamer/README.md](./nicknamer/README.md) - Nicknamer server details
