# PNPM Workspace Setup for Bazel Repo

## Overview

This repository has been configured as a pnpm workspace with centralized dependency management, following the patterns from the [Aspect Build bazel-examples/angular-ngc](https://github.com/aspect-build/bazel-examples/tree/main/angular-ngc) project.

## Structure

```
bazel-repo/
├── pnpm-workspace.yaml          # Defines workspace packages
├── package.json                  # Root package.json with all dependencies
├── pnpm-lock.yaml               # Centralized lockfile
├── .npmrc                        # pnpm configuration
├── MODULE.bazel                  # References root pnpm-lock.yaml
├── BUILD.bazel                   # Root BUILD with npm_link_all_packages
└── angular-test/
    ├── package.json              # Workspace package (no dependencies)
    ├── BUILD.bazel               # Uses npm_link_all_packages
    └── src/                      # Angular application source
```

## Key Files

### `pnpm-workspace.yaml`
```yaml
packages:
  - 'angular-test'
```

Defines `angular-test` as a workspace member.

### Root `package.json`
- Contains all shared dependencies in one place
- Includes `pnpm.onlyBuiltDependencies` configuration for native modules
- Uses `workspaces` field to reference workspace packages

### `angular-test/package.json`
- Simplified to only include package metadata and scripts
- No dependencies defined (inherited from root)
- Named with scoped package name: `@bazel-repo/angular-test`

### `MODULE.bazel`
Updated to:
```starlark
pnpm = use_extension("@aspect_rules_js//npm:extensions.bzl", "pnpm", dev_dependency = True)
use_repo(pnpm, "pnpm")

npm = use_extension("@aspect_rules_js//npm:extensions.bzl", "npm", dev_dependency = True)
npm.npm_translate_lock(
    name = "npm",
    pnpm_lock = "//:pnpm-lock.yaml",
    npmrc = "//:.npmrc",
    verify_node_modules_ignored = "//:.bazelignore",
)
use_repo(npm, "npm")
```

### `.npmrc`
```
public-hoist-pattern[]=
node-linker=hoisted
ignore-scripts=false
```

## Benefits

1. **Centralized Dependency Management**: All npm dependencies are declared once in the root `package.json`
2. **Single Lockfile**: One `pnpm-lock.yaml` at the root for the entire monorepo
3. **Workspace Support**: Multiple packages can share dependencies efficiently
4. **Bazel Integration**: Works seamlessly with `@aspect_rules_js` and `npm_link_all_packages`
5. **Consistent Versions**: All workspace packages use the same versions of dependencies

## Usage

### Installing Dependencies

```bash
pnpm install
```

This will install all dependencies for all workspace packages.

### Adding a New Dependency

Add to the root `package.json`:
```bash
pnpm add <package-name> -w
```

The `-w` flag indicates installing at workspace root.

### Running Bazel Commands

```bash
# Build Angular application
bazel build //angular-test:app

# Run development server
bazel run //angular-test:dev_server

# Query available packages
bazel query //angular-test:node_modules/@angular/core
```

### Adding a New Workspace Package

1. Create a new directory (e.g., `new-package/`)
2. Add it to `pnpm-workspace.yaml`:
   ```yaml
   packages:
     - 'angular-test'
     - 'new-package'
   ```
3. Create `new-package/package.json` with minimal metadata
4. Add any specific dependencies to root `package.json`
5. Create `new-package/BUILD.bazel` with `npm_link_all_packages()`

## References

- [Aspect Build bazel-examples/angular-ngc](https://github.com/aspect-build/bazel-examples/tree/main/angular-ngc)
- [pnpm workspaces documentation](https://pnpm.io/workspaces)
- [aspect_rules_js documentation](https://docs.aspect.build/rules/aspect_rules_js)
