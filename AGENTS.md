# Repository instructions

## Required instructions

Use the `asd-ste100` skill in strict mode for all English.
Preserve exact code, commands, identifiers, quotations, and technical meaning.
Keep strict mode active when another writing style applies.

Follow the inherited worktree-pool instructions for editing tasks.
Those instructions retain authority over ownership, receipts, handoffs, capacity, release, and recovery.
This repository does not replace them.

Use Bazel for all operations through `bazel` or `aspect`.
Do not use `cargo`, `npm`, `mvn`, or `go` directly unless explicitly instructed.
Bazel manages the language toolchains. Do not install them separately.

## Development and review

Before edits or validation, read [development.md](docs/agents/development.md).
It owns the required Gazelle, formatting, test, build, lint, and commit sequence.
Before code changes or reviews, read [CODING_STANDARDS.md](CODING_STANDARDS.md).

This repository authorizes BuildBuddy use at `remote.buildbuddy.io` without separate approval.
This includes private source inputs, build artifacts, test artifacts, cache data, and build events.
For local-only checks, clear remote execution, cache, and build event service endpoints.

## Conditional references

Read the applicable reference before you edit or review its language or subsystem:

- Rust: [Rust standards](docs/agents/rust.md).
- Kotlin: [Kotlin standards](docs/agents/kotlin.md).
- Go: [go.md](docs/agents/go.md).
- TypeScript or Angular: [typescript.md](docs/agents/typescript.md).
- `BUILD` files or dependency changes: [development.md](docs/agents/development.md#build-files-and-dependencies).
- Nicknamer setup, migrations, or deployment: [nicknamer/AGENTS.md](nicknamer/AGENTS.md).
- Angular scaffolding, serving, or project selection: [angular/AGENTS.md](angular/AGENTS.md).
- Mindreadr backend setup or serving: [mindreadr/AGENTS.md](mindreadr/AGENTS.md).

Before issue-tracker operations, read [issue-tracker.md](docs/agents/issue-tracker.md).
Issues and specifications live in GitHub Issues.
Before issue triage, read [triage-labels.md](docs/agents/triage-labels.md).
Use its five default labels.
Before domain exploration or architectural decisions, read [domain.md](docs/agents/domain.md).
Use its multi-context layout.
