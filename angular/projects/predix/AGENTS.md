# AGENTS.md (Predix)

Project-specific commands and notes for the Predix Angular application.

## Targets & Commands

- Build the app:

  ```bash
  bazel build //angular/projects/predix:predix
  ```

- Run unit tests:

  ```bash
  bazel test //angular/projects/predix:test
  ```

- Serve locally via Angular CLI:

  ```bash
  ng serve --project predix
  ```

## Notes

- Install dependencies from the repo root:

  ```bash
  pnpm install
  ```

- For repo-wide commands and Angular workspace guidance, see `angular/AGENTS.md`.
