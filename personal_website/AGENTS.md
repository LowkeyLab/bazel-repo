# Agent Instructions for Personal Website

This project is an Astro-based static site integrated into a Bazel monorepo. Agents should use Bazel commands rather than native `npm` or `pnpm` commands to ensure hermeticity and caching.

## Building

To build the static site:

```bash
bazel build //personal_website:build
```

The output will be in `bazel-bin/personal_website/dist`.

## Development Server

To run the development server with live reloading (hot module replacement), use `ibazel`:

```bash
ibazel run //personal_website:dev
```

## Preview

To preview the production build locally:

```bash
bazel run //personal_website:preview
```

## Linting

To lint the project files:

```bash
bazel lint //personal_website:...
```
