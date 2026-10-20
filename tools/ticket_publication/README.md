# Prepare and create a ticket publication batch

This command checks and saves a publication plan. It can create the planned
issues and resume creation after an interruption. Tickets #2403 and #2404
implement these operations. Later tickets add explicit recovery commands,
native graph operations, and readiness labels.

```sh
bazel run //tools/ticket_publication -- prepare --plan /absolute/plan.json
bazel run //tools/ticket_publication -- status --batch <batch-id>
bazel run //tools/ticket_publication -- publish --batch <batch-id>
```

All commands return JSON. `prepare` returns `batch_id` and `plan_path`.
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
persistent `lock` file. A `.ticket-publication-directory` marker identifies each
directory that this tool creates. The snapshot contains the original checked plan,
fingerprint, batch identity, ticket identity markers, and creation progress.
Each marker has the form `<!-- ticket-publication:<batch-id>:<key> -->`.
The fingerprint identifies the serialized checked plan. It is not a signature.

The command synchronizes the snapshot before committing its name. It then
synchronizes the directory. Creation markers become durable before directory
publication. Preparation retries synchronization for marked directories and their
parent boundary. It does not require read access to earlier ancestors.
Retain these markers during state transfer. Preparation does not overwrite an
existing snapshot. Publication replaces progress through a synchronized temporary
file and an atomic rename. It then synchronizes the batch directory.
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

## Issue creation

`publish` reads the saved plan. It checks the parent and existing issue mappings
through fresh `gh api` requests. Creation follows dependency order. Each request
uses `--hostname github.com` and sends JSON through standard input.

The command saves each creation intent before the request. The request body
contains the original body and the exact hidden identity marker. A checked result
saves the issue number, numeric issue ID, and GraphQL node ID as separate values.
No dependent creation proceeds before this mapping becomes durable.

After uncertain creation, the command enumerates all repository issue pages
across open and closed states. It excludes pull requests. One exact marker match
must also pass repository, identity, and content checks before reuse. Multiple
matches stop publication. No match leaves creation uncertain. Failed reads and
incomplete responses cannot authorize another creation request.

A successful creation phase returns `status: partial` and `phase: issues_created`.
It does not establish publication completion. This phase applies no labels or
native relationships. A stopped run returns a nonzero exit code and reports
`partial`, `uncertain`, or `conflict`, with the available ticket progress.
Local `status` reports saved progress. It does not check remote state.

Preserve saved state after errors. A durable-write failure can leave an intent
without a request or a checked issue without a saved mapping. Restart checks the
saved state and remote evidence. It cannot infer safe retry from an absent match.
Explicit adoption, retry authorization, and reconstruction require later recovery
commands. Do not edit progress to bypass uncertainty.

## Checks

Command tests execute the Bazel-built program with private state and a fake `gh`
process. These tests check local behavior. They do not establish live GitHub
availability or isolation from external writers.
