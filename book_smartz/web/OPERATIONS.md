# Book Smartz maintainer operations

The maintainer operates one app and private PostgreSQL. Review health, diagnostics,
unresolved actions, and resource usage manually. Local acceptance and image builds
prove neither live provider compatibility nor deployment. Backups and disaster
recovery remain deferred.

## Release and health

Build `//book_smartz/web:package` and `//book_smartz/web:image`. The image contains
`/book-smartz`, `/assets`, and `/integrity.json`. Its default command serves HTTP.
Configure the pre-deploy command as `/book-smartz migrate` with a two-minute timeout.
A failed command must block rollout. Requests never migrate. Preserve old images.
Compatible schemas support overlap without destructive rollback.

Use the existing main-test-triggered Deploy workflow and its exact source commit.
`//book_smartz/web:push_image` publishes `ghcr.io/lowkeylab/book_smartz:latest`.
Record source commit, successful test run, published digest, and Railway's running
digest separately. Verify the intended digest, packaged assets, readiness,
Google sign-in, discovery, Add, comparison, saved ranking, and restart continuity
against the deployed instance. Publication alone doesn't establish these facts.

Set `HOST=0.0.0.0`, the exact HTTPS canonical origin, and runtime secrets described
in the README. Keep the selection key stable across overlap and restarts. Only
the app receives public ingress. Use private database references. Readiness at
`/health/ready` checks database, schema, and package integrity without contacting
Google or Open Library. One background verifier checks integrity on a blocking
worker, waits one second between checks, and expires successful verification
after three seconds. Concurrent probes share that result and do no package
I/O or hashing. The detached verifier runs outside HTTP drain and runtime
shutdown. A provider outage can affect its own operation while the
app remains ready. A database/schema/asset failure requires investigation before
admitting traffic.

On termination, stop admission and allow 20 seconds for active requests, followed
by at most five seconds for pool closure. Set Railway termination grace to 30
seconds. An interrupted write may remain checking. Cancellation never proves
rollback. Test restart with an existing browser session and ranking.

Each serving instance allows five database connections, with five-second
acquisition/lock and fifteen-second statement timeouts. Migrations use a separate
single-connection pool. Budget at least 11 app connections for two overlapping
instances plus migration, as well as maintainer/provider overhead. These initial
settings aren't capacity measurements.

## Diagnostics and private evidence

Restrict platform logs and database access to the maintainer.
Observed operations emit typed `OperationFinished` facts.
The producer captures one opaque operation ID per attempt and one process-instance ID per serving process.
The release fingerprint comes from the verified package.
Elapsed duration uses a monotonic clock and whole milliseconds.
Listeners receive the closed field allowlist before they select severity or delivery policy.

Each operation fact permits only event identity, operation kind, outcome, stage,
bounded reason, duration, release, process-instance ID, operation ID, and optional original-action ID.
The stderr record contains `fact` and `severity`.
These catalog, identity, and read operations have no original-action ID.
Mutations include the existing original-action ID after authorization and offer validation.
Original-save checks retain the submitted original-action ID after authorization and cross-site request forgery validation.
The ID does not identify the reader or contain action contents.
Mutation and original-save paths also retain existing persistence records.
Original-save checks use the same typed boundary.

| Operation kind        | Confirmed meaning                                                                          |
| --------------------- | ------------------------------------------------------------------------------------------ |
| `catalog_search`      | The app prepared search results and selections                                             |
| `sign_in_start`       | The app stored the challenge and prepared the redirect                                     |
| `sign_in`             | Account and session persistence received a commit acknowledgement                          |
| `sign_out`            | Revocation received an acknowledgement and session cleanup completed                       |
| `ranking_read`        | The app loaded saved-ranking data and display metadata                                     |
| `add`                 | The original Add has complete saved evidence                                               |
| `preference`          | The original preference has complete saved evidence                                        |
| `pause`               | The original Pause has complete saved evidence                                             |
| `resume`              | The original Resume has complete saved evidence                                            |
| `original_save_check` | The original save received an acknowledgement, or the app verified complete saved evidence |

A confirmed `sign_in_start` does not mean that the reader signed in.
Each endpoint attempt has its own operation ID.
OAuth state, session tokens, and reader identities do not supply diagnostic correlation.
Mutation observation starts after form parsing, before handler validation.
A rejected form that cannot enter its handler has no mutation fact.

A new save requires a commit acknowledgement.
A duplicate attempt can confirm complete retained evidence with reason `existing_saved`.
That result does not count as another save.
Each duplicate attempt has a new operation ID and retains the original-action ID.
An `original_save_check` with reason `none` confirms a newly acknowledged original save.
Reason `existing_saved` identifies complete retained evidence without another save.
Rejected original-save checks with reason `action_rejected` identify an authoritative rejection for the original action.
An invalid or unknown action uses `invalid_input` instead.
Unresolved checks retain uncertainty and keep the original-action identity stable.
Each attempt receives a new operation ID.

Legacy persistence records still describe newly acknowledged effects.
Mutation terminal facts describe attempts and do not replace authoritative action evidence.

| Outcome or reason                                  | Meaning                                                         | Severity |
| -------------------------------------------------- | --------------------------------------------------------------- | -------- |
| Confirmed                                          | The named operation completed                                   | `INFO`   |
| Identity rejected or invalid input                 | The app rejected the attempt                                    | `INFO`   |
| Admission limited or provider limited              | The applicable limit rejected the attempt                       | `INFO`   |
| Timeout, unavailable, malformed, oversized         | The provider operation failed                                   | `WARN`   |
| Database, schema, or persistence failure           | A storage operation failed                                      | `ERROR`  |
| Corrupt history                                    | The app could not interpret the stored ranking                  | `ERROR`  |
| Session operation or selection preparation failure | The named preparation failed                                    | `ERROR`  |
| Interrupted                                        | Work stopped at its last known stage without a terminal result  | `WARN`   |
| Uncertain persistence with commit uncertainty      | The app cannot establish the original save outcome              | `WARN`   |
| Uncertain persistence with interruption            | Work stopped after persistence could begin                      | `WARN`   |
| Action rejected or blocked                         | A domain decision or an unresolved action prevents this attempt | `INFO`   |

Stages distinguish authentication, validation, admission, provider access,
selection preparation, persistence, session work, and session revocation.
Before persistence can begin, known failures remain dependency failures.
After an intent can persist, an error or cancellation retains uncertain persistence until complete saved or rejected evidence resolves it.
A known database, schema, or persistence failure remains `ERROR`, including during unresolved persistence.
An interrupted attempt reports its last known stage and does not invent a cause.
Abrupt process termination can prevent any terminal fact.

Missing relation, column, or schema SQLSTATE codes identify schema failures.
Other database errors retain the bounded database reason.
The listener never formats dependency messages.
Provider unavailability does not identify its underlying cause.
The token reader recognizes HTTP 400 with the allowlisted `invalid_grant` code as an expected identity rejection.
[OAuth 2.0 specification section 5.2](https://www.rfc-editor.org/rfc/rfc6749.html#section-5.2) defines this protocol response.
The reader ignores error descriptions and preserves the existing response-size limit and deadline.
Other token-error codes do not establish an identity rejection.

The protected ranking route and sign-out route observe authorization rejections.
Catalog observation starts after authorization and cross-site request forgery validation.
Saved-ranking observation continues through asynchronous data loading.
Other authorized pages also emit read facts when they load a saved-ranking section.
A completed read does not prove that the browser received the response.

Interruption does not invent a dependency cause.
An HTTP disconnect does not prove that the framework cancelled the operation.
A dependency failure or interruption during a write does not prove rollback.
A session cleanup failure does not undo an acknowledged revocation.
Abrupt process termination can prevent a terminal fact.
Provider budgets, deadlines, authorization, session behavior, and saved state remain unchanged.

The allowlist excludes reader IDs, Google subjects, email, searches, titles,
ranking contents, cookies, tokens, selections, raw requests, SQL, and dependency errors.
Never paste private action evidence into public issues or logs.
Opaque operation, process, and action identifiers must not become metric dimensions.
These diagnostics add no metrics, exporter, or operational endpoint.

Delivery retains the 256-record queue. Normal queue producers do not wait for output.
Full or closed destinations can discard records without changing app results.
The listener owns severity, formatting, and stderr output.
A missing diagnostic record proves neither failure nor rollback.
Successful stderr output does not prove platform ingestion.
Diagnostic records remain advisory. Confirm saved outcomes from authoritative state.
An operation fact describes an attempt, not a business-activity count.
Reading or replaying saved state can emit read facts but produces no new business activity.

### Initialization diagnostics

`StartupFinished` describes serving initialization before the first accepted request.
Success requires package verification, configuration, schema checks, and a bound listener.
Provider initialization checks configuration. It does not probe Google or Open Library.
A startup fact does not establish continuous readiness or future serving success.

`MigrationFinished` describes the explicit migration command.
Its 120-second limit includes connection establishment, migrations, and pool closure.
The command retains its original success or failure exit status.
Diagnostic failure cannot change that status.

Initialization facts contain event identity, outcome, stage, bounded reason, monotonic duration, process-instance ID, and available verified release context.
They contain no operation or original-action ID.
Outcomes are `succeeded`, `failed`, and `deadline_exceeded`.
Stages distinguish configuration, database, package, provider, schema, listener, and completion.
Reasons distinguish configuration, database, package, provider, schema, listener unavailability, and timeout.
Successful results use `INFO`. Failed results and deadline expiration use `ERROR`.
A database error during schema work retains that stage and identifies the database cause.
Reasons never derive from formatted dependency messages.

The process-instance ID starts with command initialization and continues into normal serving records.
Release context appears only after package verification succeeds.
The migration command does not require package assets and omits unverified release context.

Early failures and migration completion use a minimal fallback before normal listeners exist.
The fallback starts a detached output worker and waits at most 250 milliseconds for its result.
A blocked worker cannot extend that wait. Output remains best effort.
Successful serving initialization uses the normal queue.
A known stopped worker returns the fact before it enters the queue.
Composition can use the bounded fallback.
An occurrence uses one delivery path. Queue saturation remains a counted drop without a fallback retry.
Complete output failure or process termination can prevent a record.
Readiness and shutdown observations remain separate work.

### Diagnostic delivery loss

The worker retains process-local lifetime counts for queue drops and destination failures.
A full or closed queue increments `queue_drops` and discards that record.
A failed output attempt increments `destination_failures`, including a failed summary attempt.
Failed records never return to the queue.
After an output failure, the next attempt writes a newline before a fresh record.
A partial failed record can remain invalid, but it cannot consume the next record boundary.
Counters saturate at the maximum unsigned 64-bit value and reset with the process.
Their dimensions contain no reader or action identifiers.

After a new record reaches stderr, the worker can emit a `DiagnosticLossObserved` summary at `WARN` severity.
The summary reports pending loss since the last successful summary.
Reported totals advance only after summary output succeeds.
Loss that occurs during summary delivery remains pending for a later successful record.
The summary neither restores discarded records nor retries their delivery.
There is at most one summary attempt per newly delivered record.

Summary fields are event identity, queue drops, destination failures, worker status, release, and process-instance ID.
A summary has no operation ID, original-action ID, or operation duration.
The worker status is `running` while its thread remains active and `stopped` after it ends or cannot start.
A running worker can still wait on blocked output.
Worker status does not establish destination progress or platform ingestion.

A failed summary does not trigger another output attempt through the failed path.
Only another successfully delivered record can trigger a new summary with accumulated counts.
Complete stderr failure can leave no visible indication in the app.
Process exit can lose buffered records, counters, and pending summaries.
These counters do not alter readiness or authoritative history.
They add no endpoint, local control socket, exporter, or maintainer authentication mechanism.

Configure and verify the destination's retention against the seven-day target.
Record actual access and retention settings during launch. Local code can't
verify Railway policy. Authoritative app/ranking history remains in
PostgreSQL and doesn't expire with diagnostic logs.

## Unresolved actions

Inspect immutable app events in `book_smartz.application_events` and
ranking envelopes in `book_smartz.ranking_events` using privileged private database
access. Read the actual table definitions before querying. Correlate the action
with its frozen intent and prepared ranking envelope. Match the original event
identity and complete contents. An ID match alone is insufficient. Registration
may have committed independently even when ranking didn't.

Use the reader's authorized **Check original save** POST to reconcile/retry the
original action only. Preserve its identity, command, timestamp, revision, and
event envelope. The existing pipeline synchronizes with in-flight commands.
An absent unlocked read is inconclusive. Keep new mutations blocked until you
establish the original outcome. Never invent success, alter history, manufacture
a replacement receipt, or release a block based only on timeout or cancellation.

## Usage review

Measure app and PostgreSQL usage together, the observation window,
projected monthly cost, and shared workspace headroom. The $5 monthly target allows possible overages. Review connection usage during release
overlap and migration, response times, storage/history growth, and diagnostic
volume before changing initial limits. Record actual measurements and limitations.
