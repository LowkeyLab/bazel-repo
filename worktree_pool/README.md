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

<!-- vale Google.Headings = NO -->

## Why Pooled Worktrees

<!-- vale Google.Headings = YES -->

Pooled worktrees can get you from starting a task to a finished build sooner.
In 100 comparisons using a Rust/Bazel build in this repository, repeating a build
or making a small edit took about half the time with pooled worktrees.

These are median times from requesting a worktree to finishing the build:

| Task                                       | New worktree each time | Pooled worktree |
| ------------------------------------------ | ---------------------- | --------------- |
| Repeat a build without changes             | 28.8 seconds           | 13.7 seconds    |
| Make a small source edit                   | 73.4 seconds           | 36.0 seconds    |
| Request a different revision               | 28.9 seconds           | 17.6 seconds    |
| Return to a previous revision              | 29.7 seconds           | 19.7 seconds    |
| Run two tasks at once: first task finishes | 36.5 seconds           | 20.8 seconds    |
| Run two tasks at once: both tasks finish   | 36.9 seconds           | 35.6 seconds    |

The gain was smaller when waiting for both concurrent tasks to finish.
Reuse allows reusing of existing build artifacts and existing build daemons,
speeding up builds. The four warmed worktrees and their build data used about 25 GB
of disk. Their idle build servers reported about 10.6 GB of memory
in total, including shared pages.

These results cover one build target with a local Git remote. Savings vary by
project and machine. See the [full benchmark evidence](CONTRIBUTING.md#accepted-benchmark-and-resource-costs)
for the measurements and test conditions.

## Installation

Supports Linux x86-64 with Git 2.36+. Install from an extracted source package
using Rust/Cargo 1.96+ and a native linker/C toolchain:

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
