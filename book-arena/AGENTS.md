# Book Arena Angular Guide

This project is an Angular SSR application managed inside the monorepo via Bazel. Use the commands below to stay aligned with repo tooling.

## Setup

- From `book-arena/`, install dependencies with `bazel run @pnpm -- --dir $PWD install`.
- Keep Bazel on the expected version by running `mise install` in the repo root when the toolchain changes.

## Angular CLI Essentials

- Start the development server with hot reload: `ng serve --host 0.0.0.0 --port 4200`.
- Produce a development build: `ng build book-arena --configuration development`.
- Run unit tests via Karma: `ng test book-arena`.
- Generate scaffolding, e.g. `ng generate component shelves/shelf-list`.

## Bazel Integration

- Verify the Angular workspace configuration and Node linking through Bazel with `bazel build //book-arena:ng-config`.
- Bazel will reuse the same lockfiles and toolchains; rerun `bazel build //book-arena:ng-config --sandbox_debug` when diagnosing environment issues.

## Helpful Tips

- Prefer running CLI commands from the project root so Angular picks up `angular.json` automatically.
- When switching branches, rerun the dependency install command to ensure generated Bazel symlinks stay current.
- Use `bazel clean --expunge` if you hit mismatched toolchain errors after version bumps.
