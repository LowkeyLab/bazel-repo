# Prepare and link a ticket publication batch

This command checks and saves a publication plan. It can create the planned
issues and resume creation after an interruption. Tickets #2403 through #2406
implement preparation, creation, explicit recovery, and native relationships.
Ticket #2407 adds readiness labels.

```sh
bazel run //tools/ticket_publication -- prepare --plan /absolute/plan.json
bazel run //tools/ticket_publication -- status --batch <batch-id>
bazel run //tools/ticket_publication -- publish --batch <batch-id>
```

All commands return JSON. `prepare` returns `batch_id` and `plan_path`.
`status` returns the saved plan, its SHA-256 fingerprint, ticket progress, and
fresh remote mappings. It does not save progress or write to GitHub.
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

After creation, publication checks and creates the required native relationships.
A successful graph phase returns `status: partial`, `phase: graph_checked`, and
`publication_complete: false`. This phase applies no labels. A stopped run returns a nonzero exit code and reports
`partial`, `uncertain`, or `conflict`, with the available ticket progress.
`status` checks saved mappings and enumerates unresolved identities.
Status and reconstruction each enumerate the repository once for the batch. It reports
`prepared`, `partial`, `uncertain`, `conflict`, or `unverified`. Failed remote
reads return `unverified` and a nonzero exit code. Conflicts also return a nonzero
exit code. `publication_complete` remains false during this graph phase.
A checked issue does not establish graph or label completion.

Preserve saved state after errors. A durable-write failure can leave an intent
without a request or a checked issue without a saved mapping. Restart checks the
saved state and remote evidence. It cannot infer safe retry from an absent match.
Use the recovery commands below. Do not edit progress to bypass uncertainty.

## Native graph operations

Publication checks every ticket mapping, title, body, and identity marker before
creating relationships. It reads all pages of the parent sub-issues and each
ticket's blockers. A GraphQL query checks each ticket's nullable parent identity.
Errors and incomplete responses cannot establish an absent relationship.

The command adds only missing parent and blocker links. Requests use numeric
issue IDs. Parent requests specify `replace_parent: false`. A different parent
stops publication without replacement. Compatible extra links remain intact, including links to other repositories.
Issue numbers from other repositories cannot satisfy required local links.
Parent title, body, labels, and state remain intact.

Publication saves observed graph progress before each relationship mutation.
It checks an accepted link through fresh reads before another mutation. A lost
response stops the command. On restart, fresh reads identify accepted links and
prevent repeated mutations. Failed durable writes stop dependent mutations.

After the last mutation, publication checks all tickets and required links again.
The result includes `graph.parent_links`, `graph.dependency_links`, and
`graph.verified`. These fields describe checked required links. Compatible extra
links do not increase these counts. Saved observations cannot authorize skipped
checks. `status` checks the current graph without saving progress or GitHub writes.
A saved verified graph can become incomplete or conflicting after external edits.

Native endpoint failures stop publication. The command does not substitute body
references. Preserve state and inspect the error before resuming. Graph checks
cannot prevent external changes between requests and do not establish isolation.

## Explicit recovery

Recovery commands save local decisions. They perform no GitHub writes.
A later `publish` command checks fresh remote evidence before a mutation.

To adopt an issue, specify its issue number and the original ticket key:

```sh
bazel run //tools/ticket_publication -- recover adopt --batch <batch-id> --ticket <key> --issue <number>
```

Adoption checks the repository, exact body with the identity marker, and title.
It records the checked issue number, numeric ID, and node ID. It can resolve
multiple marker matches through an explicit issue choice. It cannot replace an
existing mapping with a different issue. Conflicts require inspection and explicit
reconciliation. The command does not overwrite GitHub content.

An absent match cannot prove that creation failed. To authorize one retry,
acknowledge the risk of creating a duplicate:

```sh
bazel run //tools/ticket_publication -- recover authorize-retry --batch <batch-id> --ticket <key> --acknowledge-duplicate-risk
bazel run //tools/ticket_publication -- publish --batch <batch-id>
```

Authorization requires uncertain creation. It does not issue a request.
Publication checks remote evidence again. One valid match becomes the saved
mapping. Multiple matches, content conflicts, and failed reads stop publication.
If no match exists, publication saves a new creation intent before the request.
This consumes the authorization. Another uncertain attempt requires another
explicit authorization.

## Export and reconstruction

Keep the original plan and batch identity outside the checkout. To export the
checked snapshot, use private file permissions:

```sh
umask 077
bazel run //tools/ticket_publication -- export --batch <batch-id> > /absolute/batch-export.json
```

The export contains the original plan, fingerprint, batch identity, markers,
and saved progress. It contains no credentials. An export does not transfer
publisher ownership or coordinate commands on another machine.

Before transfer, stop the previous publisher. Copy the complete batch directory
and original plan when available. Retain directory markers and the lock file.
Only one publisher may use the batch, including copies on different machines.
The tool cannot check that a publisher on another machine stopped.

If state is missing, reconstruct it in a private state directory with one of
these commands:

```sh
bazel run //tools/ticket_publication -- --state-dir /absolute/recovered recover reconstruct --batch <original-batch-id> --export /absolute/batch-export.json --publisher-stopped
bazel run //tools/ticket_publication -- --state-dir /absolute/recovered recover reconstruct --batch <original-batch-id> --plan /absolute/original-plan.json --publisher-stopped
```

`--publisher-stopped` records the operator's acknowledgement for this command.
It does not establish ownership. Reconstruction requires the original batch ID
and exactly one original input. An export must pass fingerprint and identity
checks. A plan input must be the original approved plan. The fingerprint is not
a signature and cannot establish who approved a plan.

Reconstruction checks all issue pages across open and closed states. It excludes
pull requests. Each unique match must pass content and repository checks.
Missing matches become uncertain. Reconstruction never restores retry
authorization from a copied export. Multiple matches without an exported mapping
save unresolved progress and return `conflict`. Use `recover adopt` to select
a checked issue. Publication rejects those unresolved matches until adoption.
An existing exported mapping retains its explicit issue choice after fresh checks.
An absent exported mapping or changed content stops reconstruction.
Existing state, including corrupt state, remains intact. Use a separate private directory to preserve that evidence.
Reconstruction neither creates issues nor applies labels or relationships.

Missing inputs stop the command. Do not use `prepare` to replace a lost batch.
Do not change the plan or identity to bypass recovery. A changed saved plan,
invalid fingerprint, or conflicting mapping stops resumed work. Retain the
original inputs and evidence for explicit reconciliation.

## Checks

Command tests execute the Bazel-built program with private state and a fake `gh`
process. These tests check local behavior. They do not establish live GitHub
availability or isolation from external writers.
