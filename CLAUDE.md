# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Setup

This repo uses a **Nix flake** for reproducible dev tooling. Nix provides `bazelisk`, `bazel`, `gcc`, and `pre-commit` — no manual installation needed.

### Prerequisites

- [Nix](https://nixos.org/download/) with flakes enabled (`experimental-features = nix-command flakes` in `~/.config/nix/nix.conf`)
- [direnv](https://direnv.net/) >= 2.29
- **NixOS only:** `programs.nix-ld.enable = true` in your NixOS configuration (needed for bazelisk to run downloaded Bazel binaries)

### Getting started

```bash
direnv allow   # Enters Nix dev shell + sources .envrc
```

The `.envrc` automatically sources [nix-direnv](https://github.com/nix-community/nix-direnv) 3.1.1 for cached flake evaluations. After the first `direnv allow`, subsequent shell entries are near-instant.

> **Note:** The Nix dev shell is not CI-validated. If `flake.nix` breaks, it will only be caught by developers using Nix. Bazel still works without Nix if you install `bazelisk` and `pre-commit` manually.

## Build System

This is a **Bazel 9 polyglot monorepo** managed by Bazelisk. Use `aspect` (a Bazel wrapper) for build/test/lint. Never invoke `cargo`, `npm`, `go`, `mvn`, `pnpm`, or `ng` directly for building — all operations go through Bazel.

## Essential Commands

### Running targets

Use `bazel run` (not `aspect run`) — the `aspect` wrapper only supports `build`, `test`, and `lint`.

```bash
# Build and test everything
aspect build //...
aspect test //...

# Lint all code
aspect lint //...

# Format all code (Rust, Kotlin, Go, JS/TS, BUILD files)
format

# Regenerate BUILD files after editing source files
bazel run gazelle

# Install/sync NPM deps
bazel run @pnpm -- --dir $PWD install
```

### Remote execution

CI uses `--config=ci-remote` for remote build execution via BuildBuddy (with `--noremote_local_fallback`). To test locally:

```bash
bazel build //... --config=remote-linux
bazel test //... --config=remote-linux
```

Note: Use `bazel` (not `aspect`) for remote execution — the Aspect CLI does not support `--config` flags.

### After editing source files, always:

1. `bazel run gazelle` — regenerates BUILD files (must run BEFORE format)
2. `format` — formats all code
3. `aspect build //...` — verify build

### Coverage

```bash
# Run coverage for all targets
bazel coverage //...

# Run coverage for a specific service
bazel coverage //nicknamer/...

# Generate HTML report (requires lcov/genhtml)
coverage

# Coverage for a specific service with HTML report
coverage //predix/...
```

### Running a single test

```bash
aspect test //nicknamer/server/lib/tests:tests --test_filter="test_name_pattern"
aspect test //predix/internal/domain/circle:circle_test --test_filter="^TestCircleCreation$"
```

### Updating insta snapshots (Rust)

```bash
INSTA_UPDATE=always aspect test //nicknamer/server/lib/tests:tests
```

### Dependency management

```bash
# Rust: edit Cargo.toml, then:
CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index

# Go:
go get <package> && bazel mod tidy

# NPM: edit pnpm-workspace.yaml or package.json, then:
bazel run @pnpm -- --dir $PWD install

# JVM (Kotlin/Java): edit MODULE.bazel maven artifacts, then:
REPIN=1 bazel run @maven//:pin
```

## Architecture

Polyglot monorepo with independent backend services and Angular frontends, all built with Bazel.

### Services

| Service              | Path                | Stack                                           | Notes                        |
| -------------------- | ------------------- | ----------------------------------------------- | ---------------------------- |
| **nicknamer**        | `nicknamer/`        | Rust, Axum, SeaORM, PostgreSQL                  | REST API with Swagger/utoipa |
| **nicknamer2**       | `nicknamer2/`       | Rust, Axum, Juniper (GraphQL), sqlx, PostgreSQL | Relay-style pagination       |
| **mindreadr**        | `mindreadr/`        | Kotlin, Ktor 3, Exposed, JVM 21                 |                              |
| **predix**           | `predix/`           | Go, Gin, pgx, sqlc                              |                              |
| **cowsay**           | `cowsay/`           | Go                                              | Demo service                 |
| **personal_website** | `personal_website/` | Astro 5, Caddy                                  | Static site                  |

### Frontends

Angular 21 projects live in `angular/projects/` (mindreadr, nicknamer, predix, tailwind-sample). Uses standalone components, signals, OnPush change detection, and native control flow (`@if`, `@for`).

### Shared build infrastructure

- `bzl/` — shared Bazel macros (e.g., `kotlin.bzl`)
- `tools/format/` — multi-language format runner
- `tools/lint/linters.bzl` — lint aspect definitions (clippy, ktlint, eslint, ruff, shellcheck, buf, pmd, stylelint, keep-sorted)
- `3rdparty/` — build patches for third-party deps
- `MODULE.bazel` — all external Bzlmod dependencies
- `Cargo.toml` — Rust workspace dependency catalog

### CI/CD

- **run-tests.yml**: on push/PR to main — runs `aspect lint //...` then `aspect test //...` with BuildBuddy remote cache
- **deploy.yml**: after tests pass on main — builds optimized and pushes all OCI images to GHCR

## Code Conventions

### Rust (edition 2024)

- `anyhow::Result<T>` for binaries; `thiserror` for library error types
- `tracing` for logging (not `println!`)
- Tokio async runtime

### Kotlin

- Ktor patterns with Kotlin Coroutines
- 2-space indent, max 200 char lines

### Go

- Propagate `context.Context` as first parameter
- Table-driven tests; `fmt.Errorf("operation: %w", err)` for error wrapping

### TypeScript/Angular

- `strict: true`; standalone components (do NOT set `standalone: true` explicitly — it's the default)
- Use `inject()` over constructor injection; signals (`input()`, `output()`, `signal()`, `computed()`)
- `ChangeDetectionStrategy.OnPush` on all components

### BUILD files

- Always run `bazel run gazelle` before manually editing
- Use `# keep` comments to prevent Gazelle from modifying specific lines
- Gazelle extensions: Go, Kotlin (contrib_rules_jvm), Rust (gazelle_rust), Skylib

### Worktree gotchas

- `format` (from `.envrc`) doesn't work in worktrees — use `bazel run //tools/format` instead
- `aspect test` doesn't support `--cache_test_results` or `--test_output` — use `bazel test` directly for those flags

## Tooling

- **pre-commit**: runs `format` and `buildifier-lint` (provided by the Nix dev shell). Install hooks with `pre-commit install`.
- **direnv/.envrc**: sources `.env`, sets PATH from `bazel-out`. Run `bazel run //tools:bazel_env` to populate.
- **ibazel**: incremental Bazel for hot reload (e.g., `ibazel run //personal_website:dev`)
- **rust-analyzer**: regenerate project with `bazel run //:gen_rust_project`
- **Renovate**: automated dependency updates

