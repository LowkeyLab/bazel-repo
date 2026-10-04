# Issue tracker: GitHub

Issues and specs live in GitHub Issues for LowkeyLab/bazel-repo.
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

When a skill says "publish to the issue tracker", create a GitHub issue.
When it says "fetch the relevant ticket", read the issue and its comments.

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

One coordinator session per map dispatches work. Before parallel work starts,
the maintainer designates that coordinator; other sessions request tickets
from it rather than claiming independently. If no coordinator is designated,
pause claiming until one is selected.

The coordinator reads the map's ordered subIssues and each child's current
state, assignees, and blockedBy before selecting the first open, unassigned,
unblocked child. Follow pagination when enumerating children or blockers:
use `gh api --paginate repos/LowkeyLab/bazel-repo/issues/<map>/sub_issues`
and `gh api --paginate repos/LowkeyLab/bazel-repo/issues/<child>/dependencies/blocked_by`.
For fallback task lists and "Blocked by" lines, read every referenced issue's
current state. Missing or incomplete dependency data prevents dispatch.

The coordinator serializes selection, assignment, and dispatch. Assign the
chosen ticket with `gh issue edit --repo LowkeyLab/bazel-repo <number> --add-assignee <developer>`,
then confirm assignment and eligibility with a
fresh read before dispatching it to exactly one worker session. Track the
ticket-to-session mapping in a comment on each ticket before dispatch, with
the worker session identifier and coordinator identifier, even when workers
share a GitHub account; assignment alone is not a lock. Workers start only after explicit dispatch. Conflicting
claims pause work until the coordinator selects one worker and releases the
others. Coordinator handoff requires the previous coordinator to stop and
transfer its active dispatches before the replacement starts.

### Coordinator recovery

If the coordinator is unavailable, pause new dispatches and map-body updates.
The maintainer may authorize a replacement after stopping or revoking the old
coordinator. The replacement reconstructs active dispatches from ticket
comments and confirms each worker's status with the worker or maintainer.
Retain confirmed active ownership; reassign only after the previous worker
is confirmed stopped. Uncertain tickets remain paused rather than dispatched
again. Record the takeover and reconciled ownership in tracker comments before
resuming; a returning old coordinator must obtain authorization before acting.

### Recording results

Workers post resolution comments on their own tickets, close them, and notify
the coordinator. Only the coordinator edits the map body, including its
Decisions-so-far index. It processes updates serially: reread the current body,
merge the new context link while preserving existing entries, write the body,
and verify the result. The maintainer and other sessions route map edits
through the coordinator while it is active. If another writer is detected,
pause map edits and reconcile before continuing. After recovery, reconcile
closed tickets against the index so completed results are not lost.
