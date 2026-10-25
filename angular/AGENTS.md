# AGENTS.md (Angular workspace)

This guide covers Angular development within this Bazel monorepo. For repo-wide prerequisites and common commands, see the root `AGENTS.md`.

## Workspace & commands

- Run Angular command-line tool via Bazel (for scaffolding only):

  ```bash
  bazel run //tools:ng -- <args>
  # examples
  bazel run //tools:ng -- generate component feature/example --project mindreadr
  ```

- Serve a project (dev server):

  ```bash
  ibazel run //angular/projects/mindreadr:mindreadr.serve
  ```

## Project guides

- Mindreadr: see `angular/projects/mindreadr/AGENTS.md`

Note: additional Angular projects live under `angular/projects/<name>/` and should include their own `AGENTS.md`.

## Code standards

Before TypeScript or Angular changes, read [the language standards](../docs/agents/typescript.md).
Use [the development workflow](../docs/agents/development.md) for dependencies and verification.
