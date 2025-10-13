# Bazel Monorepo

A Bazel-based monorepo containing multiple applications with centralized dependency management using PNPM workspaces.

## 📋 Table of Contents

- [Overview](#overview)
- [Projects](#projects)
- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Workspace Structure](#workspace-structure)
- [Development](#development)
- [Build System](#build-system)
- [Testing](#testing)
- [Deployment](#deployment)
- [Contributing](#contributing)
- [License](#license)

## 🎯 Overview

This monorepo demonstrates modern application development using Bazel for builds, PNPM workspaces for dependency management, and supports multiple technology stacks including:

- **Rust** - Backend services with Axum, SeaORM, and PostgreSQL
- **Angular** - Frontend applications with Tailwind CSS and modern tooling
- **Docker** - Containerized deployments

## 🚀 Projects

### Nicknamer Server

A full-featured web service for managing names with authentication and REST API.

**Tech Stack:**
- Axum (Web framework)
- SeaORM (Database ORM)
- PostgreSQL (Database)
- JWT Authentication
- OpenAPI/Swagger Documentation
- Askama Templates

**Location:** `nicknamer/`

**Quick Start:**
```bash
# Start server with PostgreSQL in Docker
bazel run //nicknamer:run_locally

# Build server binary
bazel build //nicknamer/server/bin

# Run tests
bazel test //nicknamer/server/lib/tests:tests
```

See [AGENTS.md](./AGENTS.md) for detailed documentation.

### Angular Test Application

A modern Angular 19 application demonstrating Bazel integration with Tailwind CSS.

**Tech Stack:**
- Angular 19 (Standalone components)
- Tailwind CSS 3.4
- esbuild (Fast bundling)
- TypeScript 5.6
- Karma (Testing)

**Location:** `angular-test/`

**Quick Start:**
```bash
# Build production bundle
bazel build //angular-test:app

# Start development server
bazel run //angular-test:serve

# Run tests
bazel test //angular-test:test
```

See [angular-test/README.md](./angular-test/README.md) for detailed documentation.

## 📦 Prerequisites

### Required Tools

Install using [mise](https://mise.run) (recommended):

```bash
# Install mise
curl https://mise.run | sh

# Install all project dependencies
mise install
```

Or install manually:
- **Bazel** 8.4.2+
- **Node.js** 20.18.0
- **PNPM** 9+
- **Rust** 2024 edition
- **Docker** (for local development)

## 🏁 Quick Start

### 1. Clone the Repository

```bash
git clone https://github.com/LowkeyLab/bazel-repo.git
cd bazel-repo
```

### 2. Install Dependencies

```bash
# Using mise (recommended)
mise install

# Install Node.js dependencies
pnpm install
```

### 3. Build Everything

```bash
# Build all projects
bazel build //...

# Or build specific projects
bazel build //nicknamer/server/bin
bazel build //angular-test:app
```

### 4. Run Tests

```bash
# Run all tests
bazel test //...

# Or test specific projects
bazel test //nicknamer/server/lib/tests:tests
bazel test //angular-test:test
```

## 📁 Workspace Structure

```
bazel-repo/
├── MODULE.bazel                  # Bazel module definition
├── BUILD.bazel                   # Root build file
├── defs.bzl                      # Custom build macros
├── pnpm-workspace.yaml          # PNPM workspace configuration
├── package.json                  # Root package with all dependencies
├── pnpm-lock.yaml               # Centralized lockfile
├── .npmrc                        # PNPM configuration
├── mise.toml                     # Tool version manager config
├── README.md                     # This file
├── AGENTS.md                     # Detailed project documentation
├── LICENSE                       # AGPLv3 license
│
├── nicknamer/                    # Rust web service
│   ├── server/                   # Server implementation
│   │   ├── bin/                  # Binary target
│   │   └── lib/                  # Library code & templates
│   └── migration/                # Database migrations
│
├── angular-test/                 # Angular application
│   ├── BUILD.bazel              # Build configuration
│   ├── app/                     # Application source
│   └── README.md                # Angular-specific docs
│
├── 3rdparty/                     # Third-party dependencies
│   ├── swagger_ui/              # Swagger UI integration
│   └── utoipa_swagger_ui/       # Rust OpenAPI integration
│
└── tools/                        # Build tooling
    ├── angular/                  # Angular build tools
    ├── bazel/                    # Bazel utilities
    └── scripts/                  # Helper scripts
```

## 💻 Development

### PNPM Workspace Setup

This repository uses PNPM workspaces for centralized Node.js dependency management, following the patterns from [Aspect Build's bazel-examples](https://github.com/aspect-build/bazel-examples/tree/main/angular-ngc).

#### Key Concepts

1. **Centralized Dependencies**: All npm dependencies declared once in the root `package.json`
2. **Single Lockfile**: One `pnpm-lock.yaml` at the root for the entire monorepo
3. **Workspace Support**: Multiple packages share dependencies efficiently
4. **Bazel Integration**: Seamless integration with `@aspect_rules_js`

#### Workspace Configuration

**`pnpm-workspace.yaml`:**
```yaml
packages:
  - 'angular-test'
  # Add more workspace packages here
```

**Root `package.json`:**
- Contains all shared dependencies
- Includes `pnpm.onlyBuiltDependencies` for native modules
- References workspace packages

**Individual `package.json` files:**
- Simplified to only include package metadata and scripts
- No dependencies (inherited from root)
- Named with scoped package names (e.g., `@bazel-repo/angular-test`)

#### Module.bazel Configuration

```starlark
pnpm = use_extension("@aspect_rules_js//npm:extensions.bzl", "pnpm", dev_dependency = True)
use_repo(pnpm, "pnpm")

npm = use_extension("@aspect_rules_js//npm:extensions.bzl", "npm", dev_dependency = True)
npm.npm_translate_lock(
    name = "npm",
    pnpm_lock = "//:pnpm-lock.yaml",
    npmrc = "//:.npmrc",
    verify_node_modules_ignored = "//:.bazelignore",
)
use_repo(npm, "npm")
```

#### `.npmrc` Configuration

```
public-hoist-pattern[]=
node-linker=hoisted
ignore-scripts=false
```

### Managing Dependencies

#### Adding Node.js Dependencies

Add to the root `package.json`:
```bash
pnpm add <package-name> -w
```

The `-w` flag indicates installing at workspace root.

#### Adding Rust Dependencies

Modify the `Cargo.toml` in the relevant crate and regenerate Bazel files:
```bash
CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index
```

### Adding a New Workspace Package

1. Create a new directory (e.g., `new-package/`)
2. Add it to `pnpm-workspace.yaml`:
   ```yaml
   packages:
     - 'angular-test'
     - 'new-package'
   ```
3. Create `new-package/package.json` with minimal metadata:
   ```json
   {
     "name": "@bazel-repo/new-package",
     "version": "0.0.0",
     "private": true
   }
   ```
4. Add dependencies to root `package.json`
5. Create `new-package/BUILD.bazel` with `npm_link_all_packages()`
6. Run `pnpm install` to update lockfile

## 🔧 Build System

This project uses **Bazel** as the primary build system with the following advantages:

- **Fast Incremental Builds**: Only rebuilds what changed
- **Distributed Caching**: Share build artifacts across team
- **Hermetic Builds**: Reproducible builds on any machine
- **Multi-Language Support**: Rust, TypeScript, JavaScript, Docker
- **Dependency Management**: Precise dependency tracking

### Key Bazel Rules

- **Rust**: `rules_rust` for Rust compilation and dependencies
- **JavaScript/TypeScript**: `aspect_rules_js` and `aspect_rules_ts`
- **Bundling**: `aspect_rules_esbuild` for fast JavaScript bundling
- **Containers**: `rules_oci` for Docker image building
- **Shell**: `rules_shell` for shell scripts

### Common Bazel Commands

```bash
# Build all targets
bazel build //...

# Build specific target
bazel build //nicknamer/server/bin

# Run tests
bazel test //...

# Run a target
bazel run //angular-test:serve

# Query dependencies
bazel query 'deps(//angular-test:app)'

# Clean build artifacts
bazel clean

# Format BUILD files
bazel run //:buildifier
```

### Build Output

Build artifacts are generated in:
- `bazel-bin/` - Compiled binaries and bundles
- `bazel-out/` - Intermediate build outputs
- `bazel-testlogs/` - Test results and logs

## 🧪 Testing

### Running Tests

```bash
# Run all tests
bazel test //...

# Run specific test suite
bazel test //nicknamer/server/lib/tests:tests

# Run with verbose output
bazel test //... --test_output=all

# Run tests continuously (watch mode)
bazel test //... --watch
```

### Rust Tests

```bash
# Run Nicknamer tests
bazel test //nicknamer/server/lib/tests:tests

# Update snapshot tests
INSTA_UPDATE=always bazel test //nicknamer/server/lib/tests:tests

# Test with filter
bazel test //nicknamer/server/lib/tests:tests --test_filter="test_pattern"
```

### Angular Tests

```bash
# Run unit tests
bazel test //angular-test:test

# Run tests in watch mode
bazel run //angular-test:test.server
```

## 🚢 Deployment

### Building Docker Images

#### Nicknamer Server

```bash
# Build image
bazel build //nicknamer/server/bin:image

# Push to registry
bazel run //nicknamer/server/bin:push_image

# Run locally with Docker
bazel run //nicknamer:run_locally
```

#### Required Environment Variables

For the Nicknamer server:
- `DB_URL`: PostgreSQL connection string
- `ADMIN_USERNAME`: Admin credentials
- `ADMIN_PASSWORD`: Admin credentials
- `JWT_SECRET`: Secret for JWT token signing

### Production Deployment

1. Build production images:
   ```bash
   bazel build //nicknamer/server/bin:image
   bazel build //angular-test:app
   ```

2. Push to container registry:
   ```bash
   bazel run //nicknamer/server/bin:push_image
   ```

3. Deploy using your orchestration platform (Kubernetes, Docker Swarm, etc.)

## 🎨 Code Style

### Rust

- Use Rust 2024 edition
- Follow standard formatting: `rustfmt`
- Use `cargo clippy` recommendations
- Prefer explicit error handling with `anyhow::Result`
- Use structured logging with `tracing`

### TypeScript/Angular

- Follow Angular style guide
- Use TypeScript strict mode
- Prefer standalone components
- Use reactive patterns with RxJS
- Keep components focused and testable

### Build Files

- Format BUILD files: `bazel run //:buildifier`
- Use consistent naming conventions
- Document complex build rules
- Keep BUILD files close to source code

## 📚 References

### Bazel
- [Bazel Documentation](https://bazel.build/docs)
- [aspect_rules_js](https://docs.aspect.build/rulesets/aspect_rules_js/)
- [aspect_rules_ts](https://docs.aspect.build/rulesets/aspect_rules_ts/)
- [rules_rust](https://bazelbuild.github.io/rules_rust/)

### Frontend
- [Angular Documentation](https://angular.dev)
- [Tailwind CSS Documentation](https://tailwindcss.com/docs)
- [esbuild Documentation](https://esbuild.github.io/)

### Backend
- [Axum Documentation](https://docs.rs/axum)
- [SeaORM Documentation](https://www.sea-ql.org/SeaORM/)
- [Tokio Documentation](https://tokio.rs/)

### Dependency Management
- [PNPM Workspaces](https://pnpm.io/workspaces)
- [Aspect Build Examples](https://github.com/aspect-build/bazel-examples)

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes
4. Run tests: `bazel test //...`
5. Commit your changes: `git commit -am 'Add new feature'`
6. Push to the branch: `git push origin feature/my-feature`
7. Submit a pull request

### Development Workflow

1. Make code changes
2. Format code: `bazel run //:buildifier`
3. Run tests: `bazel test //...`
4. Test locally: `bazel run //nicknamer:run_locally`
5. Ensure all checks pass before committing

## 🐛 Troubleshooting

### Build Issues

```bash
# Clean build cache
bazel clean

# Re-install dependencies
pnpm install

# Check for Bazel issues
bazel info

# Verbose build output
bazel build //... --verbose_failures
```

### Docker Issues

```bash
# Check Docker daemon
docker ps

# Check PostgreSQL container
docker compose -f nicknamer/compose.yaml ps

# View container logs
docker compose -f nicknamer/compose.yaml logs
```

### Port Conflicts

```bash
# Find process using port
lsof -ti:8080 | xargs kill -9  # Adjust port as needed
```

### Dependency Issues

```bash
# Sync PNPM dependencies
pnpm install

# Repin Rust dependencies
CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index

# Clear PNPM cache
pnpm store prune
```

## 📄 License

This project is licensed under the GNU Affero General Public License v3.0 (AGPLv3).

See [LICENSE](./LICENSE) for the full license text.

Key points:
- You can freely use, modify, and distribute this software
- If you modify and run this on a network server, you must provide source code to users
- Any modifications must also be licensed under AGPLv3
- No warranty is provided

## 📞 Support

For detailed project documentation, see:
- [AGENTS.md](./AGENTS.md) - Comprehensive project overview
- [angular-test/README.md](./angular-test/README.md) - Angular app documentation
- [nicknamer/README.md](./nicknamer/README.md) - Nicknamer server documentation

---

**Built with ❤️ using Bazel, Rust, and Angular**
