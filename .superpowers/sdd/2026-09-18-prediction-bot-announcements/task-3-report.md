# Task 3 report: deterministic rendering and retry decisions

## Files changed

- `prediction_bot/lib/src/announcements/render.rs`
  - Renders creation, resolution, and cancellation snapshots into plain Discord messages with original timestamps, event-specific fields, escaped user content, non-pinging creator IDs, disabled allowed mentions, and a 2,000 UTF-16-unit cap with a truncation marker.
- `prediction_bot/lib/src/announcements/worker.rs`
  - Defines the Task 4-compatible `AttemptOutcome`, overflow-safe capped retry scheduling, and safe Serenity delivery-failure classification.
- `prediction_bot/lib/src/announcements/tests.rs`
  - Adds semantic JSON output tests for all message forms, refunds, timestamps, hostile Markdown/mentions, Unicode truncation, retry boundaries, and delivery-failure outcomes.
- `prediction_bot/lib/src/announcements.rs`
  - Declares the renderer, worker, and unit-test modules.

## RED evidence

After writing and wiring the tests before production code, I ran:

```sh
nix develop --command aspect test //prediction_bot/lib:discord_test --test_filter=announcements
```

The target failed to compile with `E0432` because `announcements::render` and `announcements::worker` did not exist. No tests executed, which was the expected missing-feature failure.

## GREEN evidence

Every source-file edit was immediately followed by:

```sh
nix develop --command bazel run //:gazelle
```

After implementation, the focused target passed. Following the final test assertion and a repository-wide formatting pass, I ran the fresh verification:

```sh
nix develop --command aspect test //prediction_bot/lib:discord_test --test_filter=announcements --test_output=errors
```

Bazel reported 1 test target passed. The Rust test log reported 64 passed, 0 failed, including all 7 Task 3 announcement tests. The command completed in 13.7 seconds.

Formatting was run with:

```sh
nix develop --command aspect format --scope=all
```

It reported that no files were modified.

## Executed behavior coverage

- Creation output includes a stable heading and market ID, saved question, numeric non-mention creator ID, options, closing time, and original event time.
- Resolution output includes the saved winner and distinguishes refunded no-winner settlement; cancellation explicitly confirms refunds.
- User Markdown is escaped, Unicode remains intact, `@everyone` cannot ping, and the serialized allowed-mention parse list is empty with replied-user mentions disabled.
- Oversized Unicode output stays within 2,000 UTF-16 units, retains the heading and market ID, and ends with a truncation marker.
- Retry delays begin at five seconds, double, cap at five minutes, respect longer nonnegative provider delays, ignore negative provider delays, and saturate at `i64::MAX`.
- Transport, server, rate-limit, permission, missing-channel, payload, and authentication paths produce retry or pause outcomes with fixed safe reasons and no copied response body or credentials.

## Concerns

- `PendingAnnouncement` and the `AttemptOutcome::Delivered` variant emit dead-code warnings until Task 4 consumes them. These are expected handoff APIs, not unused final design.
- Provider-specific retry delays are accepted by `retry_at`; Serenity's normal rate limiter handles provider responses and Task 4 can pass a surfaced delay when available.
- Full-repository `aspect build //...` is assigned to the parent agent and was not run here.
