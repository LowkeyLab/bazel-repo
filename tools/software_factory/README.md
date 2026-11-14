# Software factory controller

This Rust library implements issue #3152 for intake, issue #3153 for one native
implementation worker, and issue #3154 for AFK research under specification #3151. It contains no service, scheduled entry, merge
implementation, or activation command. It does not start normal operation.

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
- `NativeEffects` retains the generic `Hermes`, `Pool`, and `Notifications` routes.
  `NativeWorker` implements the concrete worker protocol mapping and the
  `WorkerRuntime` controller boundary. Native gateway admissions, standalone
  worktree-pool, pool-receipt, hosted-review, and merge verification remain
  authoritative. The library does not replace these interfaces with a second execution queue.
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

## Native worker workflow

### Native worker workflow and prerequisite stops

`Controller::execute_worker` operates only on its selected implementation or research issue.
It stores run, create, submit, cancel, and publication identities before a native
send. Every transition uses the existing protected `Store` and App-authenticated
issue history. `WorkerCheckpoint` binds the complete private receipt by digest.
It does not publish original pool captures. Restart checks that binding before
using the original identities. A missing worker field in an older private state
is a prerequisite stop, not permission to reinitialize or reconstruct history.
The operator must retain the old files and seek a maintainer-approved migration.

The worker states are `Prepared`, `Created`, `Running`, `Implemented`,
`RepairBlocked`, `Cancelling`, `Stopped`, and `ReviewCiWait`. None means merged
or complete. The private receipt retains the terminal implementation result. PR
publication is a separate authenticated intent through `publish_worker_pr`.
The exact remote PR, marker, repository, branch, head, base, number, and node
identity must match. Review/CI waits retain the issue slot. There is no release,
replacement, repair-authorize, merge, or completion API in this slice.

`NativeWorker` sends the supported flat `session.create` fields `request_id`,
`source`, `cwd`, and `skills`. It submits the issue URL separately with
`prompt.submit`. It reads `runtime.describe`, `session.info`, and
`prompt.receipt`, and cancels with `prompt.cancel`. Profile/OS identity is not
an invented session parameter. Admission `queued`, `started`, `unknown`, and
`terminal` states retain authority epoch and execution generation. Acknowledged
admission is not successful execution. An unknown result stops dependent work.

The native `skills` loading mechanism can skip missing names when another name resolves.
The trusted host must resolve **every** mandatory skill and supply actual content
digests, then verify the actual frozen loaded content before submission. The
requested-name echo in `launch_request`, a catalog listing, or slash-prefixed
text is not that proof. The required-skill set must include repository and
workflow requirements, not just `implement`. Bypass launches are not used.

`NativeHost` is an explicit trusted supervision/extension boundary, not a new
Hermes wire protocol. It must establish distinct controller/worker OS users,
exclude workers from private credentials and ownership files, provide model and
gateway credentials, and enforce a **synchronous mandatory review-before-correction gate**.
That gate must stop a failed internal review before another corrective action
can execute, and must remain closed until the future shared-budget workflow
publishes durable repair authorization. `accept_hooks` only consents to shell
hooks. Polling and racing cancellation cannot implement this prerequisite.
The stock gateway contract inspected for this ticket does not demonstrate this
gate or an all-skills frozen-content receipt. `UnprovisionedHost` therefore
refuses admission. No weaker fallback is available.

`NativeRpcCommand` is a transport adapter for an operator-supplied supported
authenticated native JSON-RPC bridge executable. It is **not** a claim that
Hermes ships a command with that interface. The library selects and installs no
default bridge. The bridge receives one JSON-RPC packet on `stdin`, no arguments, and
only the explicitly selected gateway credential as `HERMES_GATEWAY_TOKEN`.
Controller App credentials are not inherited. Production must provision and
validate that bridge or supply another supported `NativeRpc` connection.

The adapter may cache acknowledged replies in memory, but that cache is not recovery
authority. After an uncertain send or adapter restart, `NativeHost::exact_reply`
must obtain the original exact native receipt through a provisioned supported
interface. Missing lookup support, a missing result, or ambiguous evidence stops
the action. The adapter does not repeat create/submit to discover their effects,
derive undocumented native IDs, search for a newest session, or start a new run.

`CapturedPoolInspector` reads the original private pool-receipt acquisition files
and invokes only `assignment inspect` for the retained handle through the
approved wrapper. Its constructor requires explicit approved private receipt and
catalog directories; cleared subprocess environments do not select defaults.
It compares the original catalog authority, assignment, acquisition operation,
repository, worktree, and lossless Unix path bytes. Pending, unknown, capture
errors, changed identities, or non-UTF-8 `cwd` bytes stop admission. It never
acquires, releases, recovers, expands capacity, or substitutes a checkout.

Retained post-admission observation reads the authenticated exact session and
admission even while execution inventory reports that issue occupied. It retains
all other authorization checks and cannot send create or submit. New admission
and PR publication still require execution clearance. Submission readback preserves
an acknowledged execution generation and authority epoch; only a queued admission
without a generation can acquire its first generation.

`GitHubPublications` reuses the existing GitHub `Transport` and requires a
separately scoped publication credential. It reads all configured pages, checks
the branch commit identity before posting, adds the stable publication marker, and reads
back the exact PR. A matching existing marker prevents another publication.
Ambiguity or an exhausted page bound stops reconciliation.

### Worker operations

The Software Factory operator owns these procedures. Authority changes require a maintainer.

| Condition                                                   | Evidence and stop                                                                                                 | Resumption                                                                                                                      |
| ----------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Missing native/skill/isolation/credential/gate prerequisite | No worker creation or submission. Retain selected issue and original acquisition.                                 | Operator supplies reviewed, exact bound evidence; no transport/worktree fallback.                                               |
| Lost create/submit/publication acknowledgement              | Retain original action and run. Inspect its exact native/remote target. Absence does not permit resend.           | Only confirmed matching evidence can advance; otherwise maintainer reconciliation.                                              |
| Internal review failure                                     | Trusted synchronous gate already prevents correction. Controller persists `RepairBlocked` and findings reference. | Future shared-budget workflow must durably authorize repair; this API cannot clear the stop.                                    |
| Cancellation                                                | Persist exact cancellation intent. A request acknowledgement leaves `Cancelling`.                                 | `Stopped` requires trusted exact cessation, reconciled effects, and preserved unfinished work. Slot and assignment remain held. |
| Implement success or review/CI wait                         | `Implemented` is not publication or completion. PR intent/readback is separate.                                   | Later review/merge tickets provide acceptance. The controller selects no new originating issue.                                 |

All preceding production prerequisites remain unverified. Fixture success does not
authorize live workers, PRs, pool changes, provisioning, or activation. The final
readiness ticket retains the single maintainer activation gate.

## Validation

```sh
bazel run //:gazelle
aspect format --scope=all
aspect test //tools/software_factory:acceptance_test
aspect test //tools/software_factory:worker_test
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

The native worker adapter follows the official
[Gateway local sessions](https://hermes-agent.nousresearch.com/docs/developer-guide/gateway-local-sessions)
contract. The inspected native source includes `gateway/session_controls.py`,
`gateway/session_policy.py`, and the canonical admission and session contracts.
These sources establish protocol shapes, not deployed host compatibility.

## AFK research workflow

`wayfinder:research` authorizes research only. The same selection, complete native
reads, ownership, authenticated history, protected receipts, original pool capture,
explicit frozen skills, and one-active-issue rules apply. Waiting labels suppress
admission even if `ready-for-agent` is present. Decision-type labels do not become
implementation permission. The required trusted skill manifest must contain both
`research` and `ask-matt`, plus repository requirements. An omitted, unresolved,
or incompletely frozen required skill stops the operation before submission.
Implementation workflow skills cannot replace research.

The worker follows the explicitly loaded research workflow and originating issue
contract. It returns public findings with source references. The native terminal
`final_response` contains this JSON object; a directly structured object is also
accepted at the runtime boundary:

```json
{
  "findings": "Cited Markdown findings under the originating issue contract.",
  "evidence": ["https://example.test/primary-source"],
  "human_question": null
}
```

These fields are public. They must not contain secrets, private pool captures, or
private native receipts. Unknown output fields, including implementation PRs,
merge authority, and worker-supplied human answers, are rejected. Empty findings,
missing sources, and empty human questions are rejected. The retained terminal
result cannot be replaced with a different answer after restart.

Successful native execution enters `ResearchReady`, not `Implemented`. The
controller publishes `ResearchFindings` through its existing authenticated history
path and reads back the exact comment, App identity, payload, and continuity.
It then publishes and verifies `ResearchPublished`, which binds the confirmed
comment identity and digest. Only then does it checkpoint `ResearchComplete`.
Research completion is not a PR, merge, pool release, or activation decision.
The receipt and issue slot remain retained under the existing stopped-job policy;
this library does not add a release or replacement API.

A non-null `human_question` remains unanswered. Findings publication and the
`HumanInputRequired` stop precede a separate authenticated submission to the
explicitly loaded `ask-matt` workflow in the retained native conversation. The
original reserved publication action identity binds that handoff rather than a
PR. Its separate admission and generation are retained for exact cancellation.
The controller accepts no agent response as human input and provides no automatic
readiness-restoration or implementation-conversion API. Native admission confirms
the handoff submission, not that a human answered or that a downstream alert arrived.

The common mutation seam rejects research PR publication, review checks, hosted
reviews, merge, and arbitrary replacement prompts. It permits only the exact
bound native creation, research submission, unanswered handoff, cancellation,
and the existing notification interface. The trusted native host must preserve
worker credential isolation and authorize only these effects. A prompt is not a
substitute for those host prerequisites.

### Research recovery and operation

Owner: the Software Factory operator; a maintainer controls authority changes.

| Condition                                        | Required evidence and stop                                                                                                                                    | Resumption                                                                                                                                      |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Missing research skill or host prerequisite      | Retain the selected issue. Send no native mutation.                                                                                                           | Resolve and verify the complete trusted manifest and existing native prerequisites.                                                             |
| Interrupted findings or confirmation publication | Retain the original protected pending record and terminal result. No completion without exact readback.                                                       | Adopt only the exact authenticated comment and finish only unpublished confirmation/checkpoint work. Remote absence never permits another send. |
| Interrupted human handoff                        | Retain the unanswered question, original submission action, and exact admission when known. Keep the slot occupied.                                           | Reconcile the original action. Reporting/handoff recovery may finish with waiting labels present; it cannot admit another research run.         |
| Actual human decision needed                     | Preserve `HumanInputRequired`; use the loaded `ask-matt` workflow and existing `needs-info` policy. Worker output and readiness labels are not human answers. | A real human supplies the answer, and the maintainer follows the existing reassessment policy. This slice grants no automatic re-entry.         |
| Research complete                                | Retain verified findings, publication confirmation, native result, pool receipt, and ownership evidence.                                                      | Apply the existing stopped-job policy; do not infer pool release or implementation authority.                                                   |

All research tests use controlled fixtures. They do not establish deployed native
capabilities, App permissions, OS isolation, real human delivery, or production
readiness. No live research fixture or activation is authorized by this work.
