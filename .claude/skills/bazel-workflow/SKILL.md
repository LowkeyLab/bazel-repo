---
name: bazel-workflow
description: Run the full Bazel regeneration pipeline (gazelle, format, build) after editing source files. Use when source files have been added, removed, or modified.
---

# Bazel Workflow

Run the following pipeline in order, stopping on first failure:

1. `bazel run gazelle` — regenerate BUILD files
2. `format` — format all code
3. `aspect build //...` — verify build

If `$ARGUMENTS` is provided, use it as the build target pattern instead of `//...`:
- Example: `/bazel-workflow //nicknamer/...` builds only the nicknamer service
- Gazelle and format always run against the full repo

Report the result of each step. If any step fails, show the error output and stop.
