use googletest::{
    assert_that,
    matchers::{contains_substring, ends_with, eq, le, not, starts_with},
};
use serde_json::Value;

use super::{
    SnapshotV1,
    render::render,
    worker::{AttemptOutcome, classify_delivery_failure, classify_http_failure, retry_at},
};
use crate::types::UserId;

fn payload(snapshot: SnapshotV1) -> Value {
    serde_json::to_value(render(&snapshot)).expect("announcement message serializes")
}

fn content(payload: &Value) -> &str {
    payload["content"].as_str().expect("message has content")
}

fn assert_mentions_disabled(payload: &Value) {
    assert_that!(
        payload["allowed_mentions"]["parse"],
        eq(&serde_json::json!([]))
    );
    assert_that!(payload["allowed_mentions"]["replied_user"], eq(false));
}

#[googletest::test]
fn creation_renders_saved_details_as_non_pinging_text() {
    let payload = payload(SnapshotV1::Created {
        id: "rain-1".into(),
        question: "Will **rain** ping @everyone?".into(),
        creator: UserId(42),
        options: vec!["Yes_please".into(), "No | sunshine".into()],
        closes_at: 2_000,
        occurred_at: 1_000,
    });
    let content = content(&payload);

    assert_that!(content, contains_substring("Market created"));
    assert_that!(content, contains_substring("Market ID: `rain-1`"));
    assert_that!(
        content,
        contains_substring(r"Will \*\*rain\*\* ping @everyone?")
    );
    assert_that!(content, contains_substring("Creator: <@42>"));
    assert_that!(content, contains_substring(r"Yes\_please"));
    assert_that!(content, contains_substring(r"No \| sunshine"));
    assert_that!(content, contains_substring("<t:2000:F>"));
    assert_that!(content, contains_substring("<t:1000:F>"));
    assert_mentions_disabled(&payload);
}

#[googletest::test]
fn resolution_distinguishes_a_refund_and_uses_the_original_event_time() {
    let payload = payload(SnapshotV1::Resolved {
        odds: vec![],
        id: "unicode-🔮".into(),
        question: "Café or 茶?".into(),
        winner: "No winners".into(),
        refunded: true,
        occurred_at: 3_000,
    });
    let content = content(&payload);

    assert_that!(content, contains_substring("Market resolved"));
    assert_that!(content, contains_substring("Market ID: `unicode-🔮`"));
    assert_that!(content, contains_substring("Café or 茶?"));
    assert_that!(content, contains_substring("Winning outcome: No winners"));
    assert_that!(
        content,
        contains_substring("Stakes refunded: yes (no winning bets)")
    );
    assert_that!(content, contains_substring("<t:3000:F>"));
    assert_mentions_disabled(&payload);
}

#[googletest::test]
fn cancellation_confirms_refunds_and_escapes_hostile_markdown() {
    let payload = payload(SnapshotV1::Cancelled {
        odds: vec![],
        id: "cancel-1".into(),
        question: "[click](https://example.invalid) # heading".into(),
        occurred_at: 4_000,
    });
    let content = content(&payload);

    assert_that!(content, contains_substring("Market cancelled"));
    assert_that!(content, contains_substring("Market ID: `cancel-1`"));
    assert_that!(
        content,
        contains_substring(r"\[click\]\(https://example.invalid\) \# heading")
    );
    assert_that!(content, contains_substring("Stakes refunded: yes"));
    assert_that!(content, contains_substring("<t:4000:F>"));
    assert_mentions_disabled(&payload);
}

#[googletest::test]
fn long_unicode_content_stays_within_discords_utf16_limit() {
    let payload = payload(SnapshotV1::Created {
        id: "kept-id".into(),
        question: "🔮".repeat(1_500),
        creator: UserId(7),
        options: vec!["A".repeat(1_500), "B".into()],
        closes_at: 2_000,
        occurred_at: 1_000,
    });
    let content = content(&payload);

    assert_that!(content.encode_utf16().count(), le(2_000));
    assert_that!(
        content,
        starts_with("📈 Market created\nMarket ID: `kept-id`")
    );
    assert_that!(content, contains_substring("…"));
    assert_that!(content, contains_substring("Closes: <t:2000:F>"));
    assert_that!(content, ends_with("Event time: <t:1000:F>"));
    assert_mentions_disabled(&payload);
}

#[googletest::test]
fn retries_start_after_failure_and_cap_the_local_delay() {
    assert_that!(retry_at(1000, 1, None), eq(1005));
    assert_that!(retry_at(1000, 2, None), eq(1010));
    assert_that!(retry_at(1000, 100, None), eq(1300));
    assert_that!(retry_at(1000, 1, Some(600)), eq(1600));
    assert_that!(retry_at(i64::MAX - 1, 1, None), eq(i64::MAX));
}

#[googletest::test]
fn retry_ignores_negative_provider_delays() {
    assert_that!(retry_at(1000, 1, Some(-60)), eq(1005));
}

#[googletest::test]
fn truncation_preserves_a_valid_maximum_length_market_id() {
    let id = "12345678-1234-1234-1234-123456789abc";
    let payload = payload(SnapshotV1::Created {
        id: id.into(),
        question: "🔮".repeat(1_500),
        creator: UserId(7),
        options: vec!["A".repeat(1_500), "B".into()],
        closes_at: 2_000,
        occurred_at: 1_000,
    });

    assert_that!(
        content(&payload),
        starts_with(format!("📈 Market created\nMarket ID: `{id}`"))
    );
}

#[googletest::test]
fn permanent_local_delivery_errors_pause_without_copying_details() {
    let model = serenity::Error::Model(serenity::model::ModelError::MessageTooLong(1));
    assert_that!(
        classify_delivery_failure(&model),
        eq(AttemptOutcome::Pause {
            reason: "The announcement could not be prepared for Discord; contact an operator.",
        })
    );

    let missing_application =
        serenity::Error::Http(serenity::http::HttpError::ApplicationIdMissing);
    assert_that!(
        classify_delivery_failure(&missing_application),
        eq(AttemptOutcome::Pause {
            reason: "The announcement could not be prepared for Discord; contact an operator.",
        })
    );
}

#[googletest::test]
fn ambiguous_json_errors_retry_because_discord_may_have_accepted_the_message() {
    let json = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
    assert_that!(
        classify_delivery_failure(&serenity::Error::Json(json)),
        eq(AttemptOutcome::Retry {
            reason: "discord_json",
            provider_delay: None,
        })
    );
}

#[googletest::test]
fn http_failures_use_safe_delivery_outcomes() {
    let transport = serenity::Error::Io(std::io::Error::new(
        std::io::ErrorKind::ConnectionReset,
        "body must not be persisted",
    ));
    assert_that!(
        classify_delivery_failure(&transport),
        eq(AttemptOutcome::Retry {
            reason: "transport",
            provider_delay: None,
        })
    );
    assert_that!(
        classify_http_failure(429, 0),
        eq(AttemptOutcome::Retry {
            reason: "discord_rate_limit",
            provider_delay: None,
        })
    );
    assert_that!(
        classify_http_failure(500, 0),
        eq(AttemptOutcome::Retry {
            reason: "discord_server_error",
            provider_delay: None,
        })
    );
    assert_that!(
        classify_http_failure(403, 0),
        eq(AttemptOutcome::Pause {
            reason: "Announcement delivery lacks permission to use the configured channel.",
        })
    );
    assert_that!(
        classify_http_failure(404, 10_003),
        eq(AttemptOutcome::Pause {
            reason: "The configured announcement channel is unavailable.",
        })
    );
    assert_that!(
        classify_http_failure(400, 50_035),
        eq(AttemptOutcome::Pause {
            reason: "Discord rejected the announcement payload; reconfigure the destination or contact an operator.",
        })
    );
    assert_that!(
        classify_http_failure(401, 0),
        eq(AttemptOutcome::Retry {
            reason: "discord_authentication",
            provider_delay: None,
        })
    );
}

#[googletest::test]
fn saved_discord_syntax_is_escaped_but_announcement_timestamps_remain_active() {
    let text = r"<t:0:R> <#123> <:coin:456> <a:dance:789> \<t:1:F>";
    let escaped = r"\<t:0:R\> \<\#123\> \<:coin:456\> \<a:dance:789\> \\\<t:1:F\>";
    let snapshots = [
        SnapshotV1::Created {
            id: "00000000-0000-4000-8000-000000000001".into(),
            question: text.into(),
            creator: UserId(42),
            options: vec![text.into(), "No".into()],
            closes_at: 2000,
            occurred_at: 1000,
        },
        SnapshotV1::Resolved {
            odds: vec![],
            id: "00000000-0000-4000-8000-000000000001".into(),
            question: text.into(),
            winner: text.into(),
            refunded: false,
            occurred_at: 1000,
        },
        SnapshotV1::Cancelled {
            odds: vec![],
            id: "00000000-0000-4000-8000-000000000001".into(),
            question: text.into(),
            occurred_at: 1000,
        },
    ];
    for (snapshot, expected_copies) in snapshots.into_iter().zip([2, 2, 1]) {
        let payload = payload(snapshot);
        let text = content(&payload);
        assert_that!(text.matches(escaped).count(), eq(expected_copies), "{text}");
        assert_that!(text, contains_substring("Event time: <t:1000:F>"));
        if text.contains("Market created") {
            assert_that!(text, contains_substring("Closes: <t:2000:F>"));
        }
        assert_mentions_disabled(&payload);
    }
}

#[googletest::test]
fn http_request_timeouts_retry_without_pausing_the_guild() {
    assert_that!(
        classify_http_failure(408, 0),
        eq(AttemptOutcome::Retry {
            reason: "timeout",
            provider_delay: None,
        })
    );
}

#[googletest::test]
fn maximum_creation_preserves_all_fields_when_user_text_expands() {
    for character in ["🔮", "*", "\\"] {
        let payload = payload(SnapshotV1::Created {
            id: "12345678-1234-1234-1234-123456789abc".into(),
            question: character.repeat(200),
            creator: UserId(u64::MAX),
            options: (0..10)
                .map(|i| format!("{i}{}", character.repeat(79)))
                .collect(),
            closes_at: i64::MAX,
            occurred_at: i64::MAX,
        });
        let text = content(&payload);
        assert_that!(text.encode_utf16().count(), le(2000));
        for i in 0..10 {
            assert_that!(text, contains_substring(format!("• {i}")), "{text}");
        }
        assert_that!(
            text,
            contains_substring(format!("Creator: <@{}>", u64::MAX))
        );
        assert_that!(
            text,
            contains_substring(format!("Closes: <t:{}:F>", i64::MAX))
        );
        assert_that!(text, ends_with(format!("Event time: <t:{}:F>", i64::MAX)));
        assert_mentions_disabled(&payload);
    }
}

#[googletest::test]
fn enrollment_renders_a_member_without_pinging_or_inventing_a_market() {
    let snapshot = serde_json::from_value(serde_json::json!({"MemberEnrolled": {
        "user_id": 42, "occurred_at": 1000
    }}))
    .unwrap();
    let payload = payload(snapshot);
    let content = content(&payload);
    assert_that!(
        content,
        contains_substring("<@42> joined this server’s prediction market!")
    );
    assert_that!(content, contains_substring("<t:1000:F>"));
    assert_that!(content, not(contains_substring("Market ID")));
    assert_mentions_disabled(&payload);
}

#[googletest::test]
fn bet_activity_escapes_and_limits_the_saved_question() {
    let snapshot = serde_json::from_value(serde_json::json!({"BetPlaced": {
        "id": "rain-1", "question": "**🌧** @everyone ".repeat(500),
        "bet_count": 2, "occurred_at": 1001
    }}))
    .unwrap();
    let payload = payload(snapshot);
    let content = content(&payload);
    assert_that!(content, contains_substring(r"\*\*🌧\*\*"));
    assert_that!(content, contains_substring("2 bets placed"));
    assert_that!(content, contains_substring("<t:1001:F>"));
    assert_that!(content.encode_utf16().count(), le(2000));
    assert_mentions_disabled(&payload);
}

#[googletest::test]
fn market_announcements_show_saved_percentages() {
    for (kind, extra) in [
        ("BetPlaced", serde_json::json!({"bet_count": 3})),
        (
            "Resolved",
            serde_json::json!({"winner": "Yes", "refunded": false}),
        ),
        ("Cancelled", serde_json::json!({})),
    ] {
        let mut fields = serde_json::json!({
            "id": "rain-1", "question": "Will it rain?", "occurred_at": 1001,
            "odds": [
                {"label": "Yes", "tenths_percent": 750},
                {"label": "No", "tenths_percent": 250}
            ]
        });
        fields
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let snapshot = serde_json::from_value(serde_json::json!({kind: fields})).unwrap();
        let payload = payload(snapshot);
        assert_that!(
            content(&payload),
            contains_substring("Yes — 75.0% implied chance")
        );
        assert_that!(
            content(&payload),
            contains_substring("No — 25.0% implied chance")
        );
        assert_mentions_disabled(&payload);
    }
}

#[googletest::test]
fn new_markets_have_no_bet_based_percentage() {
    let snapshot = serde_json::from_value(serde_json::json!({"Created": {
        "id": "rain-1", "question": "Will it rain?", "creator": 7,
        "options": ["Yes", "No"], "closes_at": 2000, "occurred_at": 1000
    }}))
    .unwrap();
    let payload = payload(snapshot);
    for label in ["Yes", "No"] {
        assert_that!(
            content(&payload),
            contains_substring(format!("{label} — N/A (no bets) implied chance"))
        );
    }
}

#[googletest::test]
fn odds_preserve_every_outcome_within_discords_message_limit() {
    for kind in ["BetPlaced", "Resolved", "Cancelled"] {
        let snapshot = serde_json::from_value(serde_json::json!({kind: {
            "id": "12345678-1234-1234-1234-123456789abc",
            "question": "🔮".repeat(200), "occurred_at": i64::MAX,
            "bet_count": 100, "winner": "*".repeat(80), "refunded": false,
            "odds": (0..10).map(|i| serde_json::json!({
                "label": format!("{i}{}", "*".repeat(79)), "tenths_percent": 100, "movement": "up"
            })).collect::<Vec<_>>()
        }}))
        .unwrap();
        let payload = payload(snapshot);
        let text = content(&payload);
        assert_that!(text.encode_utf16().count(), le(2000));
        for i in 0..10 {
            assert_that!(text, contains_substring(format!("• {i}")));
        }
        assert_that!(text.matches("10.0% implied chance").count(), eq(10));
        assert_that!(
            text,
            contains_substring(format!("Event time: <t:{}:F>", i64::MAX))
        );
        assert_mentions_disabled(&payload);
    }
}

#[googletest::test]
fn bet_announcements_render_saved_movement_and_allow_missing_history() {
    let snapshot = serde_json::from_value(serde_json::json!({"BetPlaced": {
        "id": "12345678-1234-1234-1234-123456789abc",
        "question": "Which choice?", "occurred_at": 1000, "bet_count": 2,
        "odds": [
            {"label": "Rising", "tenths_percent": 600, "movement": "up"},
            {"label": "Falling", "tenths_percent": 200, "movement": "down"},
            {"label": "Unchanged", "tenths_percent": 100},
            {"label": "Legacy", "tenths_percent": 100}
        ]
    }}))
    .unwrap();
    let message = payload(snapshot);
    let text = content(&message);
    assert_that!(
        text,
        contains_substring("Rising — 60.0% implied chance 🟢 ⬆️")
    );
    assert_that!(
        text,
        contains_substring("Falling — 20.0% implied chance 🔴 ⬇️")
    );
    assert_that!(
        text,
        contains_substring("Unchanged — 10.0% implied chance\n")
    );
    assert_that!(text, contains_substring("Legacy — 10.0% implied chance\n"));
    assert_mentions_disabled(&message);
}

#[googletest::test]
fn movement_compares_displayed_percentages_without_inventing_a_first_bet_baseline() {
    use crate::domain::{Bet, Market, Status};
    use crate::odds::OutcomeOdds;
    use crate::types::{OutcomeIndex, Points};

    for (stakes, added_outcome, added_amount, expected) in [
        (vec![], 0, 1, ["", ""]),
        (vec![(0, 5)], 0, 5, ["", ""]),
        (vec![(0, 10000), (1, 10000)], 0, 1, ["", ""]),
        (vec![(0, 1), (1, 1)], 0, 2, [" 🟢 ⬆️", " 🔴 ⬇️"]),
        (vec![(0, 1), (1, 1)], 1, 2, [" 🔴 ⬇️", " 🟢 ⬆️"]),
    ] {
        let mut market = Market {
            creator: UserId(20),
            question: "Which choice?".into(),
            options: vec!["Yes".into(), "No".into()],
            closes_at: 2000,
            created_at: 1000,
            status: Status::Open,
            total_staked: Points(stakes.iter().map(|(_, amount)| amount).sum()),
            bets: stakes
                .into_iter()
                .map(|(outcome, amount)| Bet {
                    user_id: UserId(20),
                    outcome: OutcomeIndex(outcome),
                    amount: Points(amount),
                })
                .collect(),
        };
        let previous = OutcomeOdds::for_market(&market);
        market.bets.push(Bet {
            user_id: UserId(20),
            outcome: OutcomeIndex(added_outcome),
            amount: Points(added_amount),
        });
        market.total_staked.0 += added_amount;
        let odds = OutcomeOdds::for_market(&market)
            .into_iter()
            .zip(&previous)
            .map(|(current, previous)| current.with_previous(previous))
            .collect();
        let message = payload(SnapshotV1::BetPlaced {
            id: "market".into(),
            question: market.question,
            bet_count: market.bets.len(),
            odds,
            occurred_at: 1001,
        });
        for (line, indicator) in content(&message)
            .lines()
            .filter(|line| line.starts_with("• "))
            .zip(expected)
        {
            assert_that!(line, ends_with(format!("implied chance{indicator}")));
        }
    }
}
