# Pool command receipts

Use this wrapper for one explicitly requested `worktree-pool --json` command.
It preserves raw command evidence and returns at most 8,192 bytes of JSON, including the final newline.
It adds `--json` to the supplied pool arguments.
It does not choose a task assignment or take additional pool actions.
The standalone pool remains authoritative for catalog and ownership state.

## Commands

Inspect the selected catalog:

```sh
nix develop --command bazel run //tools/pool_receipt -- \
  --task issue-2778 -- catalog info
```

Select an exact retained assignment within that response:

```sh
nix develop --command bazel run //tools/pool_receipt -- \
  --task issue-2778 --assignment <exact-assignment-handle> -- catalog info
```

The selector reads that response only. It does not issue an assignment inspection or select another handle.
An assignment returned directly by an acquisition, release, or inspection remains that command's returned identity.
An inspection does not transfer ownership.
Follow the inherited ownership rules before editing.

Pass other explicitly authorized commands after `--`:

```sh
nix develop --command bazel run //tools/pool_receipt -- \
  --task issue-2778 -- assignment inspect <exact-assignment-handle>
```

For source or build work, stop active writers before release.
Use the already built wrapper executable after Bazel shutdown when you release its own checkout.
Keep its resolved executable path with the task receipts.

## Receipt location and capture

The default root is `$XDG_STATE_HOME/worktree-pool/agent-receipts`, or `$HOME/.local/state/worktree-pool/agent-receipts`.
Use `--receipt-dir <absolute-private-directory>` to select another root.
The root must be outside Git repositories and reusable checkouts.
The wrapper rejects discovered Git repositories and non-private existing roots.
Before using a custom root, confirm that it is outside pooled checkouts, including uncertain checkouts.
The root must be UTF-8, contain no control characters or parent traversal, and occupy at most 2,048 bytes.

The synchronous wrapper sets `umask` to `077` before it creates files or children.
It uses the repository's existing safe `rustix` binding.
Each invocation creates a unique directory with mode `0700`.
Files have mode `0600`.

The invocation contains:

- `invocation.json`: task identity, explicit assignment selector, program, and argument bytes.
- `stdout`: complete raw stdout bytes.
- `stderr`: complete raw stderr bytes.
- `exit-status.json`: original exit code, signal, process ID, and whether the process started.
- `selected-assignment.json`: the complete selected record when selection succeeds.
- `wrapper-error.json`: diagnostic metadata for an unknown wrapper result, when writing remains possible.

The wrapper connects the child's streams directly to private files.
It synchronizes invocation metadata before starting the command.
It waits for the process and synchronizes raw streams and exit status before parsing stdout.
Capture failures stop interpretation and return status `4`.
Invalid wrapper arguments can stop before a receipt directory exists.
They return bounded JSON with `pool_started: false` and status `4`.
A capture failure after process start can describe a completed or uncertain mutation.
Preserve that receipt. Inspect known identities before further effects.

## Summary and follow-up

The summary identifies the receipt directory, underlying exit status, wrapper exit status, capture status, and follow-up requirement.
It summarizes the returned catalog authority and pending operations.
It distinguishes a returned empty pending inventory from an inventory that the command did not return.
It returns at most eight pending records, their total count, and an explicit omission flag.
A known catalog blocker requires inspection even when catalog inspection itself completed.

An exact selected assignment includes its handle, acquisition operation, repository, worktree, state, and path reference.
`command_identity.operation_id` separately identifies the current command operation when returned.
The summary labels display paths as informational.
Read the complete path bytes from `selected-assignment.json` before using a checkout path.
Arguments retain their original Unix bytes in `invocation.json`.
The wrapper never converts lossless paths into display strings for filesystem actions.
Other command-specific evidence remains in raw stdout.

| Pool outcome | Pool exit | Wrapper behavior                                                                                 |
| ------------ | --------- | ------------------------------------------------------------------------------------------------ |
| `completed`  | `0`       | Preserve completion and exact returned identities. Apply ownership rules before further effects. |
| `rejected`   | `2`       | Preserve rejection. Inspect and resolve its cause before another attempt.                        |
| `pending`    | `3`       | Stop effects. Inspect the exact recorded operation and assignment.                               |
| `unknown`    | `4`       | Stop effects. Preserve evidence and resolve the uncertain outcome.                               |

The wrapper also returns `4` for unusable capture, malformed JSON, schema changes, outcome conflicts, or missing required identity.
`pool_exit` still reports the original exit when available.
`pool_outcome` retains a recognized reported outcome when JSON parsing succeeds.
Raw interpretation has a 16 MiB budget. Larger responses remain complete in the receipt and cause status `4`.
A summary that cannot fit its budget returns a short overflow result with status `4`.
Neither case permits an automatic retry.

The wrapper executes the supplied command once.
It never selects the newest assignment, infers a stopped caller, retries a mutation, expands capacity, or recovers automatically.
An explicitly supplied recovery command still requires human approval of its exact identity and action.
Successful recovery can leave ownership active or preparing, or leave a checkout withheld.
Inspect ownership and availability separately.

## Verification boundary

Command tests use a controlled pool executable and real private files.
They verify large responses, all outcome codes, exact selection, partial output, raw stderr, capture failures, and non-UTF-8 bytes.
They also verify the child process's inherited `umask` and single-command behavior.
These tests do not prove compatibility with the installed pool binary.
A live read-only catalog observation supplies separate compatibility evidence.
