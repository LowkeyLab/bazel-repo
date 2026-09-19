# Discord prediction market

This standalone bot gives each Discord server its own play-point economy. Members enroll with `/market join`, receive a grant immediately, and receive later grants on their fixed enrollment schedule. Members can create and bet on markets. Server Administrators and members with Manage Guild permission can settle or cancel them. Points have no cash value.

## Local setup

Create a Discord application and bot in the developer portal. Install it in a server with `bot` and `applications.commands` scopes. The gateway uses the **Guilds** and **Guild Messages** intents; no privileged member, presence, or message-content intent is needed. Keep the bot token private. Slash commands are registered separately in each guild at gateway startup.

Copy `.env.example` to `.env` and replace the placeholder token and database passwords. `.env` is ignored by Git. `DISCORD_APPLICATION_ID` is an optional expected ID. Startup always asks Discord for the authenticated application ID and rejects a mismatch before opening the event store. `GRANT_AMOUNT` and `GRANT_INTERVAL_SECONDS` must be positive integers and default to 100 and 86400. The grant policy is recorded when a guild's first member joins, so changing these defaults affects only newly initialized guilds.

The provided Compose file runs PostgreSQL 18 with a persistent named volume and a local-only port. `--migrate` applies embedded SQLx migrations to create the schema, the restricted `prediction_bot_runtime` role, and the `prediction_bot_app` login with membership in that role. The database owner needs schema creation and `CREATEROLE` privileges (and permission to administer existing bot roles). Use separate owner and runtime URLs:

```bash
docker compose --env-file prediction_bot/.env -f prediction_bot/compose.yml up -d postgres
set -a; . prediction_bot/.env; set +a
nix develop --command bazel run //prediction_bot/bin:bin -- --migrate
nix develop --command bazel run //prediction_bot/bin:bin
```

The migration command uses `MIGRATION_DATABASE_URL` and requires `POSTGRES_RUNTIME_PASSWORD`. The password is passed as a bound connection setting and safely quoted by PostgreSQL when creating the application login. Existing login passwords are preserved; changing this variable after creation does not rotate the password. Match the password in `DATABASE_URL` to the application login (URL-encode special characters in the URL). SQLx records applied migrations in `_sqlx_migrations`, validates their checksums, and only runs pending migrations. These migrations require a fresh database and no pre-existing bot roles; adoption of the previous setup is not supported. `prediction_schema` remains the runtime schema compatibility marker. Normal startup uses `DATABASE_URL`; it rejects a role with UPDATE, DELETE, or TRUNCATE rights on the event tables, validates schema version, and replays all committed guild events before connecting to the gateway. For an existing PostgreSQL server, create an owner with schema creation and `CREATEROLE`, set `POSTGRES_RUNTIME_PASSWORD`, and run `--migrate` with that owner; no manual bot role creation is needed. Never use the owner URL for normal startup.

## Container image

The Deploy workflow publishes `ghcr.io/lowkeylab/prediction_bot:latest` after Bazel Tests succeeds on `main`. Aspect delivery discovers `//prediction_bot/bin:push_image` automatically. The image packages the bot binary and its embedded migrations on the repository's distroless C/C++ base.

Build or load the image locally:

```bash
nix develop --command aspect build //prediction_bot/bin:image
nix develop --command bazel run //prediction_bot/bin:load_image
```

The local image is tagged `local/prediction-bot:latest`. Pass the environment variables described above at runtime, using a database URL reachable from inside the container. Run migrations once with the owner credentials before starting the bot with its restricted runtime credentials:

```bash
docker run --rm --env-file prediction_bot/.env local/prediction-bot:latest --migrate
docker run --rm --env-file prediction_bot/.env local/prediction-bot:latest
```

For the provided PostgreSQL Compose setup on Linux, add `--network=host` to each `docker run` command to reach its local-only database port. For the published image, replace `local/prediction-bot:latest` with `ghcr.io/lowkeylab/prediction_bot:latest`.

To publish manually after authenticating to GHCR with package write access:

```bash
nix develop --command bazel run --compilation_mode=opt //prediction_bot/bin:push_image
```

## Commands

`/market help` privately explains the bot and its main commands, including how to join and start betting. It works before enrollment and does not read the economy database. Mentioning the bot in a server channel prompts you to run `/market help`; the prompt is visible in that channel and does not ping anyone. Bot-authored messages and direct messages are ignored. The bot needs View Channel and Send Messages permissions to deliver channel prompts (Send Messages in Threads for threads).

`/market create` with no arguments opens a private outcome picker: **Yes / No**, **Win / Lose / Draw**, or **Custom outcomes**. Selecting a preset opens a form for the question and closing time. Enter a positive whole-number duration such as `30m`, `1h`, or `2d`, or an RFC 3339 timestamp such as `2030-01-02T03:04:05Z`. Durations start when you submit the form. Custom outcomes are entered one per line. Questions may be up to 200 characters; provide two to ten distinct outcomes of up to 80 characters each.

All timestamps are represented as whole Unix seconds. Fractional seconds in RFC 3339 closing times are discarded: `2030-01-02T03:04:05.900Z` closes at `2030-01-02T03:04:05Z`. Duration submissions are also anchored to the whole second of submission. Durations use fixed elapsed time: `1d` means 86,400 seconds, including across daylight-saving changes.

`/market list` offers a dropdown of the ten newest open markets. Selecting a market, or using `/market show id`, displays its question, closing time, status, and pooled points. Choose an outcome on the card to open a stake form showing your available balance. Submit a positive whole-number stake to place the bet; the private receipt records the outcome, stake, and remaining balance. Enroll with `/market join` before betting. A market that closes while a form is open cannot accept the submitted bet, and repeated delivery of the same submission cannot charge twice. Forms and controls belong to the member and server that opened them. Use `/market list` again to refresh pooled points and available markets.

The direct shortcuts remain available: `/market create question options closes_at` requires all three fields, outcomes separated by `|` (for example `Yes | No`), and an RFC 3339 close time. `/market bet id outcome amount` uses the displayed **one-based** outcome number and a positive integer stake. `/market balance` and `/market leaderboard` show current points. `/market resolve id outcome` requires the market to have closed; `/market cancel id` can be used before or after close while the market is unresolved. Resolution is available to the market creator or members with Administrator or Manage Guild permission. Cancellation requires Administrator or Manage Guild permission. Slash-command responses are private to the invoking member and do not ping users or roles.

### Market announcements

A server Administrator or member with Manage Guild permission can run `/market announcements set channel` to choose one ordinary server text channel. The bot validates that the channel belongs to the server and that its effective permissions include **View Channel** and **Send Messages** before saving it. The root `/market` command remains available to every server member. Configuration responses are private and suppress user and role pings.

When enabled, the bot announces new participant registration, each accepted bet, market creation, resolution, and cancellation. Registration announcements identify the member without pinging them. Bet announcements show the market question, market ID, and total number of accepted bets (including repeat bets by the same person), without identifying the bettor, outcome, or stake. Event times and bet counts are captured when the action succeeds and remain unchanged during delayed delivery or retries. Repeated joins and redelivered commands do not enqueue extra announcements. Events that happened before announcements were enabled are not backfilled. Use `/market announcements status` to see the configured channel, whether delivery is enabled or paused, the pending count, and any safe pause reason.

Failed deliveries retry after five seconds with an increasing delay capped at five minutes; provider-requested longer delays are respected, and there is no attempt limit. Delivery is at least once: an uncertain Discord response can cause a duplicate announcement. Changing the destination moves pending work to the new channel, but an announcement already in flight can still reach the old channel. `/market announcements disable` stops future enqueueing and permanently discards pending announcements; reenabling later does not restore discarded work, though an in-flight request may still complete.

Authentication failures (HTTP 401) and request timeouts (HTTP 408) remain queued and retry with backoff. An operator must correct invalid credentials and restart the bot; affected servers then resume at their saved retry deadlines without an administrator reconfiguring the announcement channel.

The announcement worker polls immediately at startup and waits one second between completed batches. It preserves revision order within each server while allowing up to eight servers to progress concurrently. Shutdown stops discovering new announcements and gives accepted requests the remainder of a shared 15-second gateway shutdown budget to record their acknowledgement; unfinished sends are then aborted and their durable pending work remains for restart. An unexpected announcement-worker exit stops the gateway so it cannot continue with delivery silently disabled.

The grant worker checks due schedules once per minute and catches up all missed intervals. Its stream command key and guild transaction lock prevent double grants. The process holds a PostgreSQL gateway advisory lock so only one gateway process runs for an application. Stop with Ctrl-C or SIGTERM for a graceful shutdown.

## Operational audit records

Production logging writes newline-delimited JSON at information level and above. Typed audit events cover command and query completion, interaction acknowledgement and response delivery, mention reply completion, grant discovery or reconstruction failures, command registration, and migration, startup, readiness, and shutdown lifecycle changes. Each record has a stable event name, outcome, operation stage, and the fields appropriate to that event family.

Mention replies emit one `mention_reply_completed` record per attempted send, correlated by guild, channel, and triggering message ID. Successful delivery is logged at INFO and delivery failure at WARN with sanitized failure categories and provider status codes. Success means Discord accepted the request, not that the user read the prompt. Ignored messages emit no event.

Announcement attempts emit `announcement_attempt_completed`, correlated by guild, event revision, attempted channel, and configuration version. Records include the selected delivered/retry/pause decision and sanitized failure category; a concurrent configuration change can supersede that decision. Delivery failures are WARN, database completion failures are ERROR, and discovery failures emit `announcement_worker_failed` at ERROR. Empty polls emit no record. Use `/market announcements status` and reconfigure a paused destination after correcting its channel or permissions.

Guild IDs and existing command or interaction identifiers correlate related records. Discord command keys use `discord:<interaction-id>`; scheduled grants use `grant:<member-id>:<schedule-boundary>`, so the selected grant boundary remains visible during retries. Interaction IDs are identifiers, not interaction tokens. A successful command record describes a successful invocation. It does not prove that invocation created a new economic effect: Discord redelivery can recover an existing command receipt, and a grant retry can find an already committed receipt without issuing points twice.

Audit delivery is synchronous, inline, and best effort. Logging can add latency, and records can be lost if the process or output destination fails. The repository does not configure production log collection, retention, or alert delivery, so an emitted record alone does not establish that an operator will retain or receive it. Immutable economic events and command receipts remain the recovery record.

Audit records contain allowlisted categories and status codes, not raw errors, SQL parameters, URLs, credentials, interaction tokens, market questions, outcome labels, or response content. Correlation identifiers are intended for diagnosis and should not become metric labels. Future listeners may derive metrics or traces and manage their own buffering and exporter lifecycle without changing event producers; no metrics backend, trace exporter, or durable operational audit store is configured here.

## Storage, recovery, and checks

PostgreSQL stores immutable CloudEvents, one event per guild revision, plus append-only command receipts for idempotent Discord redelivery. A command that produces several events commits them atomically. Balances and markets are read-only in-memory views rebuilt by replaying events in revision order. Restarting after a crash reconstructs committed changes; a lost Discord response does not reverse a committed bet or settlement. Full replay on startup, each query, and each command favors correctness over throughput for this first version. Preserve database backups and the named volume: losing the event history loses the economy.

Run Bazel checks from the repository root:

```bash
nix develop --command aspect test //prediction_bot/lib:domain_test
nix develop --command aspect test //prediction_bot/lib:discord_test
nix develop --command aspect test //prediction_bot/lib:store_test
nix develop --command aspect test //prediction_bot/lib:announcements_test
nix develop --command aspect format --scope=all
nix develop --command aspect build //...
```

The PostgreSQL integration target needs its isolated Docker test environment. Announcement composition tests use the migrated runtime database and a real Serenity HTTP client against Wiremock, including the guarded application startup path. These tests disable Serenity’s rate limiter and terminate the gateway through an HTTP error; they do not establish live Discord readiness, gateway reconnect behavior, or production rate-limit behavior. Adapter tests use local interaction inputs; they do not contact Discord or establish that a bot has been deployed successfully.

## Code coverage

Run coverage from the repository root (the PostgreSQL integration tests require Docker):

```bash
nix develop --command bazel run //tools/coverage -- //prediction_bot/...
```

The wrapper writes `coverage-report.lcov` and, when `genhtml` is available, an HTML report at `coverage-report/index.html`. The Nix development shell provides `genhtml`. To collect coverage without the Docker integration tests:

```bash
nix develop --command bazel run //tools/coverage -- --test_tag_filters=-requires-docker //prediction_bot/...
```

This narrower run omits integration-test coverage. The repository Coverage workflow includes `prediction_bot` in its combined report and uploads it to Codecov on pull requests and pushes to `main`.
