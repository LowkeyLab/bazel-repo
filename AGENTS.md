# AGENTS.md (Monorepo)

This is a Bazel-based monorepo. Subproject-specific guides have been moved into their own `AGENTS.md` files. Use the links below for project-level workflows.

## Subproject Guides

- Nicknamer Server: see `nicknamer/AGENTS.md`
- Angular apps: see `angular/AGENTS.md`

## Prerequisites

1. Install Bazelisk (manages Bazel versions):

   Linux:

   ```bash
   sudo wget -O /usr/local/bin/bazel https://github.com/bazelbuild/bazelisk/releases/latest/download/bazelisk-linux-amd64
   sudo chmod +x /usr/local/bin/bazel
   ```

   macOS:

   ```bash
   brew install bazelisk
   ```

   Windows (PowerShell):

   ```powershell
   choco install bazelisk
   ```

   Bazelisk uses Bazel 8.4.2 per `.bazelversion`.

2. Install NPM dependencies (if working with frontend code):

   ```bash
   bazel run @pnpm -- --dir $PWD install
   ```

   Note: Node.js and PNPM are managed by Bazel in `tools/`.

## Common Commands

```bash
# Build everything
bazel build //...

# Run all tests
bazel test //...

# Format all code (Rust, BUILD files, etc.)
bazel run format

# Run linters (Aspect CLI)
bazel lint

# Format only BUILD files
bazel run //tools:buildifier

# Investigate build issues (keep going)
bazel build //... --keep_going

# Run arbitrary pnpm commands
bazel run @pnpm -- <args>
```

## Repo Tooling

- Tools are managed centrally under `tools/` via Bazel.
- See each subproject guide for language/framework-specific tooling.

## Troubleshooting (General)

- Build failures: run `bazel clean` and retry
- Missing Bazel: ensure Bazelisk is installed and on PATH
- NPM deps: `bazel run @pnpm -- --dir $PWD install`
- For database/Docker issues, see the relevant subproject guide
