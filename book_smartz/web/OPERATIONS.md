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

Each fact permits only event identity, operation kind, outcome, stage,
bounded reason, duration, release, process-instance ID, and operation ID.
The stderr record contains `fact` and `severity`.
These catalog, identity, and read operations have no original-action ID.
Persistence and original-save checks retain their existing records until their separate diagnostic tickets complete.

| Operation kind   | Confirmed meaning                                                    |
| ---------------- | -------------------------------------------------------------------- |
| `catalog_search` | The app prepared search results and selections                       |
| `sign_in_start`  | The app stored the challenge and prepared the redirect               |
| `sign_in`        | Account and session persistence received a commit acknowledgement    |
| `sign_out`       | Revocation received an acknowledgement and session cleanup completed |
| `ranking_read`   | The app loaded saved-ranking data and display metadata               |

A confirmed `sign_in_start` does not mean that the reader signed in.
Each endpoint attempt has its own operation ID.
OAuth state, session tokens, and reader identities do not supply diagnostic correlation.

| Outcome or reason                                  | Meaning                                                        | Severity |
| -------------------------------------------------- | -------------------------------------------------------------- | -------- |
| Confirmed                                          | The named operation completed                                  | `INFO`   |
| Identity rejected or invalid input                 | The app rejected the attempt                                   | `INFO`   |
| Admission limited or provider limited              | The applicable limit rejected the attempt                      | `INFO`   |
| Timeout, unavailable, malformed, oversized         | The provider operation failed                                  | `WARN`   |
| Database, schema, or persistence failure           | A storage operation failed                                     | `ERROR`  |
| Corrupt history                                    | The app could not interpret the stored ranking                 | `ERROR`  |
| Session operation or selection preparation failure | The named preparation failed                                   | `ERROR`  |
| Interrupted                                        | Work stopped at its last known stage without a terminal result | `WARN`   |

Stages distinguish authentication, validation, admission, provider access,
selection preparation, persistence, session work, and session revocation.
Missing relation, column, or schema SQLSTATE codes identify schema failures.
Other database errors retain the bounded database reason.
The listener never formats dependency messages.
Provider unavailability does not identify its underlying cause.

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

Delivery retains the 256-record queue. Producers do not wait for output.
Full or closed destinations can discard records without changing app results.
The listener owns severity, formatting, and stderr output.
A missing diagnostic record proves neither failure nor rollback.
Successful stderr output does not prove platform ingestion.
Diagnostic records remain advisory. Confirm saved outcomes from authoritative state.
An operation fact describes an attempt, not a business-activity count.
Reading or replaying saved state can emit read facts but produces no new business activity.

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
