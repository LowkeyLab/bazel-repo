# Worktree pool

`worktree-pool` manages reusable Git worktrees from the terminal. Acquire a
worktree for a task, release it when finished, and reuse it for the next task.

## Features

- Reuse existing worktrees or create new ones as needed.
- Start work from `origin/main` or a chosen Git revision.
- Keep ignored files and build artifacts between tasks.
- Track assignments and inspect registered repositories and worktrees.
- Set a worktree limit for each repository.
- Preview and apply recovery for interrupted operations.
- Inspect resource usage and retire worktree registrations.
- Relocate or rebuild the catalog.
- Use JSON output for scripts and agent workflows.

## Installation

Supports Linux x86-64 with Git 2.36+. Install from an extracted source package
using Rust/Cargo 1.96+:

```sh
cargo install --locked --path /path/to/worktree-pool-0.1.0
```

## Quick start

Initialize the pool, register a repository, and acquire a worktree:

```sh
worktree-pool catalog init
worktree-pool repo register /absolute/repository
worktree-pool acquire --repo /absolute/repository
```

Work in the returned path. Create a Git branch if your task needs one.
When finished, commit your changes and release the returned assignment handle:

```sh
worktree-pool release <assignment-handle>
```

Use `worktree-pool --help` to explore the commands.

## Further reading

See [CONTRIBUTING.md](CONTRIBUTING.md) for development, implementation details,
and the [command reference](CONTRIBUTING.md#detailed-command-behavior).
See [WIRE_SCHEMA.md](WIRE_SCHEMA.md) for the JSON interface.
