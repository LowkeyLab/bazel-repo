---
status: accepted
date: 2026-10-06
---

# Deliver complete resolution positions in multiple messages

Resolution announcements disclose every bettor's accepted stakes by outcome, total stake, payout, and net result. A resolution may use additional numbered messages when its report exceeds Discord's text limit, with persisted delivery progress so retries resume after the last acknowledged message; this preserves readable results in Discord without omitting bettors or requiring a downloaded report.

This revises the single-message resolution rule in the [announcement design](../../../docs/superpowers/specs/2026-09-18-prediction-bot-announcements-design.md). The trade-off is additional durable delivery state and partial-delivery handling; the existing acknowledgement gap still permits duplicates if Discord accepts a message before its acknowledgement is recorded.
