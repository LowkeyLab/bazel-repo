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

Restrict platform logs and database access to the maintainer. Routine structured
records permit only outcome category, outcome, elapsed milliseconds, release
fingerprint, and an optional opaque action ID. They exclude reader IDs, Google
subjects, email, searches, titles, ranking contents, cookies, tokens, selections,
raw requests, SQL, and database errors. Never paste private evidence into public
issues or logs. Action IDs belong in diagnostic records, never metric dimensions.

Catalog, authentication, persistence, and checking outcomes identify the area to
investigate. Confirm success using durable state. A diagnostic record is advisory.
Delivery uses a bounded queue and best effort output. A missing record doesn't prove failure or
rollback. Replaying saved state produces no new business activity.

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
