# Book Smartz web app

The first web slice signs in with Google, persists browser sessions in PostgreSQL,
and reads the existing private ranking and pending placement. Signed-in readers can
explicitly search Open Library, page through work-level results, and select a
validated work. Add saves the validated work and display metadata independently,
then applies the existing Start decision. The first book appears at rank 1 only
after the database confirms the ranking event. Later books remain pending while the reader chooses either displayed book.
Pause/Resume controls belong to the following ticket.

Build the relocatable archive with `aspect build //book_smartz/web:package`.
It contains `book-smartz`, adjacent `assets`, and `integrity.json`. Run it from
any directory. `serve` verifies executable/asset integrity and the required schema
before listening. Ordinary requests never apply migrations.

Supply `DATABASE_URL` to `book-smartz migrate`. Supply the same database plus
`BOOK_SMARTZ_ORIGIN`, `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `BOOK_SMARTZ_MAINTAINER_CONTACT`,
and `BOOK_SMARTZ_SELECTION_KEY` to
`book-smartz serve`. The origin must be the exact canonical HTTPS origin without
a trailing slash. Register `<origin>/auth/google/callback` with the Google client.
Keep credentials in runtime configuration. `HOST` and `PORT` configure the listener.
Use `HOST=0.0.0.0` behind the intended HTTPS reverse proxy.

`bazel run //book_smartz/web:run -- migrate` and
`bazel run //book_smartz/web:run -- serve` use that same archive and forward arguments.
The migration command has a two-minute bound. The initial pool allows five
connections, with five-second acquisition/lock and fifteen-second statement
timeouts. These are settings, not measured capacity claims.

Sessions use opaque Topcoat tokens, hashed records, explicit secure HttpOnly
cookies, a fixed 30-day expiry, and browser-scoped revocation. Ownership follows
Google issuer/subject rather than email. Logout requires its own session-bound cross-site request forgery
token and the configured canonical Origin. Responses use a strict-origin referrer policy
to preserve native form Origin headers without disclosing URL paths or queries.
Ranking GETs derive reader authority
from the stored session. Reader identifiers supplied by browsers confer none.
The app installs a silent storage observer so existing reader-identifying
storage diagnostics don’t become routine web diagnostics. The operations and
release slice adds its restricted diagnostic listener and operations guide.

The HTTP and app suites run the actual archive with isolated PostgreSQL
and controlled Google token/JSON Web Key endpoints. Package acceptance checks delivered
CSS, local fonts, icons and runtime JavaScript from unrelated directories, plus
startup rejection of missing/mismatched assets and incompatible schema. These
checks don’t establish live Google compatibility or launch readiness.

Run `aspect test //book_smartz/web:http_test
//book_smartz/web:application_test //book_smartz/web:package_test` on one shell line.
The optional `GOOGLE_AUTHORIZATION_ENDPOINT`, `GOOGLE_TOKEN_ENDPOINT`, and
`GOOGLE_JWKS_ENDPOINT` settings let acceptance fixtures control provider I/O.
Production defaults use Google's endpoints. HTTP overrides accept only explicit
loopback addresses. The app contains no fixture identity or bypass route.

The protocol checks follow [Google's OpenID Connect validation requirements](https://developers.google.com/identity/openid-connect/openid-connect)
and [Proof Key for Code Exchange](https://www.rfc-editor.org/rfc/rfc7636.html).

Search uses [Open Library's work search endpoint](https://openlibrary.org/dev/docs/api/search)
with 20 results per page and direct [CoverID images](https://openlibrary.org/dev/docs/api/covers).
Requests identify Book Smartz and the configured maintainer contact. The app
limits queries to 256 characters, provider requests to five seconds and responses
to 1 MiB. Failures offer explicit retry controls. Results have no cache or
automatic retry. Cover failure falls back to the packaged image in the browser.

Catalog admission uses its own migration schema and ledger. The previous release
retains its identity schema and readiness contract during overlap.

PostgreSQL coordinates the deployment-wide one-search-per-second budget and a
rolling ten-searches-per-reader-per-minute budget. All instances, including
releases in overlap, must share that database. The app rejects excess requests
without waiting on another search. Failed provider calls consume admission.

Selections sign validated work identity and display data for the authenticated
reader, expire after 15 minutes, and verify without a provider call. Supply a
random secret of at least 32 bytes as `BOOK_SMARTZ_SELECTION_KEY`. Keep it stable
across restarts and release overlap. Rotating it invalidates outstanding
selections and requires a fresh search. Signatures authenticate data without encrypting it.
Selections stay in protected, non-cacheable responses. Never log them.
`OPEN_LIBRARY_SEARCH_ENDPOINT` defaults to the production search endpoint.
HTTP overrides accept explicit loopback addresses for controlled acceptance.
Actual provider registration and live compatibility remain launch acceptance.

## Add and original-action recovery

The selected-work page offers a signed action bound to that reader, selection, and
displayed revision. The first Add freezes the complete intent, command time in Coordinated Universal Time
and ranking-event ID in immutable app history before registration. A
preparation fact precedes registration. A second retains the exact ranking
envelope before its append. Correlation uses the explicitly accepted experimental
[Correlation extension at revision 472f0e1](https://github.com/cloudevents/spec/blob/472f0e1d856e335c30fa204f36fd6428c2bfd732/cloudevents/extensions/correlation.md).
App facts and ranking events have separate sources and sequences.

The accepted ranking event is the sole success evidence. There is no separate
completion receipt. Repeated submissions recover the original intent even after
selection expiry. A changed selection, action, or revision can't reuse an offer.
Registration and shared metadata may remain committed after the domain rejects Start.
blank author/cover fields preserve previously saved data. Rankings use retained
metadata and local fallback for older identities without provider requests.

An interrupted or unacknowledged action stays checking and blocks new ranking
actions. Reload exposes **Check original save**, an authorized, origin and cross-site request forgery protected
POST. Recovery first reconciles the original event identity and full envelope.
A missing read is inconclusive. Execution of an unresolved original command
acquires the same reader-stream lock as storage commands, preserves all frozen
values, and rechecks evidence before appending. The app retains definitive domain rejection without advancing ranking revision. Conflicting evidence stays blocked.
Reads never execute recovery.

The app acceptance suite includes real PostgreSQL registration races,
failed append, expired-selection recovery, conflicting event contents, lost real
COMMIT acknowledgements, and process interruption. Controlled-provider/local
results don't establish deployment or live-provider readiness.

## Preference comparisons

The main workspace shows the pending candidate and either-book preference controls.
Saved ranks stay beside it on desktop and below it on small screens. Progress counts
saved preferences without predicting a comparison total. The Add control waits until the reader completes the placement and confirms any
uncertain save. The server independently enforces this gate.
New web actions accept only candidate/opponent preference, preserving historical Skip replay.

Each offer binds the reader, action, candidate, displayed opponent, and revision.
The first submission freezes its choice, command time, and event identity in immutable
preference intent. Preparation and exact-envelope reconciliation use the same
reader-locked pipeline as Add. Duplicate submissions recover the original outcome.
A changed choice can't reuse that action. Stale comparisons require a fresh
confirmation, and sign-in never submits an earlier unconfirmed preference.
Checking blocks new actions until authorized recovery reconciles the original evidence.
Ranking/comparison reads and recovery use retained data without consulting Open Library.

`//book_smartz/web:browser_test` runs Chromium against the delivered package,
isolated PostgreSQL, and controlled Google/Open Library endpoints. The journey
covers sign-in, search, first/later Add, keyboard preference, submitting feedback,
a stale tab, responsive layout, and reload. The browser retains the canonical HTTPS
origin and secure cookies while the fixture redirects transport to loopback.
The app suite covers ordering, action/security boundaries, expiry, and real
lost-commit-acknowledgement and process-cancellation recovery. These checks provide
controlled acceptance evidence. Live-provider, deployment, and Pause/Resume acceptance
remain separate delivery work.
