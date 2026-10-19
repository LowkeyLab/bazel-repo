# Prepare a ticket publication batch

This command checks and saves a publication plan. It does not publish issues.
Ticket #2403 implements preparation and local inspection. Later tickets add
GitHub creation, recovery, graph checks, and readiness labels.

```sh
bazel run //tools/ticket_publication -- prepare --plan /absolute/plan.json
bazel run //tools/ticket_publication -- status --batch <batch-id>
```

Both commands return JSON. `prepare` returns `batch_id` and `plan_path`.
`status` returns the saved plan, its SHA-256 fingerprint, and ticket progress.
A prepared batch has `pending` tickets. Preparation does not establish GitHub
state or publication completion.

## Plan contract

```json
{
  "schema_version": 1,
  "repository": "LowkeyLab/bazel-repo",
  "parent": 2401,
  "tickets": [
    {
      "key": "prepare",
      "title": "Prepare a publication batch",
      "body": "Describe the approved behavior and acceptance criteria.",
      "labels": ["ready-for-agent"],
      "blockers": []
    },
    {
      "key": "publish",
      "title": "Publish the prepared batch",
      "body": "Describe the approved publication behavior.",
      "labels": ["ready-for-agent"],
      "blockers": [{ "ticket": "prepare" }, { "issue": 2392 }]
    }
  ]
}
```

Ticket order is part of the saved plan. Keys start with a lowercase ASCII letter.
Keys contain lowercase ASCII letters, digits, or hyphens, with at most 64 bytes.
Issue numbers must be positive. Plans must use the supported repository.
Unknown fields, duplicate keys, missing blocker keys, and dependency cycles fail.
Bodies must not contain the reserved `<!-- ticket-publication:` marker prefix.
Planned labels remain data during preparation. This includes `ready-for-agent`.

## Saved state

The default directory is `$XDG_STATE_HOME/ticket-publication`. If
`XDG_STATE_HOME` is unset, the command uses
`$HOME/.local/state/ticket-publication`. `--state-dir` selects another directory.
Keep this directory outside Git checkouts and reusable worktrees.
State paths must use UTF-8 because the command returns paths in JSON.
The command rejects other path encodings before creating state.

Each batch has a private directory. The directory contains `batch.json` and a
persistent `lock` file. The snapshot contains the original checked plan,
fingerprint, batch identity, ticket identity markers, and pending progress.
Each marker has the form `<!-- ticket-publication:<batch-id>:<key> -->`.
The fingerprint identifies the serialized checked plan. It is not a signature.

The command synchronizes the snapshot before committing its name. It then
synchronizes the directory. It does not overwrite an existing snapshot.
A failed write prevents a successful result. An interruption can leave temporary
files or a directory without a committed snapshot. `status` rejects incomplete,
corrupt, or unsupported state without replacing the snapshot.

Use `prepare --plan <path> --batch <batch-id>` to repeat preparation with the same
identity. An existing valid snapshot must match the supplied plan. This command
cannot replace a saved plan or repair corrupt state. Retain incomplete state for
inspection. Do not infer that an interrupted command completed.

The batch lock excludes another command that uses the same state directory.
A process exit releases the lock. Do not delete the persistent lock file.
A local lock cannot coordinate copied state across machines. Stop the previous
publisher before transferring the complete batch directory and original plan.
Copying state does not transfer ownership. No automatic takeover exists.

## Checks

Command tests execute the Bazel-built program with private state and a fake `gh`
process. These tests check local behavior. They do not establish live GitHub
availability or isolation from external writers.
