# Gemini Workspace Context: `bazel-repo`

This document provides a comprehensive overview of the `bazel-repo` monorepo to guide AI-assisted development. It acts as the central hub for understanding the project structure, build system, and available workflows.

## Project Overview

This is a polyglot monorepo powered by **Bazel**, containing full-stack applications and services.

### Core Services & Applications

| Project                  | Type        | Stack                       | Location                      | Documentation                      |
| :----------------------- | :---------- | :-------------------------- | :---------------------------- | :--------------------------------- |
| **Mindreadr** (Backend)  | API Service | Kotlin, Ktor, Netty         | `mindreadr/`                  | `mindreadr/AGENTS.md`              |
| **Mindreadr** (Frontend) | Web App     | Angular, TypeScript         | `angular/projects/mindreadr/` | `angular/AGENTS.md`                |
| **Nicknamer**            | API Service | Rust, Axum, SeaORM          | `nicknamer/`                  | `nicknamer/README.md`              |
| **Personal Website**     | Static Site | Astro, Tailwind, TypeScript | `personal_website/`           | `personal_website/package.json`    |
| **Ktor Tutorial**        | Demo        | Kotlin, Ktor                | `ktor_tutorial/`              | `ktor_tutorial/src/Application.kt` |
| **Cowsay**               | Demo        | Go                          | `cowsay/cmd/hello/`           | `cowsay/cmd/hello/main.go`         |

### Key Technologies

- **Build System:** `Bazel` (v8+) via `Bazelisk`.
- **Package Management:**
  - **Bazel:** Manages workspace-level toolchains (Rust, Java/Kotlin, Node.js, Go).
  - **PNPM:** Manages Node.js dependencies (via `pnpm-lock.yaml` and Bazel rules).
  - **Cargo:** Manages Rust crates (via `MODULE.bazel` and `crates.bzl` mechanism).
  - **Maven:** Manages JVM dependencies (via `MODULE.bazel`).
  - **Go Modules:** Manages Go dependencies (via `go.mod` and `MODULE.bazel`).

## Building & Running

**Crucial:** Always prefer Bazel commands over native tool commands (like `cargo`, `npm`, `mvn`) unless specifically instructed otherwise by sub-project documentation.

### Global Commands

- **Build Entire Repo:**
  ```bash
  bazel build //...
  ```
- **Test Entire Repo:**
  ```bash
  bazel test //...
  ```
- **Format Code (All Languages):**
  ```bash
  bazel run format
  ```
- **Lint Code (All Languages):**
  ```bash
  bazel lint
  ```

### Project-Specific Workflows

#### Mindreadr (Kotlin Backend)

- **Run Server:** `bazel run //mindreadr/src/main/io/lowkeylab/mindreadr/app:Application`
- **Watch Mode:** `ibazel run //mindreadr/src/main/io/lowkeylab/mindreadr/app:Application`
- **Tests:** `bazel test //mindreadr/...`

#### Nicknamer (Rust Backend)

- **Run Locally:** `bazel run //nicknamer:run_locally`
- **Build Docker Image:** `bazel build //nicknamer:build_image`

#### Angular Apps (Frontend)

- **Serve:** `ng serve --project <project_name>` (e.g., `mindreadr`)
  - _Note: Ensure `pnpm install` is run if `node_modules` is out of sync._

#### Personal Website (Astro)

- **Dev Server:** `cd personal_website && pnpm dev` (or via Bazel if configured)

#### Cowsay (Go Demo)

- **Run Server:** `bazel run //cowsay/cmd/hello:hello`

## Development Conventions

### Code Quality & Linters

The project uses `aspect_rules_lint` to enforce quality. Key linters include:

- **Kotlin:** `ktlint` (Aspect: `ktlint`)
- **Java:** `pmd`, `checkstyle`
- **TypeScript/JS:** `eslint`
- **Python:** `ruff`
- **Shell:** `shellcheck`
- **Protobuf:** `buf`

### Git Hooks

Pre-commit hooks are configured in `.pre-commit-config.yaml`. They run automatically on commit but can be triggered manually:

```bash
pre-commit run --all-files
```

### Directory Map

- `3rdparty/`: Vendored code and patches (e.g., Swagger UI fixes).
- `angular/`: Angular workspace root.
- `bzl/`: Custom Bazel Starlark macros (`.bzl`).
- `tools/`: Shared toolchains (Rust, Java, Node) and linter configs.
  - `tools/lint/`: Central linter definitions.
  - `tools/format/`: Formatter configurations.
