# Pooled lint validation

Use the [standalone pool instructions](https://github.com/LowkeyLab/worktree-pool#readme) for acquisition, assignment inspection, release, and recovery. Use the [receipt wrapper](../../tools/pool_receipt/README.md) to retain private command receipts. The pool owns the checkout; the validation directory does not establish ownership.

1. Use the registered repository. Acquire an available assignment or use the exact active assignment from an authorized handoff. Retain the acquisition operation, assignment handle, repository and worktree IDs, and lossless path. Inspect that handle before use. Do not select a checkout by branch name or last use. If capacity is exhausted, wait. For a pending or unknown result, stop. Recovery requires approval for the exact operation and action.
2. Coordinate exclusive validation time with the owner. Do not run concurrent Bazel clients that can replace the same workspace symlinks. For mutation probes, use an owned checkout with no other writers and a clean sample file. Never release a shared implementation checkout to finish a probe.
3. From the assigned workspace root, enter the repository Nix development shell. Use the Aspect built from this checkout, including the integrity patch. Nix Git flakes omit untracked patches: confirm that the patch is in the flake source before validation. Set `PRIVATE_ROOT` to an absolute private directory outside all checkouts and `AUTH_RC` to the absolute path of the private BuildBuddy configuration file. Do not log credentials.

## Run normal lint

Use Bash. Give this validation session its own output base and evidence directory. Keep the normal `bazel-out` symlink **inside** the checkout. Do not use an external `--symlink_prefix` or suppress convenience symlinks. Without a usable `bb_clientd` mount, Aspect locates remote outputs from their workspace-relative build-event paths. An isolated output base does not change that lookup.

```bash
set -eu
umask 077
run=$(mktemp -d "${PRIVATE_ROOT:?}/lint.XXXXXXXX")
export BAZEL_REAL=bazelisk
git rev-parse HEAD > "$run/revision"
git status --porcelain=v1 > "$run/start.status"
lint_sample() {
  local dest="$run/$1" status=0
  mkdir "$dest" || return 1
  local cmd=(aspect "--bazelrc=${AUTH_RC:?}" "--output_base=$run/output" lint
    //python_sample:python_sample --config=remote-linux --spawn_strategy=remote
    '--strategy_regexp=.*=remote' --noremote_local_fallback
    --remote_local_fallback_strategy=remote --bazel-flag=--strategy=CoverageReport=remote
    --symlink_prefix=bazel- --remote_download_outputs=toplevel
    --remote_grpc_log= --execution_log_compact_file=
    "--execution_log_json_file=$dest/execution.json" "--build_event_json_file=$dest/bep.json")
  printf '%q ' "${cmd[@]}" > "$dest/command"
  "${cmd[@]}" > "$dest/lint.log" 2>&1 || status=$?
  printf '%s\n' "$status" > "$dest/exit"
  return "$status"
}
lint_sample clean
```

Redirect stdout as shown to request machine reports. Confirm that `bazel-out` resolves into this session's output base. In `clean/bep.json`, locate the `namedSetOfFiles` entries for `python_sample/python_sample.AspectRulesLint{KeepSorted,Ruff,Ty}.report` and each `.report.exit_code`. For **all six files**, join `pathPrefix` and `name`, read that workspace-relative file, and compare its SHA-256 with the event digest. Save the verified files privately **before** the next invocation. Require readable SARIF with the expected tool and no findings, three numeric zero exits, and Aspect exit 0. A tool name in the log is not proof of consumption. Reject any missing entry, read error, digest mismatch, parse warning, or skipped output.

## Deliberate failure

Use the same normal command and output base. In the exclusive probe checkout, save the original file outside the workspace. Restore only that file, including on error. Do not edit generated reports or the remote cache.

```bash
(
  set -eu
  file=python_sample/cli.py
  test -z "$(git status --porcelain=v1 -- "$file")"
  cp -p "$file" "$run/cli.py.original"
  trap 'cp -p "$run/cli.py.original" "$file"' EXIT
  printf '\npooled_lint_probe: int = "not an integer"\n' >> "$file"
  if lint_sample finding; then exit 1; fi
  test "$(<"$run/finding/exit")" = 1
)
cmp "$run/cli.py.original" python_sample/cli.py
```

Repeat the six-file digest and readability checks against `finding/bep.json`. Require a successful Bazel build, a nonzero Ty report exit, and the assignment diagnostic for the probe line in both Ty SARIF and Aspect output. An unrelated build failure or an unreadable report is not a successful negative test. Rerun `lint_sample restored` after restoration and repeat the clean checks.

For a separate unreadable-output control on a runner without `bb_clientd`, use a fresh exclusive checkout with **no** `bazel-out` link. Run the clean command with a fresh private output base and an external symlink prefix instead of `bazel-`. Require a successful build but nonzero Aspect exit with `Lint results are incomplete`. Never remove or redirect another session's workspace links to force this test.

Retain each command, process status, invocation ID, reports, and execution log privately. Inspect the execution records: reject local execution and distinguish remote-cache hits from fresh remote actions. These sample checks do not establish full lint or diagnostic-strategy acceptance.

Shut down only this server with `bazelisk --output_base="$run/output" shutdown`. Verify probe restoration, compare status with the starting record, and stop all writers. For a disposable clean assignment, release the exact acquired handle with the already-built receipt wrapper and inspect that handle again. If files or ownership are uncertain, retain the assignment and request a handoff; do not clean, force release, or recover it.
