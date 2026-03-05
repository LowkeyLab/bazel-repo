# AGENTS.md (Mindreadr)

Project-specific commands and notes for the Mindreadr Angular application.

## Targets & Commands

- Build the app:

  ```bash
  aspect build //angular/projects/mindreadr:mindreadr
  ```

- Run unit tests:

  ```bash
  aspect test //angular/projects/mindreadr:test
  ```

- Serve locally (dev server with hot-reload):

  ```bash
  ibazel run //angular/projects/mindreadr:mindreadr.serve
  ```

## Notes

- Install dependencies from the repo root:

  ```bash
  pnpm install
  ```

- For repo-wide commands and Angular workspace guidance, see `angular/AGENTS.md`.
