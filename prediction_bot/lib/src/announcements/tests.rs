use serde_json::Value;

use super::{
    SnapshotV1,
    render::render,
    worker::{AttemptOutcome, classify_delivery_failure, classify_http_failure, retry_at},
};

fn payload(snapshot: SnapshotV1) -> Value {
    serde_json::to_value(render(&snapshot)).expect("announcement message serializes")
}

fn content(payload: &Value) -> &str {
    payload["content"].as_str().expect("message has content")
}

fn assert_mentions_disabled(payload: &Value) {
    assert_eq!(payload["allowed_mentions"]["parse"], serde_json::json!([]));
    assert_eq!(payload["allowed_mentions"]["replied_user"], false);
}

#[test]
fn creation_renders_saved_details_as_non_pinging_text() {
    let payload = payload(SnapshotV1::Created {
        id: "rain-1".into(),
        question: "Will **rain** ping @everyone?".into(),
        creator: 42,
        options: vec!["Yes_please".into(), "No | sunshine".into()],
        closes_at: 2_000,
        occurred_at: 1_000,
    });
    let content = content(&payload);

    assert!(content.contains("Market created"));
    assert!(content.contains("Market ID: `rain-1`"));
    assert!(content.contains(r"Will \*\*rain\*\* ping @everyone?"));
    assert!(content.contains("Creator: `42`"));
    assert!(!content.contains("<@42>"));
    assert!(content.contains(r"Yes\_please"));
    assert!(content.contains(r"No \| sunshine"));
    assert!(content.contains("<t:2000:F>"));
    assert!(content.contains("<t:1000:F>"));
    assert_mentions_disabled(&payload);
}

#[test]
fn resolution_distinguishes_a_refund_and_uses_the_original_event_time() {
    let payload = payload(SnapshotV1::Resolved {
        id: "unicode-🔮".into(),
        question: "Café or 茶?".into(),
        winner: "No winners".into(),
        refunded: true,
        occurred_at: 3_000,
    });
    let content = content(&payload);

    assert!(content.contains("Market resolved"));
    assert!(content.contains("Market ID: `unicode-🔮`"));
    assert!(content.contains("Café or 茶?"));
    assert!(content.contains("Winning outcome: No winners"));
    assert!(content.contains("Stakes refunded: yes (no winning bets)"));
    assert!(content.contains("<t:3000:F>"));
    assert_mentions_disabled(&payload);
}

#[test]
fn cancellation_confirms_refunds_and_escapes_hostile_markdown() {
    let payload = payload(SnapshotV1::Cancelled {
        id: "cancel-1".into(),
        question: "[click](https://example.invalid) # heading".into(),
        occurred_at: 4_000,
    });
    let content = content(&payload);

    assert!(content.contains("Market cancelled"));
    assert!(content.contains("Market ID: `cancel-1`"));
    assert!(content.contains(r"\[click\]\(https://example.invalid\) \# heading"));
    assert!(content.contains("Stakes refunded: yes"));
    assert!(content.contains("<t:4000:F>"));
    assert_mentions_disabled(&payload);
}

#[test]
fn long_unicode_content_stays_within_discords_utf16_limit() {
    let payload = payload(SnapshotV1::Created {
        id: "kept-id".into(),
        question: "🔮".repeat(1_500),
        creator: 7,
        options: vec!["A".repeat(1_500), "B".into()],
        closes_at: 2_000,
        occurred_at: 1_000,
    });
    let content = content(&payload);

    assert!(content.encode_utf16().count() <= 2_000);
    assert!(content.starts_with("📈 Market created\nMarket ID: `kept-id`"));
    assert!(content.ends_with("\n…"));
    assert_mentions_disabled(&payload);
}

#[test]
fn retries_start_after_failure_and_cap_the_local_delay() {
    assert_eq!(retry_at(1000, 1, None), 1005);
    assert_eq!(retry_at(1000, 2, None), 1010);
    assert_eq!(retry_at(1000, 100, None), 1300);
    assert_eq!(retry_at(1000, 1, Some(600)), 1600);
    assert_eq!(retry_at(i64::MAX - 1, 1, None), i64::MAX);
}

#[test]
fn retry_ignores_negative_provider_delays() {
    assert_eq!(retry_at(1000, 1, Some(-60)), 1005);
}

#[test]
fn http_failures_use_safe_delivery_outcomes() {
    let transport = serenity::Error::Io(std::io::Error::new(
        std::io::ErrorKind::ConnectionReset,
        "body must not be persisted",
    ));
    assert_eq!(
        classify_delivery_failure(&transport),
        AttemptOutcome::Retry {
            reason: "transport",
            provider_delay: None,
        }
    );
    assert_eq!(
        classify_http_failure(500, 0),
        AttemptOutcome::Retry {
            reason: "discord_server_error",
            provider_delay: None,
        }
    );
    assert_eq!(
        classify_http_failure(403, 0),
        AttemptOutcome::Pause {
            reason: "Announcement delivery lacks permission to use the configured channel.",
        }
    );
    assert_eq!(
        classify_http_failure(404, 10_003),
        AttemptOutcome::Pause {
            reason: "The configured announcement channel is unavailable.",
        }
    );
    assert_eq!(
        classify_http_failure(400, 50_035),
        AttemptOutcome::Pause {
            reason: "Discord rejected the announcement payload; reconfigure the destination or contact an operator.",
        }
    );
    assert_eq!(
        classify_http_failure(401, 0),
        AttemptOutcome::Pause {
            reason: "Discord authentication failed; an operator must correct the bot credentials.",
        }
    );
}
