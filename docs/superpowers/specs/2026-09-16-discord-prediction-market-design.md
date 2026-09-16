# Standalone Rust Discord prediction market

Status: the user approved a standalone Rust bot and the proposed economy. This document makes the implementation details explicit for the written-spec review required by Superpowers.

## Purpose and scope

Build a Discord bot that runs a separate play-point prediction economy in each server. Members enroll, receive points automatically at fixed intervals, create markets, and stake points against one another. Moderators settle or cancel markets. PostgreSQL stores all durable state; the bot runs independently of Predix and Nicknamer.

The first version includes enrollment, balances, rankings, market creation and inspection, betting, settlement, cancellation, periodic grants, migrations, configuration, and deployment instructions. Points have no cash value. Trading positions, order books, real-money payments, web UI, and automatic external outcome verification are outside this version.

## Architecture and repository placement

Use Rust edition 2024 and Tokio. Place the application in `prediction_bot/`, with a library in `prediction_bot/lib/`, an executable in `prediction_bot/bin/`, SQL migrations in `prediction_bot/migrations/`, and behavior tests in `prediction_bot/lib/tests/`.

The library separates these responsibilities:

- Domain rules: stake validation, market lifecycle, integer payout allocation, and grant eligibility. These rules take explicit inputs, including time, and do not depend on Discord or database connections.
- Application service and PostgreSQL storage: transactional commands and queries, authorization inputs, idempotency, migrations, and durable balances.
- Discord adapter: guild slash commands, Discord identity and permission extraction, deferred responses, and concise user-facing messages.
- Grant worker: finds due accounts and applies grants through the same transaction boundary as user commands.

Use the existing SQLx, Tokio, serde, thiserror, anyhow, and tracing dependencies. Use Serenity for the Discord gateway and interaction transport, with the minimum required features; verify its current API and resolve its dependencies through Bazel before implementation. Do not add a second async runtime. Use thiserror for library errors and anyhow at the executable boundary.

The executable starts migrations, registers guild-only commands, starts the grant worker and Discord client, and shuts down on termination. A PostgreSQL advisory lock prevents multiple active gateway processes for the same application; database correctness must still tolerate concurrent commands and worker execution.

## Discord interface and permissions

Expose a `/market` command group:

| Command                             | Behavior                                                                                                           |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `join`                              | Enroll the invoking member in this server's economy; repeated enrollment preserves the existing account.           |
| `balance`                           | Show the invoking member's available points and next grant time.                                                   |
| `leaderboard`                       | Show the top ten enrolled accounts by available balance, ties ordered by Discord user ID.                          |
| `create question options closes_at` | Create a market with two to ten distinct outcomes and a future UTC close time. Options use a documented delimiter. |
| `list`                              | Show the ten newest open markets in this server; provide market IDs for inspection.                                |
| `show id`                           | Show question, numbered outcomes, closing time, lifecycle state, and points pooled on each outcome.                |
| `bet id outcome amount`             | Add a positive integer stake to an outcome, deducting the same amount from the available balance.                  |
| `resolve id outcome`                | Settle a closed market and distribute the pool.                                                                    |
| `cancel id`                         | Cancel an unresolved market and refund its stakes.                                                                 |

Commands require a guild interaction; direct messages are rejected. Guild and user IDs come from Discord interaction metadata, never user-supplied options. Enrollment is required for creation and betting. A user may place multiple bets, including on different outcomes; accepted stakes are final until settlement or cancellation.

Resolution and cancellation require Discord Administrator or Manage Guild permission. Market creators receive no extra settlement permission. Resolution is allowed only at or after the stated closing time, and cancellation is allowed before or after close. Every database lookup includes the guild ID so a market ID from another server cannot expose or mutate its data.

Reject bot enrollment, malformed outcome lists, empty questions, questions longer than 200 characters, outcome labels longer than 80 characters, unknown outcomes, nonpositive stakes, insufficient balances, and betting at or after close. Bound command output to Discord limits and disable allowed mentions in generated content. Defer database-backed interaction responses before processing and return errors without database internals or credentials.

## Economy and grant timing

Amounts are nonnegative signed 64-bit integers in storage, with checked arithmetic and wider integer intermediates for payout multiplication. Reject operations that would exceed representable balances; never wrap or silently clamp.

Defaults are 100 points per 24-hour interval, configurable through deployment environment variables. These settings are persisted when a guild economy is first initialized; changing environment defaults affects newly initialized guilds only. Live economy reconfiguration is outside this version.

First enrollment grants 100 points immediately and sets the next grant to enrollment time plus 24 hours, using that guild's configured values. Subsequent grants follow that account's fixed enrollment schedule. Repeated enrollment does not reset the schedule or issue another initial grant. Enrollment and grants are scoped to the `(guild_id, user_id)` account.

A worker checks due accounts once per minute. It issues all completed intervals, including intervals elapsed while the application was offline, and advances the due timestamp by the number of granted intervals. It does not reset the schedule to the worker execution time. Enrollment remains active without Discord presence tracking; departed or inactive members retain their accounts and scheduled grants. Membership synchronization is outside this version.

Account row locking makes grant calculation, balance updates, and schedule advancement atomic. Multiple worker attempts cannot award the same interval twice. A failing account transaction is logged and does not stop processing other accounts.

## Market lifecycle and payouts

A market begins open. The stored closing timestamp is authoritative: betting is forbidden at or after that timestamp even if a worker has not run. Queries display a pending-outcome state after close. Resolved and cancelled markets are terminal.

Each accepted bet immediately transfers its points from the player's available balance into the market pool. No house rake applies. Settlement distributes the entire pool among winning bets proportionally to their stakes, aggregating stakes by winning player before allocation.

Use integer division for base payouts. Distribute leftover points by descending fractional remainder, with ascending Discord user ID as the deterministic tie-breaker. For example, two winners staking 1 and 2 points against a losing stake of 2 split a five-point pool into payouts of 2 and 3. A winner's payout includes the returned stake.

Cancellation refunds each player's total stake. If no bets selected the winning outcome, resolving the market refunds all stakes and records that outcome with a no-winner refund reason. A market with no bets can still be resolved without balance changes. No operation can settle or refund a terminal market again.

Outside scheduled and initial grants, the sum of available balances and unsettled stakes is conserved. Tests must demonstrate this property with concrete cases, including rounding, refunds, and concurrent requests.

## Persistence and transaction boundaries

Use dedicated PostgreSQL tables for guild economy settings, accounts, markets, outcomes, bets, and completed interaction receipts. Discord snowflakes are stored as decimal strings rather than narrowed to signed integers. Markets use UUIDs and store their guild, creator, question, close time, state, winning outcome, and settlement reason. Outcomes have stable per-market integer IDs. Store grant schedules and lifecycle timestamps in UTC.

Use foreign keys, unique keys, and check constraints to enforce account identity, valid outcome references, positive stake amounts, and nonnegative balances. Scope referenced accounts and market outcomes to the same guild.

Each mutating Discord command runs in one database transaction, including its interaction receipt. A unique interaction ID prevents re-delivery from creating a second bet, market, enrollment grant, or settlement. Store the operation's response summary with the receipt so a duplicate can return the original result. Different interaction IDs remain different commands.

Lock the market before validating or mutating bets and settlement, then lock affected account rows in a stable user-ID order. The grant worker locks accounts without taking market locks. This order prevents settlement and a late bet from both succeeding and avoids opposite lock order between market operations. Re-check closing time after locks are acquired. Balance deduction, bet insertion, payouts, terminal state, and the receipt either commit together or roll back together.

A lost Discord response after database commit does not undo or repeat the economic operation. Retries of the same interaction return the stored response. Log Discord delivery failures separately from transaction failures.

## Configuration and operation

Require `DISCORD_TOKEN` and `DATABASE_URL` through environment variables. Optional positive integer settings `GRANT_AMOUNT` and `GRANT_INTERVAL_SECONDS` default to 100 and 86400. Fail startup on invalid settings or failed migrations. Never log the token, database URL, or interaction tokens.

Document creating and installing a Discord application with slash-command access, enabling the necessary minimal gateway intents, supplying a persistent PostgreSQL database, running migrations, and starting the binary through Bazel. Include a local PostgreSQL compose example and an ignored example environment file containing placeholders only. Structured tracing reports startup, transaction failures, grant processing, and shutdown.

## Test design: Testing Principles

Test public operations and observable balances, payouts, market status, and command responses. Do not assert internal repository call sequences. Avoid sleeping to test time: provide explicit timestamps to domain rules and controllable time inputs at the service boundary.

Domain tests cover exact grant boundaries and missed intervals, integer payouts and remainder ties, no-winner refunds, invalid stakes, and arithmetic overflow. Their feedback speed is unverified until measured.

Integration tests use a real isolated PostgreSQL instance with migrations, following the repository's testcontainers pattern. Treat this dedicated bot database as an application-managed dependency. Tests cover persistence across service recreation, guild isolation, duplicate interactions, grant retries, simultaneous overspending attempts, betting versus resolution, concurrent settlement, transaction rollback, and cancellation conservation. Assert stored results through service queries; do not substitute an in-memory repository as evidence of transaction correctness.

Discord is an external unmanaged boundary. Adapter tests use representative interaction inputs and capture outgoing user-visible responses, including permission rejection, DM rejection, malformed options, and mention suppression. Live Discord smoke testing is separate and requires supplied credentials; local tests must not claim to establish successful Discord deployment.

For each implementation increment, run a meaningful failing behavior test before implementing the behavior, then run the relevant passing tests. Record commands and results, distinguish environmental failures from assertion failures, and report actual timing when available. Review tests for regression protection, resistance to refactoring, feedback speed, and maintainability.

## Bazel integration and completion evidence

Use the root Cargo manifest and lockfile as Bazel dependency inputs, following existing rules_rs targets. Do not use cargo directly. Run `bazel run //:gazelle` immediately after each source edit and before any manual BUILD edits or formatting.

Run focused `aspect test` targets for the domain, PostgreSQL, and adapter tests. Before completing implementation, run `aspect format --scope=all` and `aspect build //...`. Preserve unrelated work and report any unavailable tooling or infrastructure explicitly. Use conventional commits.
