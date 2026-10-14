# Issue tracker: GitHub

Issues and specs live in GitHub Issues for LowkeyLab/Bazel-repo.
Use `--repo LowkeyLab/bazel-repo` with `gh issue` commands, including outside
this checkout or from contributor forks. For `gh api`, specify the repository
in the endpoint (`repos/LowkeyLab/bazel-repo/...`); it has no `--repo` flag.

## Operations

- Create: `gh issue create --repo LowkeyLab/bazel-repo --title "..." --body-file <file>`
- Read: `gh issue view --repo LowkeyLab/bazel-repo <number> --json number,title,body,labels,comments,state,assignees,blockedBy,subIssues`
- List (first 30 results): `gh issue list --repo LowkeyLab/bazel-repo --state open --json number,title,body,labels,state,assignees,blockedBy`
- Comment: `gh issue comment --repo LowkeyLab/bazel-repo <number> --body-file <file>`
- Label: `gh issue edit --repo LowkeyLab/bazel-repo <number> --add-label "<label>"`
- Remove label: `gh issue edit --repo LowkeyLab/bazel-repo <number> --remove-label "<label>"`
- Close: `gh issue close --repo LowkeyLab/bazel-repo <number> --comment "..."`

For multiline bodies, write the exact text to a temporary file and use
`--body-file`. Apply state and label filters as needed.

For exhaustive discovery, fetch every page and exclude PRs returned by the
issues endpoint:

```bash
gh api --paginate 'repos/LowkeyLab/bazel-repo/issues?state=open&per_page=100' --jq '.[] | select(.pull_request == null)'
```

Apply label filters to the complete results or the API query. A limited list
is a preview, not evidence that an issue does not exist. Use the Read command
for each candidate when comments, dependencies, or child details are needed.

When a skill says "publish to the issue tracker," create a GitHub issue.
When it says "fetch the relevant ticket," read the issue and its comments.

## Pull requests as a triage surface

**PRs as a request surface: no.**

GitHub shares issue and PR numbers. When a reference is ambiguous, resolve
its type before operating on it.

## Wayfinding

A map is one issue labelled wayfinder:map, containing Notes,
Decisions-so-far, and Fog. Link child tickets as GitHub sub-issues;
if unavailable, use a task list in the map and `Part of #<map>` in each child.

Use wayfinder:research, wayfinder:prototype, wayfinder:grilling, or
wayfinder:task for child tickets.

Record blockers using GitHub native issue dependencies. If unavailable,
use `Blocked by: #<number>` in the child body. A ticket is unblocked when
all blockers are closed.

## Selecting and claiming work

GitHub issue assignment is ticket ownership for sequential work, which is the
default. Assign a newly selected ticket to the developer driving the map and
retain that assignment across sessions. Continuing an existing assignment needs
no session-specific claim, dispatch comment, or coordinator handoff.

Before selecting sequential work, reconcile closed child decisions with the map
index using the procedure in Recording results.

Read the map's ordered `subIssues` and each child's current state, assignees, and
`blockedBy` before choosing a ticket. Follow pagination when enumerating children
or blockers:
use `gh api --paginate repos/LowkeyLab/bazel-repo/issues/<map>/sub_issues`
and `gh api --paginate repos/LowkeyLab/bazel-repo/issues/<child>/dependencies/blocked_by`.
For fallback task lists and "Blocked by" lines, read every referenced issue's
current state. Missing or incomplete dependency data prevents starting work.

For sequential work, use a user-named ticket after checking its ownership,
state, and blockers. Without a named ticket, resume the first open, unblocked
child assigned to the developer driving the map, in map order. Select the first
open, unassigned, unblocked child only when no resumable assigned child exists.

Assign a newly selected ticket with
`gh issue edit --repo LowkeyLab/bazel-repo <number> --add-assignee <developer>`,
then confirm assignment and eligibility with a fresh read before starting.
When continuing a ticket already assigned to that developer, retain its
assignment and verify its current state and blockers. Resolve an assignment to
another developer with the maintainer before changing ownership.

A historical session or dispatch comment alone does not establish active
concurrent work. Ask about ownership only when current evidence identifies a
competing active session or conflicting claim. Resolve that conflict before
writing; use the protocol for parallel work when sessions continue concurrently.

### Parallel work

Use this protocol when the maintainer explicitly requests parallel sessions on
the same map. One coordinator session dispatches work; the maintainer designates
it before parallel work starts. Other sessions request tickets from it rather
than claiming independently. Pause parallel dispatch until a coordinator is
selected.

When entering parallel work, the coordinator first reconciles completed decisions
using Recording results, then confirms the worker status of retained open
assignments with the worker or maintainer. Preserve confirmed active workers and
existing dispatches. For eligible retained tickets without a dispatch, record one
for the confirmed worker. When replacing a worker, confirm the previous worker
has stopped first. Dispatch these tickets in map order before claiming unassigned
children, retaining their developer assignments. Keep blocked tickets assigned.
Pause dispatch for tickets whose worker status is uncertain.

The coordinator serializes selection, assignment, and dispatch. After accounting
for retained assignments, select the first open, unassigned, unblocked child in
map order for additional work, using the state and blocker checks. Record the
ticket-to-session mapping in a comment on each ticket before dispatch, with the
worker session identifier and coordinator identifier.
Assignment identifies the developer; dispatch identifies the worker when sessions
share a GitHub account. Confirm assignment and eligibility with a fresh read
before dispatching to exactly one worker session. Workers start after explicit
dispatch. Conflicting claims pause work until the coordinator selects one worker
and releases the others. A parallel coordinator handoff requires the previous
coordinator to stop and transfer its active dispatches before the replacement
starts.

### Parallel coordinator recovery

If the parallel coordinator is unavailable, pause new dispatches and map-body
updates. The maintainer may authorize a replacement after stopping or revoking
the old coordinator. The replacement reconstructs active dispatches from ticket
comments and confirms each worker's status with the worker or maintainer.
Retain confirmed active ownership; reassign only after the previous worker
is confirmed stopped. Uncertain tickets remain paused rather than dispatched
again. Record the takeover and reconciled ownership in tracker comments before
resuming; a returning old coordinator must obtain authorization before acting.

### Recording results

The working session posts the resolution comment, closes its ticket, and updates
the map's Decisions-so-far index. Reread the current map body, merge the new
context link while preserving existing entries, write the body, and verify the
result. If another writer is detected, pause map edits and reconcile before
continuing.

Reconciliation compares closed child issues with Decisions-so-far and Out of
scope. For each missing entry, read the recorded resolution or scope explanation.
Restore resolved decisions as title links with a short gist. Restore out-of-scope
closures in Out of scope. Preserve existing entries and verify the updated map
using the same map-update procedure. Report missing closure context rather than
inventing a decision. This recovers a session interrupted after closing its ticket
but before updating the map.

During parallel work, workers post resolution comments on their own tickets,
close them, and notify the coordinator. Only the coordinator edits the map body
and processes updates serially. The maintainer and other sessions route map edits
through it while parallel coordination is active. After coordinator recovery,
use the same reconciliation before resuming dispatches.
