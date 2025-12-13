# Mindreadr

Mindreadr is a cooperative word-convergence game for two players. Each round both players submit a word; then, seeing both words, they silently try to "meet in the middle" by each submitting a new bridging word. You win the moment both submit the exact same word in a round.

This project lives inside a Bazel monorepo and is built with Angular (zoneless), `rules_angular`, and `rules_js`.

## Project Structure

```text
angular/projects/mindreadr/
└── src/
    ├── app/                # Application source (components, signals, services)
    ├── index.html          # Root HTML
    ├── main.ts             # Bootstrap
    └── styles.css          # (Optional) Project styles
    ├── public/             # Static assets (served/copied)
    ├── BUILD.bazel         # Bazel build + test targets
    ├── tsconfig.app.json   # TypeScript config (app)
    └── tsconfig.spec.json  # TypeScript config (tests)
```

## Bazel Targets

| Target                                   | Kind             | Purpose                               |
| ---------------------------------------- | ---------------- | ------------------------------------- |
| `//angular/projects/mindreadr:mindreadr` | `ng_application` | Builds the Angular application bundle |
| `//angular/projects/mindreadr:test`      | `ng_test`        | Runs Angular unit tests               |

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

## Running Tests

Execute Angular unit tests under Bazel:

```bash
bazel test //angular/projects/mindreadr:test
```

Add a single spec file and re-run the target for fast feedback.

## Workflow Summary

1. Edit components / logic in `src/app/`.
2. Build: `bazel build //angular/projects/mindreadr:mindreadr`.
3. Serve locally: `bazel run //tools:ng -- serve mindreadr`.
4. Test: `bazel test //angular/projects/mindreadr:test`.
5. Commit source changes.

## CI Usage

CI can perform deterministic builds directly:

```bash
bazel build //angular/projects/mindreadr:mindreadr
bazel test //angular/projects/mindreadr:test
```

CSS generation is handled automatically by the build; no manual steps required.

## Troubleshooting

- Missing deps: Run `bazel run @pnpm -- --dir $PWD install`.
- Dev serve fails: Clean and retry `bazel clean` then build/serve again.

- Proxy issues: Verify path in `proxy.conf.json` and pass flag to serve command.

## License

See repository root `LICENSE`.
