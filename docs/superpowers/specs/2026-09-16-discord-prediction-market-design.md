# Standalone Rust Discord prediction market

Status: the user approved a standalone Rust bot and the proposed economy, then requested event-sourced storage with read-only projections. This specification incorporates that storage requirement for written-spec review.

## Purpose and scope

Build a Discord bot that runs a separate play-point prediction economy in each server. Members enroll, receive points automatically at fixed intervals, create markets, and stake points against one another. Moderators settle or cancel markets. An append-only PostgreSQL event store holds the durable source of truth; read-only projections derive all economy state from those events; the bot runs independently of Predix and Nicknamer.

The first version includes enrollment, balances, rankings, market creation and inspection, betting, settlement, cancellation, periodic grants, migrations, configuration, and deployment instructions. Points have no cash value. Trading positions, order books, real-money payments, web UI, and automatic external outcome verification are outside this version.

## Architecture and repository placement

Use Rust edition 2024 and Tokio. Place the application in `prediction_bot/`, with a library in `prediction_bot/lib/`, an executable in `prediction_bot/bin/`, SQL migrations in `prediction_bot/migrations/`, and behavior tests in `prediction_bot/lib/tests/`.

The library separates these responsibilities:

- Domain decisions: validate commands against replayed guild state and produce domain events. Stake validation, lifecycle transitions, payout allocation, and grant eligibility take explicit inputs, including time, and do not depend on Discord or database connections.
- Event application: a deterministic reducer folds historical events into guild state. Applying an event never issues commands, sends notifications, consults the clock, or recalculates a historical payout.
- Application service and PostgreSQL event store: serialize guild commands, load history, append event batches atomically, enforce idempotency, and manage event schema versions.
- Read-only projections: publish immutable, revisioned views of accounts, balances, markets, bets, grant schedules, and rankings. Queries consume these views; neither command handlers nor queries directly edit projected state.
- Discord adapter: guild slash commands, Discord identity and permission extraction, deferred responses, and concise user-facing messages.
- Grant worker: finds due accounts and applies grants through the same transaction boundary as user commands.

Use the existing SQLx, Tokio, serde, thiserror, anyhow, and tracing dependencies. Use Serenity for the Discord gateway and interaction transport, with the minimum required features; verify its current API and resolve its dependencies through Bazel before implementation. Do not add a second async runtime. Use thiserror for library errors and anyhow at the executable boundary.

A migration command initializes the event-store schema using a separate owner role. Normal startup validates the schema, rebuilds projections from committed events, registers guild-only commands, starts the grant worker and Discord client, and shuts down on termination. A PostgreSQL advisory lock prevents multiple active gateway processes for the same application; database correctness must still tolerate concurrent commands and worker execution.

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

Resolution and cancellation require Discord Administrator or Manage Guild permission. Market creators receive no extra settlement permission. Resolution is allowed only at or after the stated closing time, and cancellation is allowed before or after close. Every event-stream and projection lookup includes the guild ID so a market ID from another server cannot expose or mutate its data.

Reject bot enrollment, malformed outcome lists, empty questions, questions longer than 200 characters, outcome labels longer than 80 characters, unknown outcomes, nonpositive stakes, insufficient balances, and betting at or after close. Bound command output to Discord limits and disable allowed mentions in generated content. Defer database-backed interaction responses before processing and return errors without database internals or credentials.

## Economy and grant timing

Amounts are nonnegative signed 64-bit integers in storage, with checked arithmetic and wider integer intermediates for payout multiplication. Reject operations that would exceed representable balances; never wrap or silently clamp.

Defaults are 100 points per 24-hour interval, configurable through deployment environment variables. These settings are persisted when a guild economy is first initialized; changing environment defaults affects newly initialized guilds only. Live economy reconfiguration is outside this version.

First enrollment grants 100 points immediately and sets the next grant to enrollment time plus 24 hours, using that guild's configured values. Subsequent grants follow that account's fixed enrollment schedule. Repeated enrollment does not reset the schedule or issue another initial grant. Enrollment and grants are scoped to the `(guild_id, user_id)` account.

A worker checks due accounts once per minute. It issues all completed intervals, including intervals elapsed while the application was offline, and advances the due timestamp by the number of granted intervals. It does not reset the schedule to the worker execution time. Enrollment remains active without Discord presence tracking; departed or inactive members retain their accounts and scheduled grants. Membership synchronization is outside this version.

Grant commands rehydrate the guild stream under its transaction lock and append a PointsGranted event recording the amount, covered interval range, and resulting next-due timestamp. Replaying that event derives the balance and schedule together. Multiple worker attempts cannot award the same interval twice. A failing grant command is logged and does not stop processing other accounts.

## Market lifecycle and payouts

A market begins open. The stored closing timestamp is authoritative: betting is forbidden at or after that timestamp even if a worker has not run. Queries display a pending-outcome state after close. Resolved and cancelled markets are terminal.

Each accepted bet immediately transfers its points from the player's available balance into the market pool. No house rake applies. Settlement distributes the entire pool among winning bets proportionally to their stakes, aggregating stakes by winning player before allocation.

Use integer division for base payouts. Distribute leftover points by descending fractional remainder, with ascending Discord user ID as the deterministic tie-breaker. For example, two winners staking 1 and 2 points against a losing stake of 2 split a five-point pool into payouts of 2 and 3. A winner's payout includes the returned stake.

Cancellation refunds each player's total stake. If no bets selected the winning outcome, resolving the market refunds all stakes and records that outcome with a no-winner refund reason. A market with no bets can still be resolved without balance changes. No operation can settle or refund a terminal market again.

Outside scheduled and initial grants, the sum of available balances and unsettled stakes is conserved. Tests must demonstrate this property with concrete cases, including rounding, refunds, and concurrent requests.

## Event-sourced persistence and transaction boundaries

### Source of truth and consistency boundary

Use one event stream per Discord guild. Enrollment, grants, all markets, stakes, and settlement in that guild share this consistency boundary. A bet affects both an account and a market, so keeping them in one stream avoids distributed transactions between separate aggregates. Commands for different guilds can execute concurrently; commands within a guild serialize. This intentionally favors correctness and simplicity for a local server economy over high write throughput within a single server.

PostgreSQL stores only append-only command envelopes containing ordered domain-event batches and immutable command results. There are no authoritative mutable balance, account, market, or bet tables. Each envelope records guild ID, contiguous guild revision, command idempotency key, actor identity or system origin, acceptance timestamp, response summary, and an ordered JSON event array. Each event has a type and schema version. Use unique constraints on `(guild_id, revision)` and `(guild_id, command_key)`, and validate envelopes before append.

The runtime database role can SELECT and INSERT event envelopes, but cannot UPDATE, DELETE, or TRUNCATE them. A separate migration role owns schema changes. Never edit an old event to correct a business outcome; any future correction feature must append a new explicit compensating event. That feature is outside the first version.

Store Discord snowflakes as decimal strings, market IDs as UUIDs, per-market outcome IDs as stable integers, and timestamps in UTC. Compare snowflakes numerically for deterministic ties. Event payloads include all facts required to reconstruct state without Discord, current environment defaults, or external data.

### Domain events and replay

Use the following initial event vocabulary:

| Event                     | Recorded facts and projected effect                                                                                                                                                                                      |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `GuildEconomyInitialized` | Grant amount and interval; establishes this guild's economy rules.                                                                                                                                                       |
| `MemberEnrolled`          | User ID and enrollment timestamp; creates an account with zero balance and an initial grant due at enrollment.                                                                                                           |
| `PointsGranted`           | User ID, initial or periodic reason, amount, covered schedule boundaries, and next-due timestamp; credits points and advances the grant schedule. Initial enrollment and its initial grant share one atomic event batch. |
| `MarketCreated`           | Market ID, creator, question, numbered outcomes, creation time, and closing time; establishes an open market.                                                                                                            |
| `BetPlaced`               | Bet ID, market ID, user ID, outcome ID, amount, and acceptance time; deducts available points and adds the stake to the pool.                                                                                            |
| `MarketResolved`          | Market ID, winning outcome, resolver, settlement time, payout entries, and normal or no-winner-refund reason; credits the recorded allocations and marks the market terminal.                                            |
| `MarketCancelled`         | Market ID, moderator, cancellation time, and refund entries; credits recorded refunds and marks the market terminal.                                                                                                     |

Payout and refund entries record exact integer amounts per user. Compute them once when deciding the command, using checked arithmetic, then replay the recorded allocation without rerunning today's payout algorithm. A batch either appears in full or does not appear at all; consumers never observe enrollment without its initial grant or a terminal market without its associated payouts.

Separate `decide(state, command, accepted_at)` from `apply(state, event)`. Decision functions enforce current business rules and permissions; replay applies historical facts without rechecking present-day permissions or closing times. Unknown event versions, malformed payloads, revision gaps, or impossible event transitions fail reconstruction with an explicit diagnostic; never skip history or serve an incomplete view as current. Future payload changes require versioned deserialization or explicit upcasters and replay fixtures for old versions.

### Command processing and concurrency

Process each command in one PostgreSQL READ COMMITTED transaction:

1. Acquire a transaction-scoped advisory lock keyed deterministically by guild ID. Hash collisions may reduce concurrency but must never mix streams. Every event append, including worker grants, uses this path.
2. Look up the command key. For an existing envelope, return its recorded result without appending more events. Discord interaction IDs are command keys; grant keys identify the account and the scheduled grant boundary observed by the worker.
3. Read and replay the committed stream after obtaining the lock. Never validate a command using a potentially stale query projection. Capture acceptance time after the lock is acquired, then check deadlines and grant eligibility against that time.
4. Decide the command and append its complete event batch, result, and next revision. Unique revision constraints also reject append races. An accepted no-op, such as repeated enrollment, may append a receipt envelope with an empty event array so its retry result remains stable. Rejected commands append no economic events.
5. Commit, then publish the resulting immutable projection at that committed revision. A failed commit publishes nothing. Do not hold a database transaction open while calling Discord.

A retry rehydrates and reevaluates state after any conflict; it cannot blindly append previously computed events. Limit transient retries and return a retryable failure if exhausted. Grants cover only intervals still due in replayed state, even if a second worker selected an outdated schedule boundary. This prevents duplicate grants, overspending, bets after settlement, and repeated payouts without mutating live account rows.

The serialized transaction determines the accepted ordering of a bet and settlement. The time check after lock acquisition rejects a bet whose processing begins after close. The grant worker uses the same guild stream lock, so a grant and a bet cannot overwrite each other's effects.

A lost Discord response after commit does not undo or repeat the economic operation. Redelivery of the same interaction returns its stored result. The bot does not resend historical Discord messages during replay. Delivery failures are logged separately from event-store failures.

### Read-only projections and recovery

For the first version, keep projections in memory as immutable per-guild snapshots carrying the last applied revision. Only the event reducer constructs replacement snapshots; readers receive read-only access. PostgreSQL remains the durable event store, rather than a mutable copy of the economy. No additional broker, projection database, or snapshot persistence is needed initially.

Build projections by folding committed batches in ascending guild revision and events in their stored batch order. Startup discovers guild streams from the event store and fully reconstructs them before accepting commands. Closing status is a query-time derivation of recorded close time and the supplied current time; it does not mutate the projection.

Query operations check the guild's committed head revision and catch the local projection up to at least that revision before answering. Apply missing batches under a per-guild projection synchronization boundary, publish a complete snapshot atomically, and never replace a higher revision with a lower one. A command publishes or catches up to its committed revision before reporting success. Concurrent later commands may naturally advance the stream again; a response identifies a consistent committed revision rather than promising to include future writes.

If a process crashes after commit but before projection publication, restart or query catch-up reconstructs the missing effects from history. A projection failure after a committed command must be reported as a read-model availability problem, not a rolled-back command; its retry still finds the durable receipt. If catch-up fails, return an availability error rather than silently serving stale balances. Grant discovery may read projected due schedules, but each grant command revalidates against replayed events.

Full replay for each command is an explicit first-version performance tradeoff. Introduce verified snapshots only if measured history size makes it necessary; a snapshot would remain disposable acceleration, never a second source of truth.

## Configuration and operation

Require `DISCORD_TOKEN` and `DATABASE_URL` through environment variables. Optional positive integer settings `GRANT_AMOUNT` and `GRANT_INTERVAL_SECONDS` default to 100 and 86400. Use a separate `MIGRATION_DATABASE_URL` for the migration command; normal startup uses the restricted runtime `DATABASE_URL`, validates schema compatibility, and reconstructs projections. Fail startup on invalid settings, incompatible schema, or failed event replay. Never log the token, database URL, or interaction tokens.

Document creating and installing a Discord application with slash-command access, enabling the necessary minimal gateway intents, supplying a persistent PostgreSQL database, running migrations, and starting the binary through Bazel. Include a local PostgreSQL compose example and an ignored example environment file containing placeholders only. Structured tracing reports startup, transaction failures, grant processing, and shutdown.

## Test design: Testing Principles

Test public operations and observable balances, payouts, market status, and command responses. Do not assert internal repository call sequences. Avoid sleeping to test time: provide explicit timestamps to domain rules and controllable time inputs at the service boundary.

Domain tests cover exact grant boundaries and missed intervals, integer payouts and remainder ties, no-winner refunds, invalid stakes, and arithmetic overflow. Exercise command decisions followed by event application, asserting resulting public views rather than replaceable internal call sequences. Replay recorded histories into a fresh state and verify identical balances, schedules, markets, and rankings. Include versioned historical fixtures and verify that replay uses recorded payout allocations and grant settings rather than current algorithms, time, or defaults. Their feedback speed is unverified until measured.

Integration tests use a real isolated PostgreSQL instance with migrations, following the repository's testcontainers pattern. Treat this dedicated bot database as an application-managed dependency. Tests cover persistence across service recreation, guild isolation, duplicate interactions, grant retries, simultaneous overspending attempts, betting versus resolution, concurrent settlement, transaction rollback, and cancellation conservation. Also verify atomic multi-event append, append-only runtime permissions, contiguous stream revisions under concurrent commands, fresh projection reconstruction from stored history, recovery after commit without projection publication, duplicate delivery during catch-up, rejection of corrupt or unsupported history, and isolation between simultaneous guild streams. Assert resulting public projections and command results; do not substitute an in-memory event store as evidence of PostgreSQL transaction correctness. In-memory projections are the intended production read model, not a database test substitute.

Discord is an external unmanaged boundary. Adapter tests use representative interaction inputs and capture outgoing user-visible responses, including permission rejection, DM rejection, malformed options, and mention suppression. Live Discord smoke testing is separate and requires supplied credentials; local tests must not claim to establish successful Discord deployment.

For each implementation increment, run a meaningful failing behavior test before implementing the behavior, then run the relevant passing tests. Record commands and results, distinguish environmental failures from assertion failures, and report actual timing when available. Review tests for regression protection, resistance to refactoring, feedback speed, and maintainability.

## Bazel integration and completion evidence

Use the root Cargo manifest and lockfile as Bazel dependency inputs, following existing rules_rs targets. Do not use cargo directly. Run `bazel run //:gazelle` immediately after each source edit and before any manual BUILD edits or formatting.

Run focused `aspect test` targets for the domain, PostgreSQL, and adapter tests. Before completing implementation, run `aspect format --scope=all` and `aspect build //...`. Preserve unrelated work and report any unavailable tooling or infrastructure explicitly. Use conventional commits.
