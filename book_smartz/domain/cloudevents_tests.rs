use chrono::{DateTime, Utc};
use googletest::prelude::*;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    AutomaticPauseReason, BookId, ComparisonChoice, DomainNotice, EventId, EventKind,
    EventMetadata, RankingEvent, RankingProjection, ReaderId, Sequence, decode_event,
    derive_notices, encode_document, encode_event, encode_notice,
};

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn time() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-27T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn event(sequence: u64, kind: EventKind) -> RankingEvent {
    RankingEvent::new(
        EventMetadata {
            id: EventId::new(id(sequence.into())),
            reader_id: ReaderId::new(id(100)),
            sequence: Sequence::new(sequence).unwrap(),
            time: time(),
        },
        BookId::new(id(200)),
        kind,
    )
}

fn sample() -> Value {
    json!({
        "specversion": "1.0",
        "id": "e83ecc4f-68bf-4b24-9a63-60cc7cfad460",
        "source": "urn:uuid:b3692ae0-c782-4b0e-9335-4b2bdc7048d5",
        "type": "bookranking.comparison.answered.v1",
        "subject": "books/4b3d5226-5df7-4c1d-935e-bc42a3434868",
        "time": "2026-09-27T12:00:00Z",
        "datacontenttype": "application/json",
        "data": {
            "readerId": "b3692ae0-c782-4b0e-9335-4b2bdc7048d5",
            "candidateBookId": "4b3d5226-5df7-4c1d-935e-bc42a3434868",
            "sequence": "3",
            "opponentBookId": "286e3364-b260-4f4f-8ef4-8074d3eb52ba",
            "choice": "prefer_candidate"
        }
    })
}

#[googletest::test]
fn cloudevent_roundtrip_preserves_each_event_kind() {
    let kinds = [
        (
            EventKind::PlacementStarted,
            "bookranking.placement.started.v1",
        ),
        (
            EventKind::ComparisonAnswered {
                opponent: BookId::new(id(300)),
                choice: ComparisonChoice::PreferCandidate,
            },
            "bookranking.comparison.answered.v1",
        ),
        (
            EventKind::PlacementPaused,
            "bookranking.placement.paused.v1",
        ),
        (
            EventKind::PlacementResumed,
            "bookranking.placement.resumed.v1",
        ),
    ];
    for (kind, expected_type) in kinds {
        let original = event(3, kind);
        let encoded: Value = serde_json::from_str(&encode_event(&original).unwrap()).unwrap();
        assert_that!(encoded["type"].as_str(), eq(Some(expected_type)));
        assert_that!(
            encoded["source"].as_str(),
            eq(Some("urn:uuid:00000000-0000-0000-0000-000000000064"))
        );
        assert_that!(
            encoded["subject"].as_str(),
            eq(Some("books/00000000-0000-0000-0000-0000000000c8"))
        );
        assert_that!(encoded["time"].as_str(), eq(Some("2026-09-27T12:00:00Z")));
        assert_that!(encoded["data"]["sequence"].as_str(), eq(Some("3")));
        assert_that!(
            decode_event(&encoded.to_string()).unwrap().event(),
            eq(&original)
        );
    }
    for choice in [
        ComparisonChoice::PreferCandidate,
        ComparisonChoice::PreferOpponent,
        ComparisonChoice::Skip,
    ] {
        let encoded: Value = serde_json::from_str(
            &encode_event(&event(
                4,
                EventKind::ComparisonAnswered {
                    opponent: BookId::new(id(300)),
                    choice,
                },
            ))
            .unwrap(),
        )
        .unwrap();
        let expected = match choice {
            ComparisonChoice::PreferCandidate => "prefer_candidate",
            ComparisonChoice::PreferOpponent => "prefer_opponent",
            ComparisonChoice::Skip => "skip",
        };
        assert_that!(encoded["data"]["choice"].as_str(), eq(Some(expected)));
    }
    let decoded = decode_event(&sample().to_string()).unwrap();
    assert_that!(decoded.event().metadata().sequence.value(), eq(3));
    assert_that!(
        decoded.event().kind(),
        eq(&EventKind::ComparisonAnswered {
            opponent: BookId::new(Uuid::parse_str("286e3364-b260-4f4f-8ef4-8074d3eb52ba").unwrap()),
            choice: ComparisonChoice::PreferCandidate
        })
    );
}

#[googletest::test]
fn invalid_envelopes_are_rejected() {
    let original = sample();
    for (field, replacement) in [
        ("specversion", json!("2.0")),
        ("id", json!("bad")),
        ("id", json!("")),
        ("source", json!("urn:uuid:bad")),
        ("subject", json!("books/bad")),
        ("type", json!("bookranking.comparison.answered.v2")),
        ("type", json!("bookranking.book.ranked.v1")),
        ("time", json!("not-a-time")),
        ("datacontenttype", json!("text/plain")),
        ("data", json!("{\"sequence\":\"3\"}")),
    ] {
        let mut invalid = original.clone();
        invalid[field] = replacement;
        assert_that!(decode_event(&invalid.to_string()).is_err(), eq(true));
    }
    for field in [
        "specversion",
        "id",
        "source",
        "type",
        "subject",
        "time",
        "datacontenttype",
        "data",
    ] {
        let mut invalid = original.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert_that!(decode_event(&invalid.to_string()).is_err(), eq(true));
    }
    let mut invalid = original.clone();
    invalid["source"] = json!("urn:uuid:00000000-0000-0000-0000-000000000001");
    assert_that!(decode_event(&invalid.to_string()).is_err(), eq(true));
    invalid = original.clone();
    invalid["subject"] = json!("books/00000000-0000-0000-0000-000000000001");
    assert_that!(decode_event(&invalid.to_string()).is_err(), eq(true));
    invalid = original.clone();
    invalid["data_base64"] = json!("e30=");
    assert_that!(decode_event(&invalid.to_string()).is_err(), eq(true));
    assert_that!(
        decode_event(&original.to_string().replacen(
            "\"specversion\":\"1.0\"",
            "\"specversion\":\"1.0\",\"specversion\":\"2.0\"",
            1
        ))
        .is_err(),
        eq(true)
    );
    assert_that!(
        decode_event(&original.to_string().replacen(
            "\"sequence\":\"3\"",
            "\"sequence\":\"3\",\"sequence\":\"4\"",
            1
        ))
        .is_err(),
        eq(true)
    );
}

#[googletest::test]
fn large_sequence_and_offset_time_roundtrip() {
    let mut wire = sample();
    wire["data"]["sequence"] = json!("18446744073709551615");
    wire["time"] = json!("2026-09-27T13:00:00+01:00");
    let document = decode_event(&wire.to_string()).unwrap();
    assert_that!(document.event().metadata().sequence.value(), eq(u64::MAX));
    let reencoded: Value = serde_json::from_str(&encode_document(&document).unwrap()).unwrap();
    assert_that!(
        reencoded["data"]["sequence"].as_str(),
        eq(Some("18446744073709551615"))
    );
    assert_that!(reencoded["time"].as_str(), eq(Some("2026-09-27T12:00:00Z")));
    for invalid_sequence in [
        json!(0),
        json!(3),
        json!("0"),
        json!("03"),
        json!("18446744073709551616"),
        json!("+3"),
    ] {
        wire["data"]["sequence"] = invalid_sequence;
        assert_that!(decode_event(&wire.to_string()).is_err(), eq(true));
    }
}

#[googletest::test]
fn extensions_survive_roundtrip() {
    let mut wire = sample();
    wire["trace"] = json!("abc");
    wire["retry"] = json!(true);
    wire["attempt"] = json!(2147483647);
    wire["1step"] = json!("valid");
    wire["data"]["futureField"] = json!({"added": true});
    let document = decode_event(&wire.to_string()).unwrap();
    let encoded: Value = serde_json::from_str(&encode_document(&document).unwrap()).unwrap();
    for key in ["trace", "retry", "attempt", "1step"] {
        assert_that!(&encoded[key], eq(&wire[key]));
    }
    for (name, value) in [
        ("Upper", json!("x")),
        ("array", json!([1])),
        ("object", json!({"x": 1})),
        ("large", json!(2147483648_i64)),
        ("control", json!("bad\u{007f}")),
    ] {
        let mut invalid = sample();
        invalid[name] = value;
        assert_that!(decode_event(&invalid.to_string()).is_err(), eq(true));
    }
    let mut missing = sample();
    missing["data"].as_object_mut().unwrap().remove("choice");
    assert_that!(decode_event(&missing.to_string()).is_err(), eq(true));
}

#[googletest::test]
fn derived_notices_are_not_replay_events() {
    let reader = ReaderId::new(id(100));
    let before = RankingProjection::replay(reader, &[]).unwrap();
    let first = event(1, EventKind::PlacementStarted);
    let after = before.apply(&first).unwrap();
    let notices = derive_notices(&before, &first, &after);
    assert_that!(notices.len(), eq(1));
    assert_that!(
        matches!(
            &notices[0],
            DomainNotice::BookRanked {
                position: 0,
                entry_count: 1,
                ..
            }
        ),
        eq(true)
    );
    let encoded: Value = serde_json::from_str(&encode_notice(&notices[0]).unwrap()).unwrap();
    assert_that!(
        encoded["type"].as_str(),
        eq(Some("bookranking.book.ranked.v1"))
    );
    assert_that!(
        encoded["id"].as_str(),
        eq(Some("00000000-0000-0000-0000-000000000001:bookranked"))
    );
    assert_that!(
        encoded["data"]["causationId"].as_str(),
        eq(Some("00000000-0000-0000-0000-000000000001"))
    );
    assert_that!(encoded["data"]["position"].as_u64(), eq(Some(0)));
    assert_that!(encoded["data"]["entryCount"].as_u64(), eq(Some(1)));
    assert_that!(
        encoded["source"].as_str(),
        eq(Some("urn:uuid:00000000-0000-0000-0000-000000000064"))
    );
    assert_that!(
        encoded["subject"].as_str(),
        eq(Some("books/00000000-0000-0000-0000-0000000000c8"))
    );
    assert_that!(encoded["time"].as_str(), eq(Some("2026-09-27T12:00:00Z")));
    assert_that!(encoded["data"]["sequence"].as_str(), eq(Some("1")));
    assert_that!(decode_event(&encoded.to_string()).is_err(), eq(true));

    let second = RankingEvent::new(
        EventMetadata {
            id: EventId::new(id(2)),
            reader_id: reader,
            sequence: Sequence::new(2).unwrap(),
            time: time(),
        },
        BookId::new(id(300)),
        EventKind::PlacementStarted,
    );
    let pending = after.apply(&second).unwrap();
    let skip = RankingEvent::new(
        EventMetadata {
            id: EventId::new(id(3)),
            reader_id: reader,
            sequence: Sequence::new(3).unwrap(),
            time: time(),
        },
        BookId::new(id(300)),
        EventKind::ComparisonAnswered {
            opponent: BookId::new(id(200)),
            choice: ComparisonChoice::Skip,
        },
    );
    let paused = pending.apply(&skip).unwrap();
    let notices = derive_notices(&pending, &skip, &paused);
    assert_that!(notices.len(), eq(1));
    assert_that!(
        matches!(
            &notices[0],
            DomainNotice::PlacementAutomaticallyPaused {
                reason: AutomaticPauseReason::NoEligibleOpponents,
                ..
            }
        ),
        eq(true)
    );
    let encoded: Value = serde_json::from_str(&encode_notice(&notices[0]).unwrap()).unwrap();
    assert_that!(
        encoded["type"].as_str(),
        eq(Some("bookranking.placement.automaticallypaused.v1"))
    );
    assert_that!(
        encoded["id"].as_str(),
        eq(Some(
            "00000000-0000-0000-0000-000000000003:automaticallypaused"
        ))
    );
    assert_that!(
        encoded["data"]["reason"].as_str(),
        eq(Some("no_eligible_opponents"))
    );
    assert_that!(decode_event(&encoded.to_string()).is_err(), eq(true));
    let resumed = RankingEvent::new(
        EventMetadata {
            id: EventId::new(id(4)),
            reader_id: reader,
            sequence: Sequence::new(4).unwrap(),
            time: time(),
        },
        BookId::new(id(300)),
        EventKind::PlacementResumed,
    );
    let active = paused.apply(&resumed).unwrap();
    let explicit = RankingEvent::new(
        EventMetadata {
            id: EventId::new(id(5)),
            reader_id: reader,
            sequence: Sequence::new(5).unwrap(),
            time: time(),
        },
        BookId::new(id(300)),
        EventKind::PlacementPaused,
    );
    let explicit_paused = active.apply(&explicit).unwrap();
    assert_that!(
        derive_notices(&active, &explicit, &explicit_paused).len(),
        eq(0)
    );
}
