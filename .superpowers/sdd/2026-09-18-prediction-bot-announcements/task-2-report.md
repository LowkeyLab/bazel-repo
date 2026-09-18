# Task 2 report: atomic announcement enqueueing

## Files changed

- `prediction_bot/lib/src/announcements/persistence.rs`
  - Added `enqueue`, which reads the current settings in the caller transaction, returns for disabled/unconfigured settings and non-market events, builds `SnapshotV1` from the applied event/state, and inserts the outbox row using the event occurrence time as `next_attempt_at`.
- `prediction_bot/lib/src/store.rs`
  - Invokes `enqueue` after each event row is appended and before command receipt insertion and commit, using the post-apply view.
- `prediction_bot/lib/tests/announcements_test.rs`
  - Added real PostgreSQL tests for snapshots, duplicate replay, rollback atomicity, disabled/no-backfill behavior, paused enabled settings, restarted-store reconstruction, and concurrent disable/create behavior.
- `prediction_bot/lib/BUILD.bazel`
  - Added the existing `serde_json` crate to the announcement integration-test target for SQL JSON assertions.

## RED evidence

After adding the snapshot and failure-path tests, I ran:

```sh
nix develop --command aspect test //prediction_bot/lib:announcements_test --test_filter='market_events_enqueue_durable_snapshots_once_at_their_original_revisions|an_outbox_failure_rolls_back_the_market_and_receipt|a_receipt_failure_rolls_back_the_enqueued_snapshot'
```

The initial compilation needed `serde_json` added to the test target. The rerun executed 7 tests: 5 passed and 2 failed. The snapshot assertion read `[]` instead of the four expected revision-4 through revision-7 rows; the outbox-constraint case found the market command succeeded, proving no enqueue happened. The receipt constraint case passed at that point because no outbox row was present yet.

## GREEN evidence

Source edits were each immediately followed by:

```sh
nix develop --command bazel run //:gazelle
```

The initial implementation verification ran:

```sh
nix develop --command aspect test //prediction_bot/lib:announcements_test --test_filter='market_events_enqueue_durable_snapshots_once_at_their_original_revisions|an_outbox_failure_rolls_back_the_market_and_receipt|a_receipt_failure_rolls_back_the_enqueued_snapshot' --test_output=errors
```

Its log reported 7 passed, 0 failed. After formatting, the required storage targets were run together once:

```sh
nix develop --command aspect test //prediction_bot/lib:announcements_test //prediction_bot/lib:store_test --test_output=errors
```

Their logs reported `announcements_test`: 7 passed, 0 failed; `store_test`: 21 passed, 0 failed.

After adding the requested setting/concurrency coverage and running final formatting/Gazelle, the affected target was rerun:

```sh
nix develop --command aspect test //prediction_bot/lib:announcements_test --test_output=errors
```

Its log reported 9 passed, 0 failed. `nix develop --command aspect format --scope=all` completed with no files modified on the final pass.

## Executed behavior coverage

- Creation, resolution, and cancellation store the exact JSON snapshots at the original event revisions and at occurrence times 1000/2000; repeated create delivery leaves only its original row.
- An owner-installed outbox constraint rolls back event, market projection, and receipt. A receipt constraint after enqueue rolls back the outbox row.
- A join and historical creation before configuration leave no row after configuration; there is no replay backfill.
- A paused-but-enabled setting queues work. A fresh `Store` instance reconstructs committed history while the pending row remains in SQL.
- Actual bet and periodic grant commands leave the sole market snapshot unchanged.
- Disabling discards prior pending work and a later market creation creates no replacement row.
- Concurrent disable and creation produce either no row or a discarded row, never a pending row, exercising the guild transaction lock ordering.

## Concerns

- `PendingAnnouncement` remains unused and continues to emit the existing dead-code warning. This is intentionally deferred to Task 4, which consumes it.
- Full-repository `aspect build //...` is assigned to the parent agent and was not run here.
