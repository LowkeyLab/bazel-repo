# Mindreadr

Mindreadr is a cooperative word-convergence game for two players. Each round both players submit a word; then, seeing both words, they silently try to "meet in the middle" by each submitting a new bridging word. You win the moment both submit the exact same word in a round.

This project lives inside a Bazel monorepo and is built with Angular (zoneless), `rules_angular`, and `rules_js`. TailwindCSS utilities can be generated using the shared pattern (same as `tailwind-sample`) if you introduce Tailwind classes.

## Project Structure

```text
angular/projects/mindreadr/
├── src/
│   ├── app/                # Application source (components, signals, services)
│   ├── index.html          # Root HTML
│   ├── main.ts             # Bootstrap
│   ├── styles.source.css   # (Optional) Tailwind v4 source with directives
│   └── styles.css          # (Optional) Generated CSS if Tailwind used
├── public/                 # Static assets (served/copied)
├── BUILD.bazel             # Bazel build + test targets
├── tsconfig.app.json       # TypeScript config (app)
└── tsconfig.spec.json      # TypeScript config (tests)
```

## Bazel Targets

| Target                                          | Kind                 | Purpose                                   |
| ----------------------------------------------- | -------------------- | ----------------------------------------- |
| `//angular/projects/mindreadr:mindreadr`        | `ng_application`     | Builds the Angular application bundle     |
| `//angular/projects/mindreadr:test`             | `ng_test`            | Runs Angular unit tests                   |
| `//angular/projects/mindreadr:write_styles_css` | `write_source_files` | (Optional) Regenerates TailwindCSS output |

## Prerequisites

All toolchains (Node.js, pnpm) are Bazel-managed. No global installations required.

Install workspace Node dependencies once (from repo root):

```bash
bazel run @pnpm -- --dir $PWD install
```

## Building (Production Bundle)

Build the application via Bazel:

```bash
bazel build //angular/projects/mindreadr:mindreadr
```

Outputs will appear under `bazel-bin/angular/projects/mindreadr/` (fingerprinted build artifacts). Use these for packaging or deployment.

## Serving (Development)

Use the Bazel-wrapped Angular CLI tool target to serve with live reload:

```bash
bazel run //tools:ng -- serve mindreadr
```

Notes:

- Runs the Angular dev server (typically on <http://localhost:4200>).
- If you encounter a failure, ensure dependencies are installed (`bazel run @pnpm -- --dir $PWD install`) and retry.
- For proxying backend API requests, adjust `proxy.conf.json` if needed and pass `--proxy-config proxy.conf.json` after `serve`.

Example with proxy config:

```bash
bazel run //tools:ng -- serve mindreadr --proxy-config angular/projects/mindreadr/proxy.conf.json
```

## Regenerating TailwindCSS (If Using Tailwind)

When you add or change Tailwind utility classes (in templates or TS):

```bash
bazel run //angular/projects/mindreadr:tailwindcss_runner
```

This target:

1. Uses the Tailwind CLI binary provided by `@tailwindcss/cli` (via `tailwindcss_binary`).
2. Scans `src/**/*.html` and `src/**/*.ts` for referenced utility classes.
3. Produces a tree‑shaken `src/styles_generated.css` then writes `src/styles.css` via a `write_source_files` action.

Commit the updated `src/styles.css` after regeneration (it is treated as generated but checked in for deterministic builds).

## Running Tests

Execute Angular unit tests under Bazel:

```bash
bazel test //angular/projects/mindreadr:test
```

Add a single spec file and re-run the target for fast feedback.

## Workflow Summary

1. Edit components / logic in `src/app/`.
2. (Optional) Regenerate CSS if Tailwind utility usage changes.
3. Build: `bazel build //angular/projects/mindreadr:mindreadr`.
4. Serve locally: `bazel run //tools:ng -- serve mindreadr`.
5. Test: `bazel test //angular/projects/mindreadr:test`.
6. Commit source + regenerated CSS (if changed).

## CI Usage

CI can perform deterministic builds directly:

```bash
bazel build //angular/projects/mindreadr:mindreadr
bazel test //angular/projects/mindreadr:test
```

Regenerating CSS in CI is unnecessary if `styles.css` is committed.

## Troubleshooting

- Missing deps: Run `bazel run @pnpm -- --dir $PWD install`.
- Dev serve fails: Clean and retry `bazel clean` then build/serve again.
- CSS not updating: Ensure you ran `generate-css` after adding new classes.
- Proxy issues: Verify path in `proxy.conf.json` and pass flag to serve command.

## License

See repository root `LICENSE`.
