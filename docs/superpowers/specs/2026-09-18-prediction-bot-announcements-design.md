# Prediction bot announcement channel

Date: 2026-09-18
Status: Design approved in conversation; written spec awaiting review.

## Purpose and scope

Let each Discord server choose an existing channel where the prediction bot announces market creation, resolution, and cancellation. Delivery survives temporary outages and bot restarts. Existing command replies remain private.

An administrator selects the channel; the bot does not create channels. There are no per-bet announcements, historical backfill, role subscriptions, automatic closing announcements, or external message queues in this version.

## User behavior

Members with Administrator or Manage Guild permission can use:

- `/market announcements set channel:#channel`: validate and select a destination, enable announcements, and clear a delivery pause.
- `/market announcements status`: show the destination, enabled or paused state, pending count, and a safe explanation of any delivery pause.
- `/market announcements disable`: stop enqueueing and discard unsent announcements.

All configuration commands are server-only and reply privately without pings. Validation requires a supported ordinary server text channel in the same server and the bot's ability to view and send there. Threads and special channel types are outside initial scope. A failed validation preserves the previous configuration. Authorization is enforced at the command execution boundary, not just through Discord command visibility.

Enabling announcements applies only to future committed market events. While enabled but paused, new announcements continue to queue. Changing the channel moves unsent announcements to the new destination; delivered announcements are never reposted. Disabling marks pending work discarded; reenabling does not revive it.

A request already in flight may still reach its original channel after a channel change or disable. The bot cannot atomically recall a Discord request. This exception is documented in help and configuration responses.

## Messages

Each announcement is a separate message with the event type, market question, market ID, and original event time. Members can use `/market show` with that ID.

- Creation: creator, outcomes, and closing time.
- Resolution: winning outcome and whether settlement refunded stakes because there were no winning bets.
- Cancellation: cancellation and confirmation that stakes were refunded.

Store a versioned message snapshot when enqueueing. Delayed messages describe the original event, not the current market state. Render deterministically from that snapshot. Treat user-provided content as text, suppress all allowed mentions, and stay within Discord message limits. Public betting controls are outside scope because the existing interactive controls belong to their invoking member.

## Architecture and alternatives

Use a transactional outbox in the existing PostgreSQL database. Insert announcement work in the same transaction as its market event and command receipt. Sending happens asynchronously after commit; Discord failure never changes an already committed market result. An outbox insert failure rolls back the whole transaction.

A saved cursor over the existing event history would also be durable, but complicates destination changes, enablement boundaries, and per-announcement delivery state. An external queue adds infrastructure without removing the need to coordinate persistence. Neither is needed here.

Keep responsibilities focused:

- Existing Discord handlers parse and authorize configuration commands and validate destinations through the real HTTP client.
- Announcement persistence owns configuration and outbox queries and participates in the existing store transaction for enqueueing.
- A pure renderer produces messages from snapshots.
- A delivery worker owns ordered attempts, retries, and delivery outcomes.
- Gateway composition starts and shuts down the worker alongside the existing grant worker.

Keep these concerns in focused announcement modules rather than expanding every responsibility inside `discord.rs`. No generic repository abstraction or new transport trait is required.

## Persistence and transaction boundaries

Add a migration with two tables:

1. Server settings: guild ID, optional channel ID, enabled state, pause reason, and monotonically increasing configuration version.
2. Announcement outbox: guild ID, source event revision, snapshot version and payload, pending/delivered/discarded state, attempt count, next attempt time, safe last failure category, and delivered channel/message IDs when available.

The outbox has a unique guild/event-revision key and references the source event. Index pending work for due discovery and per-server ordering. Completed rows remain as delivery records in this version; automatic retention is outside scope.

Enqueue only the three supported event types when settings are enabled. Paused delivery is still enabled. Historical replay only reconstructs state and never creates outbox entries. Existing command receipt deduplication and outbox uniqueness prevent repeated interactions from creating duplicate work.

Configuration writes and enqueueing acquire the existing guild transaction lock, including when settings do not yet exist. This gives enable, disable, and market commits a defined order. Configuration changes preserve command idempotency: replaying an earlier configuration interaction returns its receipt without restoring an old destination or undoing a newer disable. Configuration commands do not introduce economy events.

Startup must verify that SQLx migration 003 for announcements completed successfully before accepting commands; retain the existing economy schema marker. Missing migration 003 produces the existing actionable migration-required startup failure. Keep existing market history and event schemas compatible. Runtime access remains append-only for event and command tables; grant only the needed SELECT, INSERT, and UPDATE permissions on the new tables. Migration tests exercise the restricted application login, not only the owner account.

## Delivery and concurrency

The existing gateway guard permits one gateway for the application. Within that process, allow at most one active delivery per guild and use bounded concurrency across guilds. No database transaction or guild lock is held during Discord I/O.

For each guild, only its earliest pending event is eligible. A retry delay blocks later announcements for that guild but does not block other guilds. Capture the destination and configuration version, then recheck eligibility and settings immediately before sending. A configuration change after that check falls within the in-flight exception.

On successful delivery, record the actual channel and Discord message ID. If settings changed during a successful attempt, treat the event as delivered and do not resend it to the new channel. If disable already discarded the event, preserve that terminal state. Late failures from an old configuration must not pause the new destination or apply its old retry delay; pending work can use the new configuration. Updates are conditional on the row still being pending and, for failure policy, on the expected configuration version.

Pending state remains durable during a send. A process interruption does not strand a permanently claimed row. On restart, the worker resumes pending work after the gateway guard is acquired. Shutdown stops new attempts and gives active work a bounded opportunity to finish; interrupted work remains recoverable.

Exactly-once delivery is not promised. If Discord accepts a message and the process fails before recording success, retry can create a duplicate. Database command deduplication does not eliminate this external acknowledgement gap.

## Retry behavior

Temporary network failures and server errors retain pending work and use capped exponential backoff. Initial policy: 5 seconds after the first failure, doubling to a maximum of 5 minutes, without a maximum attempt count. Respect any applicable server retry delay when longer. Production retains Serenity's normal rate limiting.

Persist retry deadlines as absolute UTC instants using the repository's whole-second convention. Acquire the clock after the failed request completes when computing its next deadline. A clock rollback can defer delivery until the deadline becomes due; it cannot discard pending work. Polling uses elapsed timers and checks persisted deadlines; restarting does not reset a deadline.

Missing permissions or a deleted destination pause the guild and retain pending work. A permanent payload rejection also pauses delivery with an actionable safe reason instead of retrying indefinitely. Setting a valid channel again clears the pause and makes its pending work eligible. Invalid bot credentials require operator correction rather than silently discarding announcements.

## Test seams

The system boundary includes command handling, market transactions, announcement persistence, rendering, and the worker. Its clients are server administrators and announcement recipients. Keep internal collaborators real and control only relevant inputs and external effects.

| Obstacle                                           | Seam                                                                      | Production composition                                                                               |
| -------------------------------------------------- | ------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Mutable market state changes delayed messages      | Renderer takes the saved snapshot                                         | The worker uses the same renderer covered by output tests                                            |
| Ambient time makes retry checks slow or flaky      | Small clock callable and a bounded worker operation separate from polling | Production supplies UTC; tests supply controlled instants, including a fresh time after send failure |
| Discord responses must be controllable             | Inject concrete Serenity `Http`                                           | Real authenticated client in production; existing Wiremock proxy pattern in tests                    |
| Fake storage cannot establish atomicity or locking | Inject the real `Store` with an isolated PostgreSQL instance              | Use the real migration, transactions, and restricted runtime login                                   |
| Isolated worker tests could miss startup wiring    | Shared worker-starting function called by gateway startup                 | Composition test exercises real worker startup, delivery, and shutdown                               |

Reuse `prediction_bot/lib/src/discord_tests.rs` HTTP construction and `prediction_bot/lib/tests/store_test.rs` container fixtures. The current HTTP fixture disables Serenity rate limiting; those tests do not establish rate-limiter behavior. Do not add public debug accessors, test-only production branches, mocked SQL repositories, or assertions about internal helper calls.

For concurrency tests, synchronize a recording HTTP responder and the test with explicit barriers or notifications. Use bounded test timeouts to detect hangs, not arbitrary sleeps to establish ordering.

## Verification cases

Unit-level output tests cover all three message forms, no-winner refunds, preserved original content, mention suppression, content limits, and retry policy boundaries. Expected results are independently specified rather than recomputed using production helpers.

PostgreSQL and HTTP integration tests cover:

1. Successful command commits one deliverable announcement; repeated interaction delivery does not add another.
2. A fixture-local database constraint rejects an outbox insert: market changes and command receipt roll back. A failure later in the transaction also leaves no announcement.
3. Disabled configuration and unsupported event types produce no announcements; replay and reenabling do not backfill history.
4. A failed send survives store and worker reconstruction, does not retry before its deadline, and retries when due.
5. Creation precedes resolution or cancellation for a guild; another guild progresses while the first waits or fails.
6. Unauthorized or invalid configuration commands preserve settings. Redelivery of an old configuration command cannot override newer settings.
7. Pending work moves on channel change and is discarded on disable. In-flight requests obey the documented exception, and stale failures cannot pause a new destination.
8. Permission and missing-channel failures pause delivery; status exposes a safe reason; valid reconfiguration resumes work.
9. An accepted HTTP request followed by failure to record success remains recoverable after restart; this test permits the documented duplicate.
10. The runtime login can configure, enqueue, retry, and record delivery while event and receipt mutation remains forbidden. Repeated migrations preserve existing history.
11. Both slash and modal market creation reach the shared transactional enqueue path. Private command responses remain private.
12. The production worker-starting function uses the real store and HTTP client, sends pending work, and honors shutdown without stranding it.

Rendering and retry policy tests use output assertions. Integration tests use observable market/configuration state and outgoing HTTP effects. Query assertions are reserved for durability and transaction invariants, not SQL call sequences.

PostgreSQL appears application-owned from current repository evidence; fixtures create private instances. If external consumers directly depend on the new tables, migration compatibility becomes an additional external contract. Discord is independently observable, making outgoing message assertions appropriate. Each HTTP test uses its own endpoint and data.

Regression protection targets loss, duplicate enqueueing, wrong destinations, broken permissions, and starvation. Refactoring resistance is supported by behavior-level assertions; it has not been demonstrated through an actual refactor. Feedback speed is unverified until measured. Maintainability comes from existing fixtures, small explicit setups, and no duplicate implementation of delivery logic in a fake.

During implementation run Gazelle immediately after source edits, then repository-wide formatting, focused bot tests, and the required repository build. Local HTTP checks do not establish live Discord permission enforcement, gateway behavior, or provider rate limits. No implementation tests have been run for this design document.

## Repository baseline and next step

The worktree was fetched and merged with `origin/main`; it was already current at `0c2448cb`. The Wiremock migration `a496053c` is included.

This document specifies the approved feature and test boundaries. After written-spec review, create the implementation plan using the writing-plans skill. Implementation has not started.
