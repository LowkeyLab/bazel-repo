---
status: accepted
---

# Extract worktree pool into a standalone repository

Move worktree pool into the public `LowkeyLab/worktree-pool` repository.
Preserve its directory history, the `worktree-pool` crate and executable names,
and the existing AGPL-3.0-only license. Use Cargo for development, testing,
formatting, linting, and packaging so this tool can evolve independently of the
Bazel monorepo.

The new repository becomes the authoritative source after it is validated and
known consumers and documentation are updated. Remove the monorepo
implementation only after these conditions hold. Existing catalog data and command-line contracts remain
compatible through the migration.

This decision replaces the previously approved Bazel-generated packaging
direction for this tool. Cancel the generic Bazel-to-Cargo publishing-tooling effort.
Close its open PR and unfinished tickets with links to this migration.

## CI and dependency maintenance

Run GitHub Actions on pull requests and pushes to `main`. Required checks
must preserve library tests, the complete command-line and crash-recovery suite, package
installation acceptance, formatting, Clippy, and release builds. Support remains
Linux x86-64. Validation covers Rust 1.96 and current stable, plus Git 2.36
compatibility. This is the agreed target coverage, not a claim about existing CI.

Use the existing self-hosted LowkeyLab Renovate service to maintain Cargo
dependencies and lockfiles, Rust toolchain pins, and pinned GitHub Actions.
Migration must verify GitHub App access and actual repository discovery.
Patch and minor updates may merge automatically only after required CI passes.
Major updates and changes to minimum supported versions require approval.

## Distribution and consumers

Package and installation verification are part of this migration. Automatic
crates.io publication and release-binary uploads are separate follow-up work.
The migration includes updating the `myNixOS` source pin and verifying its package
build before removing the monorepo implementation. The installed command and
agent guidance remain compatible.

## Benchmarks

Provide one manual paired Cargo benchmark in the standalone repository. Measure
worktree pool itself at a recorded commit. The benchmark compares retained worktrees and
build outputs with fresh worktrees for unchanged source and a small edit,
alternates execution order, and reports raw timings, median savings, failures,
and retained disk usage. CI checks runner correctness without timing thresholds.

Remove Bazel and Nicknamer dependencies, concurrency scenarios, and the GO/HOLD gate.
Keep the benchmark implementation outside the monorepo.
Previous published measurements remain clearly labelled historical evidence.
They do not establish the performance of the new Cargo benchmark.

## Cutover acceptance

The monorepo implementation and benchmarks may be removed only after the new
repository CI passes, required checks are configured, Renovate discovery is
verified, the Nix package builds from its new pin, and existing catalog and command-line
compatibility tests pass. The monorepo must still build, test, and lint after
removal. No task rewrites existing catalog data or reclaims retained assignments.

## Publishing-tooling cancellation

Close the unmerged [publishing PR #2162](https://github.com/LowkeyLab/bazel-repo/pull/2162)
and open issues #2141, #2152, and #2181 through #2184 with the cancellation reason.
Already closed historical tickets remain closed. Unrelated dependency updates
are outside this cancellation. The cancellation inventory was checked on
2026-10-12. Implementation must refresh remote state before changing it.

## Implementation status

The design decisions were accepted and implementation was authorized on
2026-10-12. The preceding cutover acceptance criteria determine completion.
