# Publish a ticket publication batch

This command saves an approved publication plan. It creates the planned issues,
checks native relationships, and applies planned labels. Publication can resume
after an interruption with the original plan and identities.

```sh
bazel run //tools/ticket_publication -- prepare --plan /absolute/plan.json
bazel run //tools/ticket_publication -- status --batch <batch-id>
bazel run //tools/ticket_publication -- publish --batch <batch-id>
```

All commands return JSON. `prepare` returns `batch_id` and `plan_path`.
`status` returns the saved plan, its SHA-256 fingerprint, ticket progress, and
fresh remote mappings, graph evidence, and label evidence. It does not save
progress or write to GitHub.
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
A batch can contain at most 100 tickets, the GitHub limit for one parent.
Each ticket can require at most 50 distinct blockers. Each blocker can block at
most 50 planned tickets. Repeated references to one blocker use one slot.
GitHub documents this limit in its [dependency announcement](https://github.blog/changelog/2025-08-21-dependencies-on-issues/).
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
fingerprint, batch identity, ticket identity markers, creation progress, and
observed graph and label progress.
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
It applies planned labels only after a complete graph check. A successful
publication returns `status: complete`, `phase: complete`, and
`publication_complete: true`. Fresh reads must establish the planned issue
content, identities, required links, and labels before this result.

A stopped run returns a nonzero exit code. It reports `partial`, `uncertain`, or
`conflict`, with the available progress and `publication_complete: false`.
`status` checks saved mappings and unresolved identities. It also checks the
complete graph and labels when all saved issue mappings exist. It reports
`prepared`, `partial`, `uncertain`, `conflict`, `unverified`, or `complete`.
Failed remote reads return `unverified` and a nonzero exit code. Conflicts also
return a nonzero exit code. A saved completion receipt cannot establish current
completion. Repeated `prepare` and recovery results describe local progress only.
Status and reconstruction each enumerate the repository once when unresolved
identities require enumeration.

Preserve saved state after errors. A durable-write failure can leave an intent
without a request or a checked issue without a saved mapping. Restart checks the
saved state and remote evidence. It cannot infer safe retry from an absent match.
Use the recovery commands below. Do not edit progress to bypass uncertainty.

## Native graph operations

Before issue creation, publication checks the parent’s remaining native child
capacity. Existing planned children use no additional slots. Compatible children
from other repositories still count toward the parent limit. Insufficient capacity
stops publication before issue creation. The command also checks incoming and
outgoing dependency capacity, including compatible existing relationships.
Already present required links use no new slots.

Publication checks every ticket mapping, title, body, and identity marker before
creating relationships. It reads all pages of the parent sub-issues and each
ticket's blockers. A GraphQL query checks each ticket's nullable parent identity.
Errors and incomplete responses cannot establish an absent relationship.

The command checks the complete graph before relationship mutations. It then
checks affected issues and relationships before and after each missing link.
It checks the complete graph again after the last mutation. It does not read
every unaffected ticket after each link. A 100-ticket command test uses at most
2,500 fake API requests, including label publication and final checks. This test does not establish live rate-limit availability.

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
An early publication failure returns no graph counts from a previous run.

Native endpoint failures stop publication. The command does not substitute body
references. Preserve state and inspect the error before resuming. Graph checks
cannot prevent external changes between requests and do not establish isolation.

## Label publication and final checks

The command checks every planned issue and required native link before the first
label mutation. This rule includes `ready-for-agent`. Fully specified tickets
receive this label even when their blockers remain open. Agents must still select
only open tickets whose blockers have the `closed` state.

Label requests use GitHub's [add-labels endpoint](https://docs.github.com/en/rest/issues/labels#add-labels-to-an-issue).
They contain only missing planned labels. Compatible extra labels remain intact.
The five repository triage labels are mutually exclusive for publication:
`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, and `wontfix`.
A conflicting triage label stops publication. The command does not delete it.
Label comparisons ignore ASCII case. Plans reject duplicate labels and multiple
triage labels before preparation.

Before labels, the command checks all planned tickets for label conflicts and
incomplete label reads. It saves observed progress before each label request.
It checks the affected issue after each request and saves the checked result.
A required durable-write failure stops subsequent mutations. A lost response
stops the command even if GitHub accepted the labels.

On restart, publication checks the complete graph again before further label
requests. Fresh label reads skip completed mutations. Already applied labels
remain intact after failures. No automatic label rollback exists.

The `labels.tickets` entries contain `key`, `applied`, and `remaining` lists.
These lists describe the latest observed planned labels.
`labels.checked` states whether this invocation completed a label read for every
ticket. When false, the lists contain earlier observations only.
`labels.verified` becomes true only after complete final label reads.
A failed publication always returns false for this field.
`previous_labels` contains the last saved observations from an earlier run.
These observations can help identify retained work after an early failure.
They do not establish current GitHub state or authorize a skipped check.
Earlier observations remain durable when resumed graph or label checks fail.

After label requests, fresh reads check every planned issue, required relationship,
and planned label again. Missing effects, conflicts, incomplete reads, permission
failures, and unavailable endpoints prevent completion. Final progress must also
be durable before publication reports success. An external writer can change
GitHub state between requests. These checks do not provide isolation.

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
