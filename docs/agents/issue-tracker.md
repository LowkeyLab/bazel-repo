# Issue tracker: GitHub

Issues and specs live in GitHub Issues for LowkeyLab/bazel-repo.
Use the gh CLI with --repo LowkeyLab/bazel-repo for every issue operation,
including commands run outside this checkout or from contributor forks.

## Operations

- Create: gh issue create --repo LowkeyLab/bazel-repo --title "..." --body-file <file>
- Read: gh issue view --repo LowkeyLab/bazel-repo <number> --json number,title,body,labels,comments,state,assignees,blockedBy,subIssues
- List: gh issue list --repo LowkeyLab/bazel-repo --state open --json number,title,body,labels,state,assignees,blockedBy
- Comment: gh issue comment --repo LowkeyLab/bazel-repo <number> --body-file <file>
- Label: gh issue edit --repo LowkeyLab/bazel-repo <number> --add-label "<label>"
- Remove label: gh issue edit --repo LowkeyLab/bazel-repo <number> --remove-label "<label>"
- Close: gh issue close --repo LowkeyLab/bazel-repo <number> --comment "..."

For multiline bodies, write the exact text to a temporary file and use
--body-file. Apply state and label filters as needed.

When a skill says "publish to the issue tracker", create a GitHub issue.
When it says "fetch the relevant ticket", read the issue and its comments.

## Pull requests as a triage surface

**PRs as a request surface: no.**

GitHub shares issue and PR numbers. When a reference is ambiguous, resolve
its type before operating on it.

## Wayfinding

A map is one issue labelled wayfinder:map, containing Notes,
Decisions-so-far, and Fog. Link child tickets as GitHub sub-issues;
if unavailable, use a task list in the map and "Part of #<map>" in each child.

Use wayfinder:research, wayfinder:prototype, wayfinder:grilling, or
wayfinder:task for child tickets.

Record blockers using GitHub native issue dependencies. If unavailable,
use "Blocked by: #<number>" in the child body. A ticket is unblocked when
all blockers are closed.

## Selecting and claiming work

One coordinator session per map dispatches work. Before parallel work starts,
the maintainer designates that coordinator; other sessions request tickets
from it rather than claiming independently. If no coordinator is designated,
pause claiming until one is selected.

The coordinator reads the map's ordered subIssues and each child's current
state, assignees, and blockedBy before selecting the first open, unassigned,
unblocked child. Follow pagination when enumerating children or blockers:
use gh api --paginate repos/LowkeyLab/bazel-repo/issues/<map>/sub_issues
and gh api --paginate repos/LowkeyLab/bazel-repo/issues/<child>/dependencies/blocked_by.
For fallback task lists and "Blocked by" lines, read every referenced issue's
current state. Missing or incomplete dependency data prevents dispatch.

The coordinator serializes selection, assignment, and dispatch. Assign the
chosen ticket with gh issue edit --repo LowkeyLab/bazel-repo <number>
--add-assignee <developer>, then confirm assignment and eligibility with a
fresh read before dispatching it to exactly one worker session. Track the
ticket-to-session mapping even when workers share a GitHub account; assignment
alone is not a lock. Workers start only after explicit dispatch. Conflicting
claims pause work until the coordinator selects one worker and releases the
others. Coordinator handoff requires the previous coordinator to stop and
transfer its active dispatches before the replacement starts.

On resolution, comment with the answer,
close the ticket, and add a context link to the map's Decisions-so-far.
