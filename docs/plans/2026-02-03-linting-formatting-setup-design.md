# Linting and Formatting Setup Update

**Date:** 2026-02-03  
**Status:** Approved  
**Reference:** Based on https://github.com/bazel-starters/kitchen-sink

## Overview

Update the linting and formatting setup to align with the kitchen-sink reference repository structure while preserving existing specialized tooling. This will provide a cleaner, more maintainable configuration following proven patterns.

## Goals

1. Adopt kitchen-sink's cleaner linter/formatter organization pattern
2. Add Rust clippy linting for comprehensive Rust code quality
3. Update Rust formatter to use upstream_rustfmt
4. Preserve all existing specialized linters (buf, checkstyle, keep-sorted, stylelint)
5. Add lint_test helpers for better CI integration

## Non-Goals

- Adding C++ tooling (clang-tidy, clang-format) - not actively used
- Adding Ruby tooling (rubocop) - not in codebase
- Adding Python type checking (ty) - not currently needed
- Modifying MODULE.bazel - aspect_rules_lint v2.0.0 already merged from main

## Architecture

### Directory Structure

```
tools/
├── lint/
│   ├── BUILD.bazel      # Linter binary definitions
│   └── linters.bzl      # Aspect configurations
└── format/
    └── BUILD.bazel      # Formatter binaries & multirun
```

### Linter Organization

Linters will be organized by language/domain in `tools/lint/linters.bzl`:

**Protocol Buffers:**

- `buf` - Protobuf linting (existing)

**JavaScript/TypeScript/CSS:**

- `eslint` - JavaScript/TypeScript linting (existing, reorganize)
- `stylelint` - CSS linting (existing)

**Java/Kotlin:**

- `checkstyle` - Java linting (existing)
- `pmd` - Java static analysis (existing)
- `ktlint` - Kotlin linting (existing)

**Rust:**

- `clippy` - Rust linting (NEW - uses existing `.clippy.toml`)

**Python:**

- `ruff` - Python linting and formatting (existing)

**Shell:**

- `shellcheck` - Shell script linting (existing)

**General:**

- `keep_sorted` - Keep lists sorted (existing)

### Formatter Configuration

Format multirun in `tools/format/BUILD.bazel` will support:

- CSS, HTML, JavaScript, Markdown → prettier
- Go → gofumpt
- Java → google-java-format
- Kotlin → ktfmt
- Python → ruff
- Rust → upstream_rustfmt (CHANGED from upstream_wrapper)
- Shell → shfmt
- Starlark → buildifier
- YAML → yamlfmt

## Detailed Design

### 1. Update tools/lint/linters.bzl

Reorganize to match kitchen-sink pattern:

- Group related linters together
- Use consistent naming conventions
- Add lint_test helpers for eslint, ruff, shellcheck
- Add new clippy aspect

```starlark
load("@aspect_rules_lint//lint:clippy.bzl", "lint_clippy_aspect")
# ... other imports

clippy = lint_clippy_aspect(
    config = Label("//:.clippy.toml"),
)

# Add lint_test helpers
eslint_test = lint_test(aspect = eslint)
ruff_test = lint_test(aspect = ruff)
shellcheck_test = lint_test(aspect = shellcheck)
```

### 2. Update tools/format/BUILD.bazel

Change Rust formatter path:

```starlark
format_multirun(
    name = "format",
    # ... other formatters
    rust = "@rules_rust//tools/rustfmt:upstream_rustfmt",  # Changed
    # ... other formatters
)
```

### 3. Configuration Files

All existing configuration files remain unchanged:

- `.clippy.toml` - Already exists for Rust
- `buf.yaml` - Protocol buffer config
- `checkstyle.xml` - Java checkstyle config
- `eslint.config.mjs` - ESLint config
- `pmd.xml` - PMD config
- `.editorconfig` - Editor config for ktlint
- `.ktlint-baseline.xml` - Ktlint baseline
- `pyproject.toml` - Python/ruff config
- `.shellcheckrc` - Shellcheck config
- `stylelint.config.mjs` - Stylelint config

## Implementation Plan

### Phase 1: Update Formatter

1. Modify `tools/format/BUILD.bazel`
2. Change rust formatter from `upstream_wrapper:rustfmt` to `rustfmt:upstream_rustfmt`
3. Test: `bazel format`

### Phase 2: Update Linters

1. Reorganize `tools/lint/linters.bzl` following kitchen-sink pattern
2. Add clippy aspect configuration
3. Add lint_test helpers for eslint, ruff, shellcheck
4. Test: `bazel lint`

### Phase 3: Verification

1. Run `bazel format` on entire codebase
2. Run `bazel lint` on entire codebase
3. Verify all linters/formatters work correctly
4. Check that lint_test targets pass

## Testing Strategy

**Manual Testing:**

- `bazel format` - Verify all formatters work
- `bazel lint` - Verify all linters work
- Test on sample files from each language

**Automated Testing:**

- lint_test targets will run in CI
- Catch linting issues before merge

## Risks and Mitigations

**Risk:** Rust formatter path change breaks existing workflows  
**Mitigation:** The new path is the recommended upstream approach; verify with test run

**Risk:** Clippy may find new issues in existing Rust code  
**Mitigation:** Review clippy findings and update `.clippy.toml` if needed to suppress false positives

**Risk:** aspect_rules_lint v2.0.0 API changes  
**Mitigation:** Already merged from main, patterns used are compatible with v2.0.0

## Success Criteria

1. All formatters work via `bazel format`
2. All linters work via `bazel lint`
3. Clippy successfully lints Rust code
4. lint_test targets pass
5. Code structure matches kitchen-sink organization
6. All existing linters continue to function

## References

- Kitchen-sink repository: https://github.com/bazel-starters/kitchen-sink
- aspect_rules_lint documentation: https://github.com/aspect-build/rules_lint
- Clippy documentation: https://doc.rust-lang.org/clippy/
