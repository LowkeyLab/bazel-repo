# Check all formatting on the remote executor

Use `check.sh` instead of `aspect format --scope=all` for remote-only verification.
The client packages the working tree. A Bazel test runs `//tools/format:format`
with no file arguments in a writable copy. The formatter selects languages,
reads `.gitattributes`, and applies its usual configuration. It does not edit the checkout.

Use an assigned, isolated checkout and the normal workspace-local Bazel output
symlinks. Stop other edits during the check. Set `BAZEL_OUTPUT_BASE` to the
private output base for the assignment. Set `EVIDENCE` to an existing private
absolute directory outside the checkout. Configure BuildBuddy credentials in
private Bazel configuration. From the repository root, run:

```bash
nix develop --max-jobs 0 --no-write-lock-file --command env BAZEL_REAL=bazelisk \
  bash tools/format/remote/check.sh \
  --execution_log_json_file="$EVIDENCE/format.execution.json" \
  --build_event_json_file="$EVIDENCE/format.bep.json"
```

The archive uses working-tree bytes for tracked regular files, including staged
additions, dotfiles, nested packages, and files outside the Bazel target graph.
An unexpected non-ignored untracked file stops the check before Bazel starts.
Review and stage intended additions; move or ignore private files instead.
Skipped worktree entries, deletions, links, and paths beneath links are omitted;
link targets are never inputs. Other non-regular entries are rejected.
Use repository attribute files, not private Git attribute overrides.
The wrapper clears the workspace compact execution log and gRPC log settings so
the requested JSON execution log is the only execution log format.
The client reserves `tools/format/remote/input/` for the invocation and removes
it on exit. If it already exists, the client stops without changing it.

Retain the command exit status. Zero means that the tests passed and no file
needed formatting. A formatter error, a nonempty patch, or a build or test
failure makes the command fail. Before another invocation, copy the evidence:

```bash
logs=bazel-testlogs/tools/format/remote/format_test
cp "$logs/test.log" "$logs/test.outputs/formatter.exit" \
  "$logs/test.outputs/format.patch" "$EVIDENCE/"
```

`formatter.exit` is the multirun exit code. `format.patch` contains the changes
relative to the submitted bytes, including partial changes after a formatter
error. Review it with `git apply --stat`; use `git apply --check` before applying
a nonempty patch to the same tree. Missing outputs do not establish a pass.
Confirm the test action ran remotely in the execution log. Record cache hits
separately from fresh execution. Do not use `bazel run` as a fallback.
