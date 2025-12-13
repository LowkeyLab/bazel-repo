# AGENTS.md (Mindreadr)

Project-specific commands and notes for the Mindreadr Angular application.

## Targets & Commands

- Build the app:

  ```bash
  bazel build //angular/projects/mindreadr:mindreadr
  ```

- Run unit tests:

  ```bash
  bazel test //angular/projects/mindreadr:test
  ```

- Serve locally via Angular CLI:

  ```bash
  ng serve --project mindreadr
  ```

## Notes

- Install dependencies from the repo root:

  ```bash
  pnpm install
  ```

- For repo-wide commands and Angular workspace guidance, see `angular/AGENTS.md`.
