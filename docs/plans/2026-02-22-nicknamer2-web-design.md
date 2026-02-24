# Nicknamer2 Web Design Proposal

**Date:** 2026-02-22
**Topic:** Create a new Angular application using Vite for Nicknamer2

## Context

Nicknamer2 is a Rust-based backend exposing a GraphQL API using Juniper. We are creating a new Angular web frontend to interface with this API.

The main requirements are:

1. Fast local development and builds via Vite.
2. DaisyUI + Tailwind CSS for styling.
3. GraphQL communication with `apollo-angular` handling Relay-style pagination.
4. Seamless integration with the existing Bazel and `pnpm-workspace.yaml` monorepo structure.

## Architecture

**Workspace Location**
The new project will be created inside the existing Angular workspace at `angular/projects/nicknamer2-web`. This allows us to share `node_modules`, standard configurations, and Bazel rules with the other projects (`predix`, `mindreadr`).

**Vite + Angular Setup**
We will implement the `@analogjs/vite-plugin-angular` and `@analogjs/platform:vite` plugins to replace `@angular/build:application`.

- A `vite.config.ts` will define the build pipeline, utilizing Analog's standard implementation for Angular.
- `angular.json` will be updated to point the `build` architect to the Vite platform builder.
- This creates an SPA (Single Page Application) built with Vite rather than relying on Webpack/Esbuild.

## Data Fetching (GraphQL)

The API exposes Relay `Node` specifications for objects like `Server` and `Name`.

- **Apollo Angular** will be used to initialize the connection to `http://localhost:<PORT>/graphql`.
- **GraphQL Codegen** (`@graphql-codegen/cli`) will be configured to automatically pull the schema from the Rust backend and generate TypeScript types, Queries, and Apollo Services.

## UI & Styling

The UI will be styled using Tailwind CSS coupled with DaisyUI components.

- The `tailwind.config.js` will scan `angular/projects/nicknamer2-web/src/**/*.{html,ts}`.
- `daisyui` will be registered as a Tailwind plugin.

## Bazel Build Integration

A new `BUILD.bazel` file will be created in `angular/projects/nicknamer2-web/`. It will utilize standard rules (likely `js_run_devserver` or a custom Vite invocation rule) allowing `aspect build //angular/projects/nicknamer2-web` to compile the app and `aspect run //angular/projects/nicknamer2-web:dev` to serve it.

## Trade-offs

- **Vite vs Esbuild:** Angular's default builder already uses Esbuild, but moving to full Vite allows using the wider Vite plugin ecosystem and Analog's optimized pipeline.
- **SPA vs SSR:** By choosing the Vite plugin approach over a full Analog SSR meta-framework setup, we simplify the Bazel integration while still getting the developer experience benefits of Vite. SEO is not a stated primary concern for this application.
