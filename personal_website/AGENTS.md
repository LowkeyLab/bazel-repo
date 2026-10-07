# Agent instructions for personal website

This project is an Astro-based static site integrated into a Bazel monorepo. Agents should use Bazel commands rather than native `npm` or `pnpm` commands to ensure hermeticity and caching.

## Building

To build the static site:

```bash
aspect build //personal_website:build
```

The output is in `bazel-bin/personal_website/dist`.

## Development server

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
aspect lint //personal_website/...
```
