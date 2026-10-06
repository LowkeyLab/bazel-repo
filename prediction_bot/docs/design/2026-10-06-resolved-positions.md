# Positions in market resolution announcements

Date: 2026-10-06
Status: Presentation approved in conversation; delivery edge cases and final scope confirmation pending.

## Agreed behavior

- Extend the automatic public resolution announcement in the configured announcement channel, using the existing server announcement setting.
- Include every bettor on the market, including those receiving zero payout; omit members who placed no bets.
- Group each bettor's repeated bets by outcome and show stakes by outcome, total stake, payout, and net result.
- Net result is the recorded settlement payout minus all stakes that bettor placed on this market. A bettor can back the winning outcome and still have a negative net result.
- Sort bettors by descending net result, breaking ties by ascending Discord user ID. List their outcome stakes in market outcome order.
- Identify bettors with Discord account references while suppressing mention notifications.
- For a resolution with no winning bets, retain the refund explanation and show returned stakes as payouts with net result zero.
- For a market with no bets, show "No bets were placed."
- Split large reports into additional numbered messages and persist delivery progress, so acknowledged parts are not sent again during ordinary retries.

## Existing constraints

Resolution events contain the authoritative payout allocations; the market retains accepted bets, but its projection does not retain those allocations. Capture the report from those facts during the existing resolution transaction, rather than recomputing settlement or reading current state at delivery time.

Announcements already use immutable event-time snapshots and at-least-once delivery. Discord may accept a part before the bot records its acknowledgement, so a retry can duplicate that part. The existing announcement setting controls enqueueing, and disabled announcements do not enqueue resolutions.

## Open delivery decisions

- What happens to unsent parts when the channel changes or announcements are disabled after some parts were delivered?
- How should legacy queued resolutions without saved position details behave?
- What context should each part repeat, including when a report spans channels?

## Documentation

- [Glossary](../../GLOSSARY.md)
- [Multiple-message delivery decision](../adr/0001-deliver-complete-resolution-positions-in-multiple-messages.md)
- [Original announcement design](../../../docs/superpowers/specs/2026-09-18-prediction-bot-announcements-design.md)
