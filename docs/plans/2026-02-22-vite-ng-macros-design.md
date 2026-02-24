# Vite Angular Symbolic Macro Design

## Goal

Create a Bazel symbolic macro (`vite_ng_application`) that encapsulates the boilerplate for building Angular apps with Vite + Analog.js. Reduces a ~67-line BUILD.bazel to ~10 lines.

## Scope

- New macro for the Vite+Analog build pattern only
- Existing `ng_application`/`ng_test` legacy macros remain unchanged
- Lives in `angular/bzl/ng.bzl` alongside existing macros

## Macro Interface

### `vite_ng_application`

| Attribute     | Type       | Default                                                         | Description                                       |
| ------------- | ---------- | --------------------------------------------------------------- | ------------------------------------------------- |
| `name`        | string     | (required)                                                      | Target name. Also creates `{name}_dev` devserver. |
| `srcs`        | label_list | glob of src/\*\*/\*, index.html, vite.config.ts, tsconfig files | Source files copied into sandbox.                 |
| `deps`        | label_list | `[]`                                                            | App-specific npm deps.                            |
| `vite_config` | string     | `"vite.config.ts"`                                              | Vite config path relative to package.             |
| `tailwindcss` | bool       | `False`                                                         | Adds Tailwind CSS v4 + DaisyUI deps.              |

### Core Dependencies (always included)

These are required by every Vite+Analog Angular app:

- `@analogjs/vite-plugin-angular`
- `@angular/common`, `@angular/compiler`, `@angular/compiler-cli`, `@angular/core`, `@angular/platform-browser`
- `rxjs`, `tslib`, `typescript`, `vite`

### Conditional Dependencies

When `tailwindcss = True`:

- `@tailwindcss/vite`, `tailwindcss`, `daisyui`

### Targets Created

- `{name}` — production build via `vite_bin.vite()` with `out_dirs = ["dist"]`
- `{name}_dev` — dev server via `js_run_devserver`

## Implementation

Uses Bazel 9 symbolic macro (`macro()` built-in) with explicitly declared attributes.

Internally:

1. `copy_to_bin` copies sources into execroot
2. Assembles full dependency list from core + conditional + caller deps
3. `vite_bin.vite()` for production build
4. `js_run_devserver` for dev server

## Example Usage

```starlark
load("//angular/bzl:ng.bzl", "vite_ng_application")

vite_ng_application(
    name = "nicknamer2-web",
    deps = [
        "//angular:node_modules/@apollo/client",
        "//angular:node_modules/apollo-angular",
        "//angular:node_modules/graphql",
    ],
    tailwindcss = True,
)
```
