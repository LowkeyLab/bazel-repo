# Discord prediction market

This standalone bot gives each Discord server its own play-point economy. Members enroll with `/market join`, receive a grant immediately, and receive later grants on their fixed enrollment schedule. Members can create and bet on markets. Server Administrators and members with Manage Guild permission can settle or cancel them. Points have no cash value.

## Local setup

Create a Discord application and bot in the developer portal. Install it in a server with `bot` and `applications.commands` scopes. The gateway uses the **Guilds** intent; no privileged member, presence, or message-content intent is needed. Keep the bot token private. Slash commands are registered separately in each guild at gateway startup.

Copy `.env.example` to `.env` and replace the placeholder token and database passwords. `.env` is ignored by Git. `DISCORD_APPLICATION_ID` is an optional expected ID. Startup always asks Discord for the authenticated application ID and rejects a mismatch before opening the event store. `GRANT_AMOUNT` and `GRANT_INTERVAL_SECONDS` must be positive integers and default to 100 and 86400. The grant policy is recorded when a guild's first member joins, so changing these defaults affects only newly initialized guilds.

The provided Compose file runs PostgreSQL 16 with a persistent named volume and a local-only port. Its initialization script creates a login role that inherits the migration's restricted `prediction_bot_runtime` role. The script only runs when the volume is first created. The database owner needs `CREATEROLE` for the schema migration. Use separate owner and runtime URLs:

```bash
docker compose --env-file prediction_bot/.env -f prediction_bot/compose.yml up -d postgres
set -a; . prediction_bot/.env; set +a
nix develop --command bazel run //prediction_bot/bin:bin -- --migrate
nix develop --command bazel run //prediction_bot/bin:bin
```

The migration command uses `MIGRATION_DATABASE_URL`. Normal startup uses `DATABASE_URL`; it rejects a role with UPDATE, DELETE, or TRUNCATE rights on the event tables, validates schema version, and replays all committed guild events before connecting to the gateway. For an existing PostgreSQL server, create an owner with schema creation and `CREATEROLE`, run the migration with that owner, then create a runtime LOGIN role and grant it membership in `prediction_bot_runtime`. Never use the owner URL for normal startup.

## Commands

`/market create` takes a question, outcomes separated by `|` (for example `Yes | No`), and an RFC 3339 close time such as `2030-01-02T03:04:05Z`. Questions may be up to 200 characters; provide two to ten distinct outcomes of up to 80 characters each. `/market list` shows the ten newest open markets and their IDs. `/market show id` shows numbered outcomes and pooled points. `/market bet id outcome amount` uses the displayed **one-based** outcome number and a positive integer stake. `/market balance` and `/market leaderboard` show current points. `/market resolve id outcome` requires the market to have closed; `/market cancel id` can be used before or after close while the market is unresolved. Responses are private to the invoking member and do not ping users or roles.

The grant worker checks due schedules once per minute and catches up all missed intervals. Its stream command key and guild transaction lock prevent double grants. The process holds a PostgreSQL gateway advisory lock so only one gateway process runs for an application. Stop with Ctrl-C or SIGTERM for a graceful shutdown.

## Storage, recovery, and checks

PostgreSQL stores immutable CloudEvents, one event per guild revision, plus append-only command receipts for idempotent Discord redelivery. A command that produces several events commits them atomically. Balances and markets are read-only in-memory views rebuilt by replaying events in revision order. Restarting after a crash reconstructs committed changes; a lost Discord response does not reverse a committed bet or settlement. Full replay on startup, each query, and each command favors correctness over throughput for this first version. Preserve database backups and the named volume: losing the event history loses the economy.

Run Bazel checks from the repository root:

```bash
nix develop --command aspect test //prediction_bot/lib:domain_test
nix develop --command aspect test //prediction_bot/lib:discord_test
nix develop --command aspect test //prediction_bot/lib:store_test
nix develop --command aspect format --scope=all
nix develop --command aspect build //...
```

The PostgreSQL integration target needs its isolated Docker test environment. Adapter tests use local interaction inputs; they do not contact Discord or establish that a bot has been deployed successfully.
