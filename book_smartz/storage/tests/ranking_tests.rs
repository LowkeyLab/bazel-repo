use std::sync::Arc;

use book_smartz_domain::{
    Book, BookId, Command, CommandContext, ComparisonChoice, DomainError, EventId,
    OpenLibraryWorkId, ReaderId, ReplayErrorReason,
};
use book_smartz_storage::{Operation, OperationContext, Outcome, Store, StoreError};
use googletest::{assert_that, matchers::eq};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::Barrier;
use uuid::Uuid;

use super::support::{Fixture, Recorder};

fn book(id: u128) -> Book {
    Book::new(
        BookId::new(Uuid::from_u128(id)),
        OpenLibraryWorkId::try_from(format!("OL{id}W").as_str()).unwrap(),
    )
}

fn reader(id: u128) -> ReaderId {
    ReaderId::new(Uuid::from_u128(id))
}

fn context(revision: u64, event: u128, second: u32) -> CommandContext {
    CommandContext {
        expected_revision: revision,
        event_id: EventId::new(Uuid::from_u128(event)),
        time: chrono::DateTime::parse_from_rfc3339(&format!("2026-09-27T12:00:{second:02}Z"))
            .unwrap()
            .to_utc(),
    }
}

async fn registered(store: &Store, books: &[Book]) {
    let operation = OperationContext::default();
    for book in books {
        store.register_book(book, operation).await.unwrap();
    }
}

#[googletest::test]
#[tokio::test]
async fn reconnect_recovers_ranking_and_pending_placement() {
    let fixture = Fixture::new().await;
    let observations = Arc::new(Recorder::default());
    let store = fixture.store(observations.clone());
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    let [a, b, c] = [book(1), book(2), book(3)];
    registered(&store, &[a.clone(), b.clone(), c]).await;
    let who = reader(100);
    let first = store
        .execute(
            who,
            context(0, 11, 1),
            Command::Start { book_id: a.id() },
            op,
        )
        .await
        .unwrap();
    assert_that!(first.projection.revision(), eq(1));
    store
        .execute(
            who,
            context(1, 12, 2),
            Command::Start { book_id: b.id() },
            op,
        )
        .await
        .unwrap();
    store
        .execute(
            who,
            context(2, 13, 3),
            Command::Answer {
                opponent: a.id(),
                choice: ComparisonChoice::Skip,
            },
            op,
        )
        .await
        .unwrap();
    let before = store.load_ranking(who, op).await.unwrap();
    assert_that!(before.projection().revision(), eq(3));
    assert_that!(
        before
            .projection()
            .entries()
            .iter()
            .map(|entry| entry.book_id())
            .collect::<Vec<_>>(),
        eq(&vec![a.id()])
    );
    let pending = before.projection().pending().unwrap();
    assert_that!(pending.candidate(), eq(b.id()));
    assert_that!(pending.bounds(), eq((0, 1)));
    assert_that!(pending.is_paused(), eq(true));
    assert_that!(pending.started_at(), eq(context(1, 12, 2).time));
    assert_that!(pending.last_activity_at(), eq(context(2, 13, 3).time));
    assert_that!(
        before.projection().entries()[0].added_at(),
        eq(context(0, 11, 1).time)
    );
    assert_that!(before.history().len(), eq(3));
    let options = fixture.pool().connect_options().as_ref().clone();
    fixture.pool().close().await;
    let reopened = PgPoolOptions::new().connect_with(options).await.unwrap();
    let recovered_store = Store::new(reopened.clone(), observations);
    let recovered = recovered_store.load_ranking(who, op).await.unwrap();
    assert_that!(recovered, eq(&before));
    recovered_store
        .execute(who, context(3, 14, 4), Command::Resume, op)
        .await
        .unwrap();
    assert_that!(
        recovered_store
            .load_ranking(who, op)
            .await
            .unwrap()
            .projection()
            .next_opponent(),
        eq(Some(a.id()))
    );
    recovered_store
        .execute(
            who,
            context(4, 15, 5),
            Command::Answer {
                opponent: a.id(),
                choice: ComparisonChoice::PreferCandidate,
            },
            op,
        )
        .await
        .unwrap();
    let done = recovered_store.load_ranking(who, op).await.unwrap();
    assert_that!(
        done.projection()
            .entries()
            .iter()
            .map(|entry| entry.book_id())
            .collect::<Vec<_>>(),
        eq(&vec![b.id(), a.id()])
    );
    assert_that!(
        done.projection().entries()[0].added_at(),
        eq(context(4, 15, 5).time)
    );
    assert_that!(done.projection().pending().is_none(), eq(true));
    assert_that!(done.history().len(), eq(5));
    reopened.close().await;
}

async fn race(
    store: &Store,
    who: ReaderId,
    revision: u64,
    first: BookId,
    second: BookId,
    event: u128,
) {
    let barrier = Arc::new(Barrier::new(3));
    let left_store = store.clone();
    let right_store = store.clone();
    let left_barrier = barrier.clone();
    let right_barrier = barrier.clone();
    let left = tokio::spawn(async move {
        left_barrier.wait().await;
        left_store
            .execute(
                who,
                context(revision, event, 10),
                Command::Start { book_id: first },
                OperationContext::default(),
            )
            .await
    });
    let right = tokio::spawn(async move {
        right_barrier.wait().await;
        right_store
            .execute(
                who,
                context(revision, event + 1, 11),
                Command::Start { book_id: second },
                OperationContext::default(),
            )
            .await
    });
    barrier.wait().await;
    let results = [left.await.unwrap(), right.await.unwrap()];
    assert_that!(
        results.iter().filter(|result| result.is_ok()).count(),
        eq(1)
    );
    assert_that!(results.iter().filter(|result| matches!(result, Err(StoreError::Domain(DomainError::StaleRevision { expected, actual })) if expected == &revision && actual == &(revision + 1))).count(), eq(1));
}

#[googletest::test]
#[tokio::test]
async fn first_and_existing_stream_races_reject_stale_writers() {
    let fixture = Fixture::new().await;
    let store = fixture.store(Arc::new(Recorder::default()));
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    registered(&store, &[book(1), book(2), book(3)]).await;
    let first_reader = reader(101);
    race(&store, first_reader, 0, book(1).id(), book(2).id(), 21).await;
    assert_that!(
        store
            .load_ranking(first_reader, op)
            .await
            .unwrap()
            .history()
            .len(),
        eq(1)
    );
    let existing_reader = reader(102);
    store
        .execute(
            existing_reader,
            context(0, 30, 1),
            Command::Start {
                book_id: book(1).id(),
            },
            op,
        )
        .await
        .unwrap();
    race(&store, existing_reader, 1, book(2).id(), book(3).id(), 31).await;
    let history = store.load_ranking(existing_reader, op).await.unwrap();
    assert_that!(history.history().len(), eq(2));
    assert_that!(history.projection().revision(), eq(2));
}

#[googletest::test]
#[tokio::test]
async fn reader_streams_and_duplicate_event_rules_are_independent() {
    let fixture = Fixture::new().await;
    let observations = Arc::new(Recorder::default());
    let store = fixture.store(observations.clone());
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    registered(&store, &[book(1), book(2)]).await;
    let a = reader(103);
    let b = reader(104);
    let first = context(0, 41, 1);
    store
        .execute(
            a,
            first,
            Command::Start {
                book_id: book(1).id(),
            },
            op,
        )
        .await
        .unwrap();
    store
        .execute(
            b,
            first,
            Command::Start {
                book_id: book(1).id(),
            },
            op,
        )
        .await
        .unwrap();
    assert_that!(
        store
            .load_ranking(reader(105), op)
            .await
            .unwrap()
            .history()
            .len(),
        eq(0)
    );
    assert_that!(
        matches!(
            store
                .execute(
                    a,
                    context(1, 41, 2),
                    Command::Start {
                        book_id: book(2).id()
                    },
                    op
                )
                .await,
            Err(StoreError::Domain(DomainError::InvalidTransition(
                ReplayErrorReason::DuplicateEventId
            )))
        ),
        eq(true)
    );
    assert_that!(
        matches!(
            store
                .execute(
                    a,
                    first,
                    Command::Start {
                        book_id: book(2).id()
                    },
                    op
                )
                .await,
            Err(StoreError::Domain(DomainError::StaleRevision {
                expected: 0,
                actual: 1
            }))
        ),
        eq(true)
    );
    assert_that!(
        matches!(
            store
                .execute(a, context(1, 42, 3), Command::Resume, op)
                .await,
            Err(StoreError::Domain(DomainError::InvalidTransition(
                ReplayErrorReason::NoActivePlacement
            )))
        ),
        eq(true)
    );
    let history = store.load_ranking(a, op).await.unwrap();
    assert_that!(history.history().len(), eq(1));
    assert_that!(history.projection().revision(), eq(1));
    assert_that!(
        store.load_ranking(b, op).await.unwrap().history().len(),
        eq(1)
    );
    let recorded = observations.0.lock().unwrap();
    assert_that!(
        recorded
            .iter()
            .filter(|item| item.operation == Operation::RankingCommand
                && item.outcome == Outcome::Committed)
            .count(),
        eq(2)
    );
}

async fn insert_document(pool: &sqlx::PgPool, raw: &str) -> book_smartz_domain::RankingEvent {
    // Privileged historical fixture insertion validates the raw bytes before JSONB normalizes them.
    let document = book_smartz_domain::decode_event(raw).unwrap();
    let event = document.event().clone();
    let canonical = book_smartz_domain::encode_document(&document).unwrap();
    let envelope: serde_json::Value = serde_json::from_str(&canonical).unwrap();
    let opponent = match event.kind() {
        book_smartz_domain::EventKind::ComparisonAnswered { opponent, .. } => {
            Some(*opponent.as_uuid())
        }
        _ => None,
    };
    sqlx::query(
        "INSERT INTO book_smartz.reader_streams(reader_id) VALUES ($1) ON CONFLICT DO NOTHING",
    )
    .bind(event.metadata().reader_id.as_uuid())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO book_smartz.ranking_events (reader_id, sequence, event_id, candidate_book_id, opponent_book_id, envelope) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(event.metadata().reader_id.as_uuid())
        .bind(format!("{:020}", event.metadata().sequence.value()))
        .bind(event.metadata().id.as_uuid())
        .bind(event.candidate().as_uuid())
        .bind(opponent)
        .bind(sqlx::types::Json(envelope))
        .execute(pool)
        .await
        .unwrap();
    event
}

#[googletest::test]
#[tokio::test]
async fn historical_v1_history_remains_replayable() {
    let fixture = Fixture::new().await;
    let observations = Arc::new(Recorder::default());
    let store = fixture.store(observations.clone());
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    registered(&store, &[book(1), book(2)]).await;
    let raw: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/ranking-history-v1.json")).unwrap();
    assert_that!(raw[0].get("sequence").is_none(), eq(true));
    let expected = raw
        .iter()
        .map(|value| {
            let document = book_smartz_domain::decode_event(&value.to_string()).unwrap();
            document.event().clone()
        })
        .collect::<Vec<_>>();
    for value in &raw {
        insert_document(fixture.pool(), &value.to_string()).await;
    }
    observations.0.lock().unwrap().clear();
    let recovered = store.load_ranking(reader(200), op).await.unwrap();
    assert_that!(recovered.history(), eq(expected.as_slice()));
    assert_that!(recovered.projection().revision(), eq(3));
    assert_that!(
        recovered
            .projection()
            .entries()
            .iter()
            .map(|entry| entry.book_id())
            .collect::<Vec<_>>(),
        eq(&vec![book(2).id(), book(1).id()])
    );
    assert_that!(
        recovered.projection().entries()[0].added_at(),
        eq(context(2, 53, 3).time)
    );
    assert_that!(
        recovered.projection().entries()[1].added_at(),
        eq(context(0, 51, 1).time)
    );
    assert_that!(recovered.projection().pending().is_none(), eq(true));
    let stored: Vec<(sqlx::types::Json<serde_json::Value>,)> = sqlx::query_as(
        "SELECT envelope FROM book_smartz.ranking_events WHERE reader_id = $1 ORDER BY sequence",
    )
    .bind(reader(200).as_uuid())
    .fetch_all(fixture.pool())
    .await
    .unwrap();
    assert_that!(stored.len(), eq(3));
    assert_that!(
        stored[0].0.0["sequence"].as_str(),
        eq(Some("00000000000000000001"))
    );
    assert_that!(stored[1].0.0["tracehint"].as_str(), eq(Some("opaque-v1")));
    assert_that!(stored[1].0.0["retry"].as_bool(), eq(Some(true)));
    assert_that!(
        stored[1].0.0["traceparent"].as_str(),
        eq(Some(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        ))
    );
    let recorded = observations.0.lock().unwrap();
    assert_that!(recorded.len(), eq(1));
    assert_that!(recorded[0].operation, eq(Operation::RankingLoad));
    assert_that!(recorded[0].outcome, eq(Outcome::Loaded));
    assert_that!(recorded[0].event_count, eq(Some(3)));
}

fn event(
    reader: ReaderId,
    sequence: u64,
    id: u128,
    candidate: BookId,
    kind: book_smartz_domain::EventKind,
) -> book_smartz_domain::RankingEvent {
    book_smartz_domain::RankingEvent::new(
        book_smartz_domain::EventMetadata {
            id: EventId::new(Uuid::from_u128(id)),
            reader_id: reader,
            sequence: book_smartz_domain::Sequence::new(sequence).unwrap(),
            time: context(sequence - 1, id, sequence as u32).time,
        },
        candidate,
        kind,
    )
}

#[googletest::test]
#[tokio::test]
async fn corrupt_history_is_never_returned_as_partial_state() {
    let fixture = Fixture::new().await;
    let observations = Arc::new(Recorder::default());
    let store = fixture.store(observations.clone());
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    registered(&store, &[book(1)]).await;
    let gap_reader = reader(201);
    let gap = event(
        gap_reader,
        2,
        61,
        book(1).id(),
        book_smartz_domain::EventKind::PlacementStarted,
    );
    insert_document(
        fixture.pool(),
        &book_smartz_domain::encode_event(&gap).unwrap(),
    )
    .await;
    let invalid_reader = reader(202);
    let first = event(
        invalid_reader,
        1,
        62,
        book(1).id(),
        book_smartz_domain::EventKind::PlacementStarted,
    );
    let invalid = event(
        invalid_reader,
        2,
        63,
        book(1).id(),
        book_smartz_domain::EventKind::PlacementResumed,
    );
    insert_document(
        fixture.pool(),
        &book_smartz_domain::encode_event(&first).unwrap(),
    )
    .await;
    insert_document(
        fixture.pool(),
        &book_smartz_domain::encode_event(&invalid).unwrap(),
    )
    .await;
    observations.0.lock().unwrap().clear();
    for who in [gap_reader, invalid_reader] {
        assert_that!(
            matches!(
                store.load_ranking(who, op).await,
                Err(StoreError::CorruptHistory(_))
            ),
            eq(true)
        );
    }
    let recorded = observations.0.lock().unwrap();
    assert_that!(recorded.len(), eq(2));
    assert_that!(
        recorded
            .iter()
            .all(|item| item.operation == Operation::RankingLoad
                && item.outcome == Outcome::Failed(book_smartz_storage::Failure::CorruptHistory)
                && item.event_count.is_none()),
        eq(true)
    );
}
