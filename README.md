# Bazel Monorepo

Central Bazel-based monorepo with Rust backend services and Angular frontend applications. This root `README.md` focuses on shared workflows. Project-specific details live in their own guides.

## Subproject Guides

- Nicknamer Server: `nicknamer/README.md`
- Angular Apps: `angular/AGENTS.md`
- Monorepo Overview: `AGENTS.md`

## Prerequisites

Install [Bazelisk](https://github.com/bazelbuild/bazelisk) (manages Bazel versions):

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

Node.js, PNPM, Rust and other toolchains are provisioned via Bazel under `tools/`; do not install them manually.

## Quick Start

```bash
git clone https://github.com/LowkeyLab/bazel-repo.git
cd bazel-repo

# (Optional) install NPM deps when touching frontend code
bazel run @pnpm -- --dir $PWD install

# Build everything
bazel build //...

# Run all tests
bazel test //...
```

## Common Commands

```bash
# Build everything
bazel build //...

# Run all tests
bazel test //...

# Format code (Rust, BUILD files, etc.)
bazel run format

# Lint (Aspect CLI)
bazel lint

# Format only BUILD files
bazel run //tools:buildifier

# Keep going on failures for investigation
bazel build //... --keep_going

# Run arbitrary pnpm command
bazel run @pnpm -- <args>
```

## Repo Tooling

- Centralized toolchain definitions under `tools/` (Rust, Node.js/PNPM, Angular CLI wrappers, formatters, linters).
- Bazel manages reproducible builds and dependency pinning.
- Use `MODULE.bazel` / `Cargo.toml` edits followed by repin steps for dependency changes (see project guides).

## Troubleshooting

```bash
# Clean build cache
bazel clean

# Repin Rust dependencies
CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index

# Reinstall NPM dependencies
bazel run @pnpm -- --dir $PWD install

# Verbose failure output
bazel build //... --verbose_failures
```

For service-specific environment variables, runtime instructions, or database setup, consult the respective project guide listed above.

## License

GNU Affero General Public License v3.0 (AGPLv3). See `LICENSE`.

## Further Documentation

- Monorepo reference: `AGENTS.md`
- Nicknamer service details: `nicknamer/README.md`
- Angular workflows: `angular/AGENTS.md`
