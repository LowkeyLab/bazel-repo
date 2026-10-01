use book_smartz_storage::{OperationContext, Outcome};
use std::sync::Arc;
#[path = "../wire.rs"]
mod wire;
use super::support::{Fixture, Recorder, recorder};
use googletest::{assert_that, matchers::eq};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;
use wire::{parse_sequence_key, sequence_key};

#[googletest::test]
#[tokio::test]
async fn migrations_are_isolated_and_repeatable() {
    let fixture = Fixture::new().await;
    sqlx::query(
        "CREATE TABLE public._sqlx_migrations (version BIGINT PRIMARY KEY, sentinel TEXT NOT NULL)",
    )
    .execute(fixture.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO public._sqlx_migrations VALUES (99, 'other-app')")
        .execute(fixture.pool())
        .await
        .unwrap();
    let observations = Arc::new(Recorder::default());
    let store = fixture.store(observations.clone());
    store.migrate(OperationContext::default()).await.unwrap();
    store.migrate(OperationContext::default()).await.unwrap();
    let sentinel: String =
        sqlx::query_scalar("SELECT sentinel FROM public._sqlx_migrations WHERE version = 99")
            .fetch_one(fixture.pool())
            .await
            .unwrap();
    assert_that!(sentinel, eq("other-app"));
    let version: i64 =
        sqlx::query_scalar("SELECT version FROM book_smartz._sqlx_migrations WHERE success")
            .fetch_one(fixture.pool())
            .await
            .unwrap();
    assert_that!(version, eq(1));
    let observed = observations.0.lock().unwrap().clone();
    assert_that!(observed.len(), eq(2));
    assert_that!(
        observed
            .iter()
            .all(|entry| entry.outcome == Outcome::Applied),
        eq(true)
    );
    for _ in 0..3 {
        let path: String = sqlx::query_scalar("SHOW search_path")
            .fetch_one(fixture.pool())
            .await
            .unwrap();
        assert_that!(path, eq("\"$user\", public"));
    }
}

#[googletest::test]
#[tokio::test]
async fn concurrent_migrations_are_safe() {
    let fixture = Fixture::new().await;
    let a = fixture.store(recorder());
    let b = fixture.store(recorder());
    let (first, second) = tokio::join!(
        a.migrate(OperationContext::default()),
        b.migrate(OperationContext::default())
    );
    first.unwrap();
    second.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM book_smartz._sqlx_migrations WHERE success")
            .fetch_one(fixture.pool())
            .await
            .unwrap();
    assert_that!(count, eq(1));
}

async fn insert_event(
    pool: &sqlx::PgPool,
    envelope: &Value,
    sequence: &str,
    reader: Uuid,
    event: Uuid,
    candidate: Uuid,
    opponent: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO book_smartz.ranking_events (reader_id, sequence, event_id, candidate_book_id, opponent_book_id, envelope) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(reader).bind(sequence).bind(event).bind(candidate).bind(opponent).bind(envelope).execute(pool).await.map(|_| ())
}

#[googletest::test]
#[tokio::test]
async fn sequence_range_and_required_metadata_are_enforced() {
    assert_that!(sequence_key(u64::MAX).unwrap(), eq("18446744073709551615"));
    for bad in [
        "",
        "0",
        "00000000000000000000",
        "1",
        "0000000000000000000x",
        "18446744073709551616",
    ] {
        assert_that!(parse_sequence_key(bad).is_err(), eq(true));
    }
    let fixture = Fixture::new().await;
    fixture
        .store(recorder())
        .migrate(OperationContext::default())
        .await
        .unwrap();
    let reader = Uuid::from_u128(1);
    let candidate = Uuid::from_u128(2);
    let opponent = Uuid::from_u128(3);
    let event = Uuid::from_u128(4);
    sqlx::query("INSERT INTO book_smartz.books (id, work_id) VALUES ($1, 'OL1W'), ($2, 'OL2W')")
        .bind(candidate)
        .bind(opponent)
        .execute(fixture.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO book_smartz.reader_streams (reader_id) VALUES ($1)")
        .bind(reader)
        .execute(fixture.pool())
        .await
        .unwrap();
    let valid = json!({"specversion":"1.0", "id":event.to_string(), "source":format!("urn:uuid:{reader}"), "type":"bookranking.comparison.answered.v1", "subject":format!("books/{candidate}"), "time":"2026-10-01T12:00:00Z", "datacontenttype":"application/json", "sequence":"00000000000000000001", "data":{"readerId":reader.to_string(), "candidateBookId":candidate.to_string(), "opponentBookId":opponent.to_string(), "sequence":"1", "choice":"skip"}});
    insert_event(
        fixture.pool(),
        &valid,
        "00000000000000000001",
        reader,
        event,
        candidate,
        Some(opponent),
    )
    .await
    .unwrap();
    let mut baseline = valid.clone();
    baseline["id"] = json!(Uuid::from_u128(5).to_string());
    baseline["sequence"] = json!("00000000000000000002");
    baseline["data"]["sequence"] = json!("2");
    for changed in [
        json!({"id": null}),
        json!({"source": null}),
        json!({"type": null}),
        json!({"subject": null}),
        json!({"time": null}),
        json!({"sequence": null}),
        json!({"data": null}),
        json!({"data":{"readerId":null}}),
        json!({"data":{"candidateBookId":null}}),
        json!({"data":{"sequence":null}}),
        json!({"data":{"opponentBookId":null}}),
        json!({"data":{"choice":null}}),
    ] {
        let mut bad = baseline.clone();
        for (key, value) in changed.as_object().unwrap() {
            if key == "data" && value.is_object() {
                for (nested, inner) in value.as_object().unwrap() {
                    bad["data"][nested] = inner.clone();
                }
            } else {
                bad[key] = value.clone();
            }
        }
        let error = insert_event(
            fixture.pool(),
            &bad,
            "00000000000000000002",
            reader,
            Uuid::from_u128(5),
            candidate,
            Some(opponent),
        )
        .await
        .unwrap_err();
        assert_that!(
            error
                .as_database_error()
                .and_then(|error| error.constraint()),
            eq(Some("valid_envelope"))
        );
    }
    for absent in [
        "specversion",
        "id",
        "source",
        "type",
        "subject",
        "time",
        "sequence",
        "data",
    ] {
        let mut bad = baseline.clone();
        bad.as_object_mut().unwrap().remove(absent);
        let error = insert_event(
            fixture.pool(),
            &bad,
            "00000000000000000002",
            reader,
            Uuid::from_u128(5),
            candidate,
            Some(opponent),
        )
        .await
        .unwrap_err();
        assert_that!(
            error
                .as_database_error()
                .and_then(|error| error.constraint()),
            eq(Some("valid_envelope"))
        );
    }
    for absent in [
        "readerId",
        "candidateBookId",
        "opponentBookId",
        "choice",
        "sequence",
    ] {
        let mut bad = baseline.clone();
        bad["data"].as_object_mut().unwrap().remove(absent);
        let error = insert_event(
            fixture.pool(),
            &bad,
            "00000000000000000002",
            reader,
            Uuid::from_u128(5),
            candidate,
            Some(opponent),
        )
        .await
        .unwrap_err();
        assert_that!(
            error
                .as_database_error()
                .and_then(|error| error.constraint()),
            eq(Some("valid_envelope"))
        );
    }
    sqlx::query("INSERT INTO book_smartz.reader_streams (reader_id) VALUES ($1)")
        .bind(Uuid::from_u128(9))
        .execute(fixture.pool())
        .await
        .unwrap();
    for (bad_sequence, bad_reader, bad_event, bad_candidate, bad_opponent, expected_constraint) in [
        (
            "00000000000000000003",
            reader,
            Uuid::from_u128(5),
            candidate,
            Some(opponent),
            "valid_envelope",
        ),
        (
            "00000000000000000002",
            Uuid::from_u128(9),
            Uuid::from_u128(5),
            candidate,
            Some(opponent),
            "valid_envelope",
        ),
        (
            "00000000000000000002",
            reader,
            Uuid::from_u128(9),
            candidate,
            Some(opponent),
            "valid_envelope",
        ),
        (
            "00000000000000000002",
            reader,
            Uuid::from_u128(5),
            opponent,
            Some(opponent),
            "valid_envelope",
        ),
        (
            "00000000000000000002",
            reader,
            Uuid::from_u128(5),
            candidate,
            Some(candidate),
            "valid_envelope",
        ),
        (
            "00000000000000000002",
            reader,
            Uuid::from_u128(5),
            candidate,
            None,
            "valid_envelope",
        ),
    ] {
        let error = insert_event(
            fixture.pool(),
            &baseline,
            bad_sequence,
            bad_reader,
            bad_event,
            bad_candidate,
            bad_opponent,
        )
        .await
        .unwrap_err();
        assert_that!(
            error
                .as_database_error()
                .and_then(|error| error.constraint()),
            eq(Some(expected_constraint))
        );
    }
    let mut oversized = baseline.clone();
    oversized["sequence"] = json!("18446744073709551616");
    oversized["data"]["sequence"] = json!("18446744073709551616");
    let error = insert_event(
        fixture.pool(),
        &oversized,
        "18446744073709551616",
        reader,
        Uuid::from_u128(5),
        candidate,
        Some(opponent),
    )
    .await
    .unwrap_err();
    assert_that!(
        error
            .as_database_error()
            .and_then(|error| error.constraint()),
        eq(Some("valid_sequence"))
    );
    let mut non_comparison = baseline.clone();
    non_comparison["type"] = json!("bookranking.placement.started.v1");
    non_comparison["data"]
        .as_object_mut()
        .unwrap()
        .remove("opponentBookId");
    non_comparison["data"]
        .as_object_mut()
        .unwrap()
        .remove("choice");
    insert_event(
        fixture.pool(),
        &non_comparison,
        "00000000000000000002",
        reader,
        Uuid::from_u128(5),
        candidate,
        None,
    )
    .await
    .unwrap();
    let rows = sqlx::query(
        "SELECT sequence FROM book_smartz.ranking_events WHERE reader_id = $1 ORDER BY sequence",
    )
    .bind(reader)
    .fetch_all(fixture.pool())
    .await
    .unwrap();
    assert_that!(
        rows.iter()
            .map(|row| row.get::<String, _>("sequence"))
            .collect::<Vec<_>>(),
        eq(&vec!["00000000000000000001", "00000000000000000002"])
    );
}
