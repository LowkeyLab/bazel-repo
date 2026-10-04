# Issue tracker: GitHub

Issues and specs live in GitHub Issues for LowkeyLab/bazel-repo.
Use the gh CLI from this checkout; it infers the repository from origin.

## Operations

- Create: gh issue create --title "..." --body-file <file>
- Read: gh issue view <number> --json number,title,body,labels,comments
- List: gh issue list --state open --json number,title,body,labels
- Comment: gh issue comment <number> --body-file <file>
- Label: gh issue edit <number> --add-label "<label>"
- Remove label: gh issue edit <number> --remove-label "<label>"
- Close: gh issue close <number> --comment "..."

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

Choose the first open, unassigned, unblocked child in map order.
Claim it by assigning yourself. On resolution, comment with the answer,
close the ticket, and add a context link to the map's Decisions-so-far.
