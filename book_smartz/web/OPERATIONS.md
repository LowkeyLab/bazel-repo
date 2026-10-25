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
Catalog searches emit `OperationFinished` facts with `CatalogSearch` as the operation kind.
The stderr values are `operation_finished` and `catalog_search`.
The producer captures one opaque operation ID per attempt and one process-instance ID per serving process.
The release fingerprint comes from the verified package.
Elapsed duration uses a monotonic clock and whole milliseconds.
Listeners receive this closed field allowlist before they select severity or delivery policy.

The catalog fact permits only event identity, operation kind, outcome, stage,
bounded reason, duration, release, process-instance ID, and operation ID.
The structured stderr record contains `fact` and `severity`.
Catalog searches have no original-action ID.
Other operations retain their existing category, outcome, duration, release, and optional action ID records until their separate diagnostic tickets complete.

| Outcome or reason                          | Meaning                                                        | Severity |
| ------------------------------------------ | -------------------------------------------------------------- | -------- |
| Confirmed                                  | The search and selection preparation completed                 | `INFO`   |
| Invalid input                              | The app rejected the submitted search                          | `INFO`   |
| Admission limited                          | The existing shared admission policy rejected the attempt      | `INFO`   |
| Provider limited                           | The provider returned its rate limit                           | `INFO`   |
| Timeout, unavailable, malformed, oversized | The provider operation failed                                  | `WARN`   |
| Database or configuration failure          | Admission or selection preparation failed                      | `ERROR`  |
| Interrupted                                | Work stopped at its last known stage without a terminal result | `WARN`   |

Stages distinguish validation, admission, provider access, and selection preparation.
Catalog observation starts after authorization and cross-site request forgery validation.
Missing relation, column, or schema SQLSTATE codes identify schema failures.
Other admission errors identify database failures.
The listener never formats their messages.
Interruption does not invent a dependency cause.
An HTTP disconnect does not prove that the framework cancelled the operation.
Abrupt process termination can prevent a terminal fact.
The existing provider budgets, deadlines, rejection responses, and saved state remain unchanged.

The allowlist excludes reader IDs, Google subjects, email, searches, titles,
ranking contents, cookies, tokens, selections, raw requests, SQL, and dependency errors.
Never paste private action evidence into public issues or logs.
Opaque operation, process, and action identifiers must not become metric dimensions.
This change adds no metrics, exporter, or operational endpoint.

Delivery retains the 256-record queue. Producers do not wait for output.
Full or closed destinations can discard records without changing app results.
The listener owns severity, record formatting, and stderr output.
A missing diagnostic record proves neither failure nor rollback.
Successful stderr output does not prove platform ingestion.
Diagnostic records remain advisory. Confirm saved outcomes from authoritative state.
Replaying saved state produces no new business activity.

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
