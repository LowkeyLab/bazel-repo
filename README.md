# Bazel Monorepo

Central Bazel-based monorepo with Rust backend services and Angular frontend applications. This root `README.md` focuses on shared workflows. Project-specific details live in their own guides.

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

If you use Nix + direnv, `direnv allow` enters a flake shell that provides the common local command-line tools (`bazel`, `bazelisk`, `aspect`, `buildifier`, `prettier`, `pnpm`, `go`, `java`, `starpls`, `pre-commit`, plus the `coverage` command). For now, the flake assumes `x86_64-linux`, and `aspect` comes from `LowkeyLab/nix`. Bazel still owns the build, test, format, and coverage behavior for the repo.

## Quick start

```bash
git clone https://github.com/LowkeyLab/bazel-repo.git
cd bazel-repo

# (Optional) enter the flake-based dev shell
direnv allow

# (Optional) install NPM deps when touching frontend code
bazel run @pnpm -- --dir $PWD install

# Build everything
aspect build //...

# Run all tests
aspect test //...
```

## Common commands

```bash
# Build everything
aspect build //...

# Run all tests
aspect test //...

# Format changed files
aspect format

# Format the entire repository
aspect format --scope=all

# Lint (Aspect CLI)
aspect lint

# Format only BUILD files
bazel run //tools:buildifier

# Keep going on failures for investigation
aspect build //... --keep_going

# Run arbitrary pnpm command
bazel run @pnpm -- <args>
```

### Prose lint

Vale checks first-party Markdown with the Google technical-writing rules and American English spelling. Errors and warnings block the existing lint workflow; suggestions are advisory. Generated and vendored documents are excluded.

Run the existing lint command, or explicitly validate all documents locally without filtering to changed lines:

```bash
aspect lint //...
aspect test //tools/lint:vale_test
```

The full-document test is tagged `manual` and does not run in CI.

Markdown belongs in a `filegroup` tagged `markdown`. Each participating Bazel package has a `:markdown` target; its recursive glob includes documents outside child packages. When adding a new package with Markdown, add its target to `//tools/lint:vale_test`.

Add accepted technical names to `tools/lint/vale/config/vocabularies/Repo/accept.txt`. Preserve proper names, quotations, and technical meaning with narrowly scoped, explained Vale exceptions. Use inline code for literal identifiers and commands.

Bazel supplies the pinned binary and styles; lint actions require no downloads.

### Update the Rust workspace lockfile

After changing workspace dependencies in `Cargo.toml`, update the root `Cargo.lock` using the Bazel-managed Rust toolchain:

```bash
bazel run //tools:cargo_lock
```

This runs `cargo update --workspace` from the repository root. With Nix, you can run it directly in the development shell:

```bash
nix develop --command bazel run //tools:cargo_lock
```

## Repo tooling

- Centralized toolchain definitions under `tools/` (Rust, Node.js/pnpm, Angular command-line tool wrappers, formatters, linters).
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
aspect build //... --verbose_failures
```

For service-specific environment variables, runtime instructions, or database setup, consult the respective project guide under Further Documentation below.

## License

GNU Affero General Public License v3.0 (AGPLv3). See `LICENSE`.

## Further documentation

- Monorepo reference: `AGENTS.md`
- Nicknamer service details: `nicknamer/README.md`
- Angular workflows: `angular/AGENTS.md`
