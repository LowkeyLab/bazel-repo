---
name: agent-browser-frontend-testing
description: Use when frontend UI smoke testing Angular apps with agent-browser for browser verification after UI changes.
---

# Agent Browser Frontend Testing

## Overview

Use this skill to run repo specific Angular frontend smoke checks with `agent-browser`. It captures the minimum rules needed to verify visible UI behavior after a frontend change while keeping Bazel as the source of truth for serving and building apps.

## When to Use

Use this when an Angular frontend change needs browser verification, especially when the task affects routes, templates, styling, forms, auth state, or data displayed in the UI.

## Required Preconditions

- Work from the repository root unless a project guide says otherwise.
- Use Bazel targets to start Angular dev servers.
- Confirm any required backend, database, or local service is running before opening the UI.
- Use the `agent-browser` command already provided by the repo dev shell.
- Check `agent-browser` before starting any smoke workflow:

  ```bash
  command -v agent-browser
  ```

- If `agent-browser` is missing, stop and enter or use the repo Nix dev shell. Agents must not install tools. The repo shell provides `agent-browser` through `flake.nix`.
- Keep credentials, tokens, and screenshots out of committed files unless the task explicitly asks for sanitized evidence.

## Agent-Browser Workflow

1. Start the relevant app stack using the project guide.
2. Open the local frontend URL with `agent-browser`.
3. Navigate through the changed route or interaction path.
4. Wait for the page to settle before inspecting state.
5. Capture an interactive snapshot and verify visible text, controls, counts, and error states.
6. Close the browser session when the smoke check is complete.

Generic smoke command shape:

```bash
agent-browser open http://localhost:4200
agent-browser wait --load networkidle
agent-browser snapshot -i

# Optional, only when the project guide or task requires seeded browser state.
agent-browser eval "sessionStorage.setItem('example_key', 'example_value')"
agent-browser wait --load networkidle
agent-browser snapshot -i

agent-browser close
```

## Repo Angular Targets

- Angular apps live under `angular/projects/<name>/`.
- Serve targets follow the repo pattern:

  ```bash
  bazel run //angular/projects/<project>:<project>.serve
  ```

- Build targets follow the repo pattern:

  ```bash
  aspect build //angular/projects/<project>:<project>
  ```

- Test targets commonly use:

  ```bash
  aspect test //angular/projects/<project>:test
  ```

| App              | Serve                                                              | Build                                                           | Test                                                 | Smoke notes                                                                                                           |
| ---------------- | ------------------------------------------------------------------ | --------------------------------------------------------------- | ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `mindreadr`      | `bazel run //angular/projects/mindreadr:mindreadr.serve`           | `aspect build //angular/projects/mindreadr:mindreadr`           | `aspect test //angular/projects/mindreadr:test`      | Browser smoke can usually start from `http://localhost:4200/`; check the changed route or game UI state.              |
| `nicknamer`      | `bazel run //angular/projects/nicknamer:nicknamer.serve`           | `aspect build //angular/projects/nicknamer:nicknamer`           | `aspect test //angular/projects/nicknamer:test`      | Browser smoke can usually start from `http://localhost:4200/`; confirm visible route content for the changed feature. |
| `nicknamer2-web` | `bazel run //angular/projects/nicknamer2-web:nicknamer2-web.serve` | `aspect build //angular/projects/nicknamer2-web:nicknamer2-web` | `aspect test //angular/projects/nicknamer2-web:test` | Some flows need the full stack and auth token setup. See the subsection below.                                        |

## Nicknamer2-Web Authenticated Smoke Test

Use `angular/projects/nicknamer2-web/CLAUDE.md:38-61` as the source of truth for full-stack and auth startup details. Don't duplicate the long backend setup in this skill. Follow that guide to start infrastructure, run the backend, serve `nicknamer2-web`, obtain a local token, inject it with `agent-browser eval`, open the target route, wait for `networkidle`, and inspect the UI with `agent-browser snapshot -i`.

## Evidence and Cleanup

- Save command output or concise notes in the task evidence location requested by the caller.
- Record the URL, route, visible state checked, and any unexpected UI behavior.
- Close `agent-browser` after testing.
- Stop only the local services you started for the smoke check.

## Common Mistakes

- Starting an Angular app outside Bazel when a Bazel serve target exists.
- Running a smoke check before required backend services are ready.
- Treating a page load as enough without checking visible content.
- Leaving an `agent-browser` session open after evidence is captured.
- Adding install steps for tools already provided by the repo dev shell.
