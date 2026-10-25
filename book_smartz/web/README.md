# Book Smartz web app

The web app signs in with Google, persists browser sessions in PostgreSQL,
and reads the existing private ranking and pending placement. Signed-in readers can
explicitly search Open Library, page through work-level results, and select a
validated work. Add saves the validated work and display metadata independently,
then applies the existing Start decision. The first book appears at rank 1 only
after the database confirms the ranking event. Later books remain pending while the reader chooses either displayed book.
Pause for now preserves the unfinished comparison. Resume restores it after reload, restart, or another browser login.

The Comparison desk places saved positions beside the active task on desktop.
Phone layouts place the active task before saved positions. The app uses local
`Literata` and Source Sans 3 fonts. See [font licenses](fonts/README.md).
Native forms work without JavaScript. JavaScript announces Saving… during submission
and preserves the chosen submitter value. The server controls saved positions and
completion eligibility. Only pointer input can decorate a fresh confirmed placement.
The opacity transition takes 160 ms. Reduced motion shows the final highlight immediately.

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
storage diagnostics don't become routine web diagnostics. The web listener emits only allowlisted app facts through a bounded,
best-effort queue. See [maintainer operations](OPERATIONS.md) for release, health,
private diagnostics and original-action recovery.

The HTTP and app suites run the actual archive with isolated PostgreSQL
and controlled Google token/JSON Web Key endpoints. Package acceptance checks delivered
CSS, local fonts, icons and runtime JavaScript from unrelated directories, plus
startup rejection of missing/mismatched assets and incompatible schema. These
checks don't establish live Google compatibility or launch readiness.

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

The accepted ranking event is the sole success evidence. No separate durable
completion fact records success. The app signs a temporary display cookie after a
confirmed save. The cookie expires after 60 seconds and belongs to the authenticated
reader. The cookie controls display feedback only. Recovery and repeated
submissions show confirmation without replaying the completion highlight.

Repeated submissions recover the original intent even after
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
New preference actions accept only candidate/opponent answers, preserving historical Skip replay.
Pause and Resume each require an independently authorized POST with session-bound
cross-site request forgery protection and canonical Origin validation. Their signed
offers bind the pending identity, operation, and revision. They use the same frozen
intent and exact-envelope recovery pipeline as preferences, without assigning a rank
or permitting a replacement candidate. Resume retains prior preferences and insertion
bounds. Expired sessions require sign-in and a deliberate submission. Sign-in never
executes an earlier command.

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
covers sign-in, search, first/later Add, keyboard preference, Pause/Resume,
submitting feedback, a stale tab, responsive layout, reload, missing covers, search
failure, checking recovery, and explicit Resume after session expiry. The browser retains the canonical HTTPS
origin and secure cookies while the fixture redirects transport to loopback.
The app suite covers ordering, action/security boundaries, expiry, and real
lost-commit-acknowledgement and process-cancellation recovery. These checks provide
controlled acceptance evidence. Live-provider and deployment acceptance remain
separate delivery work.

The `browser_collection_test` target runs the same packaged journey through a
native Node test entry point. This retains Node child profiles for the journey.
The native Rust test retains `LLVM` objects and child profiles.
Normal fixture teardown requests `SIGTERM` before its bounded `Child::kill` fallback.
Explicit cancellation tests retain their direct `Child::kill` calls.
Chromium supplies computed appearance data. Node calculates contrast and validates
those results. Native Node profiles measure Node execution. Browser execution
remains outside those profiles. Browser assertions validate submission behavior.

The browser records every acceptance scene at 320 × 900, 390 × 900, and
1440 × 900. `browser-evidence.json` contains the Chromium version, viewport,
visible-control bounds, and computed foreground/background contrast measurements.
The measurements composite ancestor backgrounds and the placement highlight.
They use the [W3C sRGB luminance formula](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html#dfn-relative-luminance).
Normal text must reach 4.5:1. Large text, control boundaries, and visible focus
outlines must reach 3:1. The checks include inactive controls. These measurements cover the rendered
acceptance scenes. Web Content Accessibility Guidelines conformance requires
separate review.

Full-page screenshots and `motion-evidence.json` remain in the Bazel test outputs.
Use `--test_env=BOOK_SMARTZ_ACCEPTANCE_COMMIT=<tested-commit>` to identify the
commit in both evidence files. Final acceptance must force fresh test execution.
The native journey disables JavaScript and checks first Add, comparison,
uncertainty, read/reload/sign-in without recovery, explicit Check original save,
Resume, and keyboard completion. Fixture controls remain outside production routes.
A failed remote cover exposes one accessible placeholder and retains its catalog URL.

The browser target uses the [pinned combined worker](../../tools/book_smartz_browser_worker/README.md)
on BuildBuddy Firecracker, with Docker initialization and the matching bundled
Chromium under `/ms-playwright`. The worker's [publication and anonymous full-image verification](https://github.com/LowkeyLab/bazel-repo/actions/runs/39092168629)
passed before consumer enablement. Normal CI retains its existing remote-to-local
fallback policy. The app, HTTP, and package targets retain their remote PostgreSQL fixtures.

For the complete local smoke journey, use Docker and the configured Nix browser:

```bash
nix develop --command aspect test //book_smartz/web:browser_test --strategy=TestRunner=local --test_env=CHROMIUM_EXECUTABLE --nocache_test_results
```

An explicit `--test_env=CHROME_BIN=/path/to/chrome` also works. Browser overrides
aren't inherited automatically: pass them only for local execution so remote
workers never receive a developer's Nix store path. Missing Docker or browser
prerequisites fail the test.

For a fresh focused remote acceptance probe, keep the cache and fallback overrides
scoped to this command:

```bash
nix develop --command aspect test //book_smartz/web:browser_test --config=remote-linux --strategy=TestRunner=remote --nocache_test_results --noremote_accept_cached --noremote_local_fallback --test_output=all
```

The [fresh remote probe](https://app.buildbuddy.io/invocation/30f93d62-4df3-433d-8498-66f72250dbe7)
passed the complete journey. Its test action recorded `runner=remote`,
`cacheHit=false`, Firecracker and Docker initialization with the pinned worker.
The [explicit local Nix smoke journey](https://app.buildbuddy.io/invocation/83a23410-0546-4bfc-8e1e-0728d3de98d7)
also passed. These runs used the implementation delivered for #2373. The ticket
records the final tested commit and execution evidence.

Record the tested commit, immutable worker digest, BuildBuddy invocation and test
execution alongside the result. Invocation upload or remote compilation alone
doesn't prove the browser test executed remotely.

Build `//book_smartz/web:image` for the Open Container Initiative image. Its default command is
`/book-smartz serve`. The pre-deploy command is `/book-smartz migrate`.
`//book_smartz/web:push_image` uses the existing GitHub Container Registry/latest convention.
Image construction and local package acceptance don't establish publication,
Railway configuration, live diagnostic retention or launch readiness.

Readiness rechecks packaged assets and schema without provider calls. Explicit
migration uses a separate pool capped at one connection. The two-minute bound
includes connection establishment. SIGTERM stops admission and drains requests
for twenty seconds, followed by at most five seconds for pool closure.

Diagnostic records contain typed category/outcome, elapsed milliseconds, an executable
release fingerprint, and an optional opaque action ID. Authentication and
catalog records report operation outcomes. Ranking confirmation requires a newly
acknowledged commit. Recovering an already saved action and replaying state emit
no new ranking activity. Failed or uncertain writes remain unconfirmed/checking.
The 256-record queue drops on saturation or destination failure. Delivery has no
retries and doesn't extend process shutdown. The queue may drop buffered records.
The app installs no traces or metrics. The platform collects stderr. Configure its
private access and retention separately. The app turns off dependency SQL statement logging and avoids raw
process-error formatting to protect private payloads.
