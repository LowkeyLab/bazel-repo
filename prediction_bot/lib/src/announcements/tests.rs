use googletest::{
    assert_that,
    matchers::{contains_substring, ends_with, eq, le, starts_with},
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
            id: "00000000-0000-4000-8000-000000000001".into(),
            question: text.into(),
            winner: text.into(),
            refunded: false,
            occurred_at: 1000,
        },
        SnapshotV1::Cancelled {
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
