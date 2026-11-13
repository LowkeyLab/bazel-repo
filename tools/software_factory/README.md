# Software factory intake

This Rust library implements issue #3152 under specification #3151.
It contains no service, scheduled entry, worker implementation, merge implementation,
or activation command. It does not start normal operation.

## Boundaries

- `Controller` serializes selection, history publication, action intents, read verification,
  and exact-target reconciliation. Its `Store` owns the exclusive process lock.
- `poll_with_backoff` returns the next polling delay to the host scheduler:
  60 seconds after a successful observation, configured exponential backoff after
  failures, and an error after failures exhaust the configured allowance.
  These delays do not authorize mutation retries.
- `GitHub` supplies complete Project, native relationship, issue, and history reads.
  `Adapter` uses GraphQL cursors, stable total counts, and unique identities for
  Project items, labels, assigned actors, children, and blockers. Native parent
  and child closure is complete before selection. Inaccessible or partial data
  stops the observation. History reads verify the stable comment count against
  the page budget before fetching comment bodies, then request one explicit HTTP
  page at a time. Each page must have its expected size, unique comment IDs, and
  the exact issue target. The final count must remain unchanged. Repository-qualified
  issue identities exclude foreign work. PR and draft Project items are not candidates.
- `GhTransport` calls an operator-selected absolute `gh` executable and an explicit
  credential environment variable. It does not fall back to the planning
  session's or worker's inherited GitHub token.
- `AppJwtTransport::new` requires an operator-selected absolute `curl` executable
  and the name of the dedicated App JSON web token environment variable. It sends an explicit
  `Authorization: Bearer` header through a standard-input-only curl configuration, with
  default curl configuration off and no redirect following. It never passes the token
  in arguments, temporary files, subprocess environment, or diagnostics.
  Do not use `GhTransport` for App JSON web tokens. It retains `gh` token authentication for
  read credentials and scoped installation tokens.
- `AppPublisher` authenticates the configured App with `AppJwtTransport`, verifies
  its approved installation, and obtains a token scoped
  to this repository with only issue-write and metadata-read permissions.
  Publication still needs exact App, Bot actor, issue, record, payload, digest,
  and continuity read verification before the private anchor advances. The read adapter
  requires its own provisioned read credential. Secrets are not put in receipts.
- `ExecutionInventory` must inspect trusted native execution state. Neither
  labels nor issue prose can establish absence of an executing worker.
- `NativeEffects` routes original actions to separate `Hermes`, `Pool`, and
  `Notifications` interfaces. Native tools remain authoritative. This ticket
  supplies intent/reconciliation seams, not replacements for native gateway
  admission, standalone worktree-pool, pool-receipt, hosted-review, or merge
  verification. Later integration must preserve their original receipts.
- `Unprovisioned` rejects execution, publication, effect reconciliation, and
  stopped-owner handoff. Missing prerequisites never select a weaker interface.

## Ownership and receipts

An operator must explicitly provision a new private receipt root with
`Store::initialize`. Normal starts use `Store::open`, which never creates missing
state. The root must be absolute, canonical, owned by the controller OS user,
mode `0700`, and outside Git checkouts. The controller OS user must own the
regular, singly linked, and private receipt and lock files (`0600` when created). State replacement
synchronizes the temporary file and directory before a dependent effect.

Reopening an existing store requires a fresh process identity and trusted
`StoppedOwner` evidence, even after a clean exit. Comments, an absent process ID,
quiet output, and a lock alone are not stopped-owner evidence. The controller publishes active-job owner
changes in authenticated history. A second process cannot proceed
while the first holds the lock. Mutable controller entry points serialize
transitions within the owner process.

Before history publication, the store retains the exact issue, record identity,
payload, attempted-send state, and returned comment identity when available.
An uncertain publication can only adopt the exact authenticated existing record.
Remote absence does not permit another publication. Private anchors retain
published identities and exact payload digests, not another repair ledger.

Before a non-history effect, the controller persists the original action, then
publishes and verifies its action intent. It rechecks authorization and ownership
before sending. The controller inspects an attempted action on its exact target. An absent,
unknown, or mismatched result stops dependent work without changing its identity
or repeating its mutation. A completed action identity cannot name a new payload.
One active job remains selected across these stops, waits, and restarts.
`prepare_admission` verifies eligibility. It does not itself authorize a native
mutation without that mutation's authenticated intent.

## Intake operations guide

Owner: the Software Factory operator, with a repository maintainer for authority
changes. This section is the intake portion of the specification's operational
guide. Subsequent tickets must extend it rather than add competing guides.

| Condition                                        | Required evidence and stop                                                                                                                                                                          | Resumption                                                                                                                                 |
| ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| New host provisioning                            | Operator records App/installation, OS isolation, receipt root, trusted execution inventory, approved read bounds, and retry bounds. Missing evidence blocks its operation.                          | Maintainer-approved provisioning does not grant activation approval.                                                                       |
| Restart or takeover                              | Operator proves the former controller stopped through trusted supervision. Retain exact private receipts and pending native targets. The exclusive lock must be free.                               | Fresh process identity, supervised handoff, full remote history validation, and exact pending-action reconciliation.                       |
| History damage or lost anchors                   | Operator preserves remote records and private files. Missing, changed, forged, duplicate, contradictory, or unanchored records stop advancement. Observed integrity damage persists a durable stop. | A maintainer must control reconciliation. This library has no automatic stop-clear, anchor reconstruction, fresh-budget, or reset command. |
| Lost effect acknowledgement                      | Operator inspects the original exact action target and native tool receipt. Absence alone does not authorize resending.                                                                             | Only an exact confirmed result advances automatically. Otherwise retain the slot for maintainer action.                                    |
| Blockers, waiting labels, or assignment conflict | Controller retains the active slot and does not send an admission. Blocked implementation retains readiness.                                                                                        | Fresh complete authorization, blocker, ownership, and execution reads. Do not take over a developer assignment.                            |
| External review or CI wait                       | Controller records the wait and retains the originating job. The controller selects no new issue.                                                                                                   | Resume the same job through the relevant later-ticket integration.                                                                         |
| Poll allowance exhausted                         | Host scheduler stops automatic polling and requests operator action. It infers no action-send allowance.                                                                                            | Operator supplies approved retry configuration and resolves the failed prerequisite.                                                       |

Receipt modes protect only against other OS users. Dedicated controller and
worker identities and deployed filesystem isolation remain activation
prerequisites. Fixture tests do not prove that provisioning. This ticket authorizes
and exercises no live Project mutation, issue comment, App token, worker, or
notification fixture. Final readiness retains the single activation gate.

## Validation

```sh
bazel run //:gazelle
aspect format --scope=all
aspect test //tools/software_factory:acceptance_test
aspect build //tools/software_factory/...
```

For local-only validation, append
`--remote_executor= --remote_cache= --bes_backend=` to Bazel test/build commands.
The acceptance target uses googletest and real private filesystem persistence.
It runs its harness serially because its independent-process lock probe can
inherit other harness threads' file descriptors between process creation and
exec. The lock probe itself exercises concurrent processes.

See [ACCEPTANCE.md](ACCEPTANCE.md) for expected results, actual results, fixture
configuration, evidence references, and the limits of those results.
The unavailable `asd-ste100` skill was not treated as strict-mode verification.

## Protocol references

The adapter follows the official GitHub GraphQL Issues reference (Issue,
`Assignee`, `assignedActors`, `blockedBy`, `subIssues`) and the GitHub Apps API reference
(Get the authenticated app, Get an installation, Create an installation access
token). Source inspection is not deployed protocol or permission evidence.
