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

- Generate Tailwind CSS (writes `src/styles.css`):

  ```bash
  bazel run //angular/projects/mindreadr:write_styles_css
  ```

- Serve locally via Angular CLI:

  ```bash
  ng serve --project mindreadr
  ```

## Notes

- Tailwind sources are in `src/styles.source.css`; the build writes to `src/styles_generated.css` and copies to `src/styles.css`.
- Install dependencies from the repo root:

  ```bash
  pnpm install
  ```

- For repo-wide commands and Angular workspace guidance, see `angular/AGENTS.md`.
