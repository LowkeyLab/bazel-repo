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
        stakes: None,
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
        stakes: None,
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
        stakes: None,
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
        stakes: None,
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
        stakes: None,
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
            stakes: None,
            id: "00000000-0000-4000-8000-000000000001".into(),
            question: text.into(),
            creator: UserId(42),
            options: vec![text.into(), "No".into()],
            closes_at: 2000,
            occurred_at: 1000,
        },
        SnapshotV1::Resolved {
            stakes: None,
            odds: vec![],
            id: "00000000-0000-4000-8000-000000000001".into(),
            question: text.into(),
            winner: text.into(),
            refunded: false,
            occurred_at: 1000,
        },
        SnapshotV1::Cancelled {
            stakes: None,
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
            stakes: Some(super::StakeSummary {
                total: crate::types::Points(0),
                outcomes: vec![crate::types::Points(0); 10],
            }),
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
            assert_that!(
                text,
                contains_substring(format!("{}. {i}", i + 1)),
                "{text}"
            );
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
            "stakes": {"total": 9_223_372_036_854_775_800_i64, "outcomes": vec![922_337_203_685_477_580_i64; 10]},
            "odds": (0..10).map(|i| serde_json::json!({
                "label": format!("{i}{}", "*".repeat(79)), "tenths_percent": 100, "unchanged": true
            })).collect::<Vec<_>>()
        }}))
        .unwrap();
        let payload = payload(snapshot);
        let text = content(&payload);
        assert_that!(text.encode_utf16().count(), le(2000));
        for i in 0..10 {
            assert_that!(text, contains_substring(format!("{}. {i}", i + 1)));
        }
        assert_that!(text.matches("10.0%").count(), eq(10));
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
            {"label": "Unchanged", "tenths_percent": 100, "unchanged": true},
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
        contains_substring("Unchanged — 10.0% implied chance ➖ unchanged\n")
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
        (vec![(0, 5)], 0, 5, [" ➖ unchanged", " ➖ unchanged"]),
        (
            vec![(0, 10000), (1, 10000)],
            0,
            1,
            [" ➖ unchanged", " ➖ unchanged"],
        ),
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
            payouts: Vec::new(),
            resolvers: Default::default(),
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
            stakes: None,
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

#[googletest::test]
fn unchanged_odds_remain_readable_by_the_previous_snapshot_reader() {
    use crate::odds::OutcomeOdds;

    // Preserve the previous release's wire schema to catch rollback incompatibility.
    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum PreviousMovement {
        Up,
        Down,
    }
    #[derive(serde::Deserialize)]
    struct PreviousOdds {
        label: String,
        tenths_percent: Option<u16>,
        #[serde(default)]
        movement: Option<PreviousMovement>,
    }

    let previous: OutcomeOdds = serde_json::from_value(serde_json::json!({
        "label": "Yes", "tenths_percent": 1000
    }))
    .unwrap();
    let unchanged = previous.clone().with_previous(&previous);
    let saved = serde_json::to_value(&unchanged).unwrap();
    let old_reader: PreviousOdds = serde_json::from_value(saved.clone()).unwrap();
    assert_that!(old_reader.label, eq("Yes"));
    assert_that!(old_reader.tenths_percent, eq(Some(1000)));
    assert_that!(old_reader.movement.is_none(), eq(true));
    let current_reader: OutcomeOdds = serde_json::from_value(saved).unwrap();
    assert_that!(current_reader.movement_indicator(), eq(" ➖ unchanged"));
}

#[googletest::test]
fn recorded_creation_stakes_render_numbered_zero_stake_table() {
    let snapshot = serde_json::from_value(serde_json::json!({"Created": {
        "id": "rain-1", "question": "Will it rain?", "creator": 7,
        "options": ["Yes", "No"], "closes_at": 2000, "occurred_at": 1000,
        "stakes": {"total": 0, "outcomes": [0, 0]}
    }}))
    .unwrap();
    let message = payload(snapshot);
    assert_that!(
        content(&message),
        contains_substring(
            "Outcomes:\n```\nChoice | Points staked | Implied chance | Movement\n1. Yes |             0 | N/A (no bets)  |\n2. No  |             0 | N/A (no bets)  |\nTotal  |             0 | -              | -\n```"
        )
    );
    assert_mentions_disabled(&message);
}

#[googletest::test]
fn legacy_snapshots_decode_and_preserve_their_messages_without_invented_stakes() {
    for (kind, heading, details) in [
        (
            "Created",
            "📈 Market created",
            "Creator: <@7>\nOutcomes:\n• Yes — N/A (no bets) implied chance\n• No — N/A (no bets) implied chance\nCloses: <t:2000:F>",
        ),
        ("BetPlaced", "🎲 Another bet", "2 bets placed"),
        (
            "Resolved",
            "✅ Market resolved",
            "Winning outcome: Yes\nStakes refunded: no",
        ),
        ("Cancelled", "🚫 Market cancelled", "Stakes refunded: yes"),
    ] {
        for with_odds in [false, true] {
            let mut fields = serde_json::json!({
                "id": "rain-1", "question": "Rain?", "creator": 7,
                "options": ["Yes", "No"], "closes_at": 2000, "occurred_at": 1000,
                "bet_count": 2, "winner": "Yes", "refunded": false
            });
            if with_odds {
                fields["odds"] = serde_json::json!([
                    {"label": "Yes", "tenths_percent": 750},
                    {"label": "No", "tenths_percent": 250}
                ]);
            }
            let snapshot = serde_json::from_value(serde_json::json!({kind: fields})).unwrap();
            let message = payload(snapshot);
            let odds = if with_odds && kind != "Created" {
                "\nOutcomes:\n• Yes — 75.0% implied chance\n• No — 25.0% implied chance"
            } else {
                ""
            };
            assert_that!(
                content(&message),
                eq(format!(
                    "{heading}\nMarket ID: `rain-1`\nQuestion: Rain?\n{details}{odds}\nEvent time: <t:1000:F>"
                ))
            );
            assert_mentions_disabled(&message);
        }
    }
}

#[googletest::test]
fn exact_saved_stakes_render_with_separators_and_existing_movement() {
    for kind in ["BetPlaced", "Resolved", "Cancelled"] {
        let snapshot = serde_json::from_value(serde_json::json!({kind: {
            "id": "rain-1", "question": "Rain?", "occurred_at": 1000,
            "bet_count": 2, "winner": "Yes", "refunded": false,
            "stakes": {"total": 1000, "outcomes": [600, 277, 123, 0]},
            "odds": [
                {"label": "Yes", "tenths_percent": 600, "movement": "up"},
                {"label": "No", "tenths_percent": 277, "movement": "down"},
                {"label": "Other", "tenths_percent": 123, "unchanged": true},
                {"label": "Legacy", "tenths_percent": 0}
            ]
        }}))
        .unwrap();
        let message = payload(snapshot);
        assert_that!(
            content(&message),
            contains_substring(
                "Outcomes:\n```\nChoice    | Points staked | Implied chance | Movement\n1. Yes    |           600 | 60.0%          | 🟢 ⬆️\n2. No     |           277 | 27.7%          | 🔴 ⬇️\n3. Other  |           123 | 12.3%          | ➖ unchanged\n4. Legacy |             0 | 0.0%           |\nTotal     |         1,000 | -              | -\n```"
            )
        );
        assert_mentions_disabled(&message);
    }
}

#[googletest::test]
fn saved_stakes_preserve_exact_maximum_amount_with_all_supported_outcomes() {
    for kind in ["Created", "BetPlaced", "Resolved", "Cancelled"] {
        for count in 2..=10 {
            for character in ["🔮", "*", "\\"] {
                let options: Vec<_> = (0..count)
                    .map(|index| format!("{index}{}", character.repeat(79)))
                    .collect();
                let stakes = if kind == "Created" {
                    vec![0; count]
                } else {
                    let mut amounts = vec![0; count];
                    amounts[0] = i64::MAX;
                    amounts
                };
                let snapshot = serde_json::from_value(serde_json::json!({kind: {
                    "id": "12345678-1234-1234-1234-123456789abc",
                    "question": character.repeat(200), "occurred_at": i64::MAX,
                    "creator": u64::MAX, "closes_at": i64::MAX, "options": options,
                    "bet_count": 100, "winner": character.repeat(80), "refunded": true,
                    "stakes": {"total": if kind == "Created" {0} else {i64::MAX}, "outcomes": stakes},
                    "odds": options.iter().enumerate().map(|(index, label)| serde_json::json!({
                        "label": label, "tenths_percent": if index == 0 {1000} else {0}, "unchanged": true
                    })).collect::<Vec<_>>()
                }})).unwrap();
                let message = payload(snapshot);
                let text = content(&message);
                assert_that!(text.encode_utf16().count(), le(2000));
                for index in 0..count {
                    let prefix = format!("{}. {index}", index + 1);
                    assert_that!(text, contains_substring(prefix));
                }
                if kind != "Created" {
                    assert_that!(text, contains_substring("Outcomes:\n```"));
                    assert_that!(text.matches("9,223,372,036,854,775,807").count(), eq(2));
                }
                assert_mentions_disabled(&message);
            }
        }
    }
}

#[googletest::test]
fn creation_table_keeps_unicode_and_neutralizes_row_and_fence_controls() {
    let message = payload(SnapshotV1::Created {
        stakes: Some(super::StakeSummary {
            total: crate::types::Points(0),
            outcomes: vec![crate::types::Points(0); 2],
        }),
        id: "safe-table".into(),
        question: "Unicode?".into(),
        creator: UserId(7),
        options: vec!["茶🔮e\u{301}".into(), "👩‍💻\n```|\u{202e}\u{0}".into()],
        closes_at: 2000,
        occurred_at: 1000,
    });
    let text = content(&message);
    assert_that!(
        text,
        contains_substring(
            "```\nChoice     | Points staked | Implied chance | Movement\n1. 茶🔮e\u{301}   |             0 | N/A (no bets)  |\n2. 👩‍💻 ˋˋˋ¦ |             0 | N/A (no bets)  |\nTotal      |             0 | -              | -\n```"
        )
    );
    assert_that!(text.matches("```").count(), eq(2));
    assert_that!(text.contains('\u{202e}'), eq(false));
    assert_that!(text.contains('\u{0}'), eq(false));
    assert_mentions_disabled(&message);
}

#[googletest::test]
fn creation_table_shortens_labels_without_splitting_clusters_or_losing_numbering() {
    let message = payload(SnapshotV1::Created {
        stakes: Some(super::StakeSummary {
            total: crate::types::Points(0),
            outcomes: vec![crate::types::Points(0); 4],
        }),
        id: "short-labels".into(),
        question: "Which choice?".into(),
        creator: UserId(7),
        options: vec![
            format!("{}yes", "a".repeat(77)),
            format!("{}no", "a".repeat(78)),
            format!("{}👩‍💻tail", "a".repeat(30)),
            "e\u{301}".repeat(40),
        ],
        closes_at: 2000,
        occurred_at: 1000,
    });
    let text = content(&message);
    for prefix in [
        "1. aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa… |",
        "2. aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa… |",
        "3. aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa…  |",
        "4. ééééééééééééééééééééééééééééééé… |",
    ] {
        assert_that!(text, contains_substring(prefix));
    }
    assert_that!(text.matches("N/A (no bets)").count(), eq(4));
    assert_mentions_disabled(&message);
}

#[googletest::test]
fn creation_table_budget_includes_padding_and_all_original_event_details() {
    let mut options = vec!["a".repeat(80)];
    options.extend((1..10).map(|index| {
        format!(
            "{index}{}",
            "a\u{1d165}\u{1d165}\u{1d165}\u{1d165}\u{1d165}".repeat(13)
        )
    }));
    let message = payload(SnapshotV1::Created {
        stakes: Some(super::StakeSummary {
            total: crate::types::Points(0),
            outcomes: vec![crate::types::Points(0); 10],
        }),
        id: "12345678-1234-1234-1234-123456789abc".into(),
        question: "🔮".repeat(200),
        creator: UserId(u64::MAX),
        options,
        closes_at: i64::MAX,
        occurred_at: i64::MAX,
    });
    let text = content(&message);
    assert_that!(text.encode_utf16().count(), le(2000), "{text}");
    let table = text.split("```").nth(1).unwrap();
    assert_that!(
        table.lines().filter(|line| line.contains(" | ")).count(),
        eq(12)
    );
    for number in 1..=10 {
        assert_that!(table, contains_substring(format!("\n{number}. ")));
    }
    assert_that!(table, contains_substring("\nTotal"));
    assert_that!(
        table
            .lines()
            .filter(|line| line
                .split(" | ")
                .nth(1)
                .is_some_and(|cell| cell.trim() == "0"))
            .count(),
        eq(11)
    );
    assert_that!(
        text,
        contains_substring("Market ID: `12345678-1234-1234-1234-123456789abc`")
    );
    assert_that!(text, contains_substring("Creator: <@18446744073709551615>"));
    assert_that!(
        text,
        contains_substring("Closes: <t:9223372036854775807:F>")
    );
    assert_that!(text, ends_with("Event time: <t:9223372036854775807:F>"));
    assert_mentions_disabled(&message);
}

#[googletest::test]
fn recorded_bet_and_settlement_tables_keep_safe_unicode_labels() {
    for kind in ["BetPlaced", "Resolved", "Cancelled"] {
        let snapshot = serde_json::from_value(serde_json::json!({kind: {
            "id": "safe-table", "question": "Unicode?", "occurred_at": 1000,
            "bet_count": 0, "winner": "茶🔮e\u{301}", "refunded": true,
            "stakes": {"total": 0, "outcomes": [0, 0]},
            "odds": [
                {"label": "茶🔮e\u{301}", "tenths_percent": null},
                {"label": "👩‍💻\r\t```|\u{202e}\u{0}", "tenths_percent": null}
            ]
        }}))
        .unwrap();
        let message = payload(snapshot);
        let text = content(&message);
        assert_that!(
            text,
            contains_substring("1. 茶🔮e\u{301}    |             0 | N/A (no bets)  |")
        );
        assert_that!(
            text,
            contains_substring("2. 👩‍💻  ˋˋˋ¦ |             0 | N/A (no bets)  |")
        );
        assert_that!(text.matches("```").count(), eq(2));
        assert_that!(text.contains('\u{202e}'), eq(false));
        assert_that!(text.contains('\u{0}'), eq(false));
        assert_mentions_disabled(&message);
    }
}

#[googletest::test]
fn maximum_bet_and_settlement_tables_budget_unicode_padding_without_losing_amounts() {
    let mut options = vec!["a".repeat(80)];
    options.extend((1..10).map(|index| {
        format!(
            "{index}{}",
            "a\u{1d165}\u{1d165}\u{1d165}\u{1d165}\u{1d165}".repeat(13)
        )
    }));
    for kind in ["BetPlaced", "Resolved", "Cancelled"] {
        let snapshot = serde_json::from_value(serde_json::json!({kind: {
            "id": "12345678-1234-1234-1234-123456789abc",
            "question": "🔮".repeat(200), "occurred_at": i64::MAX,
            "bet_count": usize::MAX, "winner": "🔮".repeat(80), "refunded": true,
            "stakes": {"total": i64::MAX, "outcomes": [i64::MAX, 0, 0, 0, 0, 0, 0, 0, 0, 0]},
            "odds": options.iter().enumerate().map(|(index, label)| serde_json::json!({
                "label": label, "tenths_percent": if index == 0 {1000} else {0}, "unchanged": true
            })).collect::<Vec<_>>()
        }}))
        .unwrap();
        let message = payload(snapshot);
        let text = content(&message);
        assert_that!(text.encode_utf16().count(), le(2000), "{text}");
        let table = text.split("```").nth(1).unwrap();
        assert_that!(
            table.lines().filter(|line| line.contains(" | ")).count(),
            eq(12)
        );
        for number in 1..=10 {
            assert_that!(table, contains_substring(format!("\n{number}. ")));
        }
        assert_that!(table.matches("9,223,372,036,854,775,807").count(), eq(2));
        assert_that!(
            table
                .lines()
                .filter(|line| line
                    .split(" | ")
                    .nth(1)
                    .is_some_and(|cell| cell.trim() == "0"))
                .count(),
            eq(9)
        );
        assert_that!(
            text,
            contains_substring("Market ID: `12345678-1234-1234-1234-123456789abc`")
        );
        assert_that!(text, ends_with("Event time: <t:9223372036854775807:F>"));
        if kind == "Resolved" {
            assert_that!(text, contains_substring("Winning outcome: 🔮"));
        }
        if kind != "BetPlaced" {
            assert_that!(text, contains_substring("Stakes refunded: yes"));
        } else {
            assert_that!(text, contains_substring("18446744073709551615 bets placed"));
        }
        assert_mentions_disabled(&message);
    }
}
