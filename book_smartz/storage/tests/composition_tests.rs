use super::support::{Fixture, recorder};
use book_smartz_domain::{
    Book, BookId, Command, CommandContext, EventId, OpenLibraryWorkId, ReaderId, decode_event,
    encode_event,
};
use book_smartz_storage::{OperationContext, Store};
use googletest::{assert_that, matchers::eq};
use serde_json::json;
use uuid::Uuid;

#[googletest::test]
#[tokio::test]
async fn composed_ranking_preserves_complete_envelope_after_reconnect() {
    let fixture = Fixture::new().await;
    let store = fixture.store(recorder());
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    let reader = ReaderId::new(Uuid::new_v4());
    let book = Book::new(
        BookId::new(Uuid::new_v4()),
        OpenLibraryWorkId::try_from("OL1W").unwrap(),
    );
    store.register_book(&book, op).await.unwrap();
    let mut transaction = store.begin_command(reader).await.unwrap();
    assert_that!(transaction.ranking_documents().len(), eq(0));
    let mut ranking = transaction.ranking().clone();
    let mut registry = book_smartz_domain::BookRegistry::new();
    registry.register(book.clone()).unwrap();
    let event = ranking
        .execute(
            &registry,
            CommandContext {
                expected_revision: 0,
                event_id: EventId::new(Uuid::new_v4()),
                time: chrono::DateTime::parse_from_rfc3339("2026-10-17T00:00:00Z")
                    .unwrap()
                    .to_utc(),
            },
            Command::Start { book_id: book.id() },
        )
        .unwrap();
    let mut envelope: serde_json::Value =
        serde_json::from_str(&encode_event(&event).unwrap()).unwrap();
    envelope["correlationid"] = json!("frozen-action");
    envelope["causationid"] = json!("recorded-intent");
    envelope["dataschema"] = json!("https://example.test/ranking/v1");
    let document = decode_event(&envelope.to_string()).unwrap();
    transaction.append_ranking(&document, 0).await.unwrap();
    assert_that!(
        transaction.ranking_documents(),
        eq(std::slice::from_ref(&document))
    );
    assert_that!(
        store
            .load_ranking(reader, op)
            .await
            .unwrap()
            .history()
            .len(),
        eq(0)
    );
    transaction.commit().await.unwrap();
    let reconnected = Store::new(fixture.pool().clone(), recorder());
    let documents = reconnected.load_ranking_documents(reader).await.unwrap();
    assert_that!(documents.len(), eq(1));
    assert_that!(&documents[0], eq(&document));
    assert_that!(
        reconnected
            .load_ranking(reader, op)
            .await
            .unwrap()
            .history(),
        eq(&[event])
    );
}

fn application_event(
    reader: ReaderId,
    sequence: u64,
    id: Uuid,
) -> book_smartz_storage::ApplicationEvent {
    book_smartz_storage::ApplicationEvent::decode(
        reader,
        &json!({
            "specversion": "1.0", "id": id.to_string(),
            "source": format!("urn:book-smartz:application:{}", reader.as_uuid()),
            "type": "booksmartz.action.intent.v1", "time": "2026-10-17T00:00:00Z",
            "datacontenttype": "application/json", "sequence": format!("{sequence:020}"),
            "correlationid": "original-action", "data": {"command": "Start", "expectedRevision": 0}
        })
        .to_string(),
    )
    .unwrap()
}

#[googletest::test]
#[tokio::test]
async fn immutable_application_facts_survive_restart_without_advancing_ranking() {
    let fixture = Fixture::new().await;
    let store = fixture.store(recorder());
    store.migrate(OperationContext::default()).await.unwrap();
    let reader = ReaderId::new(Uuid::new_v4());
    let intent = application_event(reader, 1, Uuid::new_v4());
    let mut transaction = store.begin_command(reader).await.unwrap();
    assert_that!(transaction.application_history().len(), eq(0));
    transaction.append_application(&intent).await.unwrap();
    assert_that!(
        transaction.application_history(),
        eq(std::slice::from_ref(&intent))
    );
    assert_that!(
        store.load_application_history(reader).await.unwrap().len(),
        eq(0)
    );
    transaction.commit().await.unwrap();
    let store = Store::new(fixture.pool().clone(), recorder());
    assert_that!(
        store.load_application_history(reader).await.unwrap(),
        eq(&vec![intent.clone()])
    );
    assert_that!(
        store
            .load_ranking(reader, OperationContext::default())
            .await
            .unwrap()
            .projection()
            .revision(),
        eq(0)
    );
    let mut transaction = store.begin_command(reader).await.unwrap();
    assert_that!(
        transaction.application_history(),
        eq(std::slice::from_ref(&intent))
    );
    assert_that!(
        transaction.append_application(&intent).await.is_err(),
        eq(true)
    );
    assert_that!(transaction.commit().await.is_err(), eq(true));
    assert_that!(
        store.load_application_history(reader).await.unwrap(),
        eq(&vec![intent])
    );
}

fn first_document(
    reader: ReaderId,
    book: &Book,
    id: Uuid,
) -> book_smartz_domain::CloudEventDocument {
    let event = book_smartz_domain::RankingEvent::new(
        book_smartz_domain::EventMetadata {
            id: EventId::new(id),
            reader_id: reader,
            sequence: book_smartz_domain::Sequence::new(1).unwrap(),
            time: chrono::DateTime::parse_from_rfc3339("2026-10-17T00:00:00Z")
                .unwrap()
                .to_utc(),
        },
        book.id(),
        book_smartz_domain::EventKind::PlacementStarted,
    );
    decode_event(&encode_event(&event).unwrap()).unwrap()
}

#[googletest::test]
#[tokio::test]
async fn ranking_and_application_facts_commit_or_roll_back_together() {
    let fixture = Fixture::new().await;
    let store = fixture.store(recorder());
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    let reader = ReaderId::new(Uuid::new_v4());
    let book = Book::new(
        BookId::new(Uuid::new_v4()),
        OpenLibraryWorkId::try_from("OL1W").unwrap(),
    );
    store.register_book(&book, op).await.unwrap();
    let id = Uuid::new_v4();
    // Identical UUIDs have distinct application/ranking sources.
    let intent = application_event(reader, 1, id);
    let document = first_document(reader, &book, id);
    let mut transaction = store.begin_command(reader).await.unwrap();
    transaction.append_application(&intent).await.unwrap();
    transaction.append_ranking(&document, 0).await.unwrap();
    drop(transaction);
    // Reacquiring the same stream synchronizes with the asynchronous rollback.
    let mut transaction = store.begin_command(reader).await.unwrap();
    assert_that!(transaction.ranking().projection().revision(), eq(0));
    assert_that!(
        store.load_application_history(reader).await.unwrap().len(),
        eq(0)
    );
    transaction.append_application(&intent).await.unwrap();
    transaction.append_ranking(&document, 0).await.unwrap();
    transaction.commit().await.unwrap();
    assert_that!(
        store.load_ranking_documents(reader).await.unwrap(),
        eq(&vec![document])
    );
    assert_that!(
        store.load_application_history(reader).await.unwrap(),
        eq(&vec![intent])
    );
}

#[googletest::test]
fn malformed_application_context_is_rejected_before_storage() {
    let reader = ReaderId::new(Uuid::new_v4());
    let valid = application_event(reader, 1, Uuid::new_v4());
    for (field, value) in [
        ("traceparent", json!(42)),
        ("tracestate", json!("vendor=value")),
        ("type", json!("bad\nvalue")),
        ("correlationid", json!("bad\nvalue")),
        ("sequence", json!("18446744073709551616")),
        ("sequence", json!("00000000000000000000")),
        ("dataschema", json!("relative/path")),
    ] {
        let mut envelope = valid.envelope().clone();
        envelope[field] = value;
        assert_that!(
            book_smartz_storage::ApplicationEvent::decode(reader, &envelope.to_string()).is_err(),
            eq(true)
        );
    }
}

#[googletest::test]
#[tokio::test]
async fn composed_commit_ack_loss_and_cancellation_preserve_complete_evidence() {
    use super::commit_proxy::CommitProxy;
    use book_smartz_storage::StoreError;
    let fixture = Fixture::new().await;
    let direct = fixture.store(recorder());
    let op = OperationContext::default();
    direct.migrate(op).await.unwrap();
    let book = Book::new(
        BookId::new(Uuid::new_v4()),
        OpenLibraryWorkId::try_from("OL1W").unwrap(),
    );
    direct.register_book(&book, op).await.unwrap();
    for cancel in [false, true] {
        let reader = ReaderId::new(Uuid::new_v4());
        let intent = application_event(reader, 1, Uuid::new_v4());
        let mut envelope = serde_json::from_str::<serde_json::Value>(
            &book_smartz_domain::encode_document(&first_document(reader, &book, Uuid::new_v4()))
                .unwrap(),
        )
        .unwrap();
        envelope["correlationid"] = json!("original-action");
        envelope["causationid"] = intent.envelope()["id"].clone();
        let document = decode_event(&envelope.to_string()).unwrap();
        let proxy = CommitProxy::new(fixture.pool()).await;
        let store = Store::new(proxy.pool.clone(), recorder());
        let executing = tokio::spawn({
            let intent = intent.clone();
            let document = document.clone();
            async move {
                let mut transaction = store.begin_command(reader).await?;
                transaction.append_application(&intent).await?;
                transaction.append_ranking(&document, 0).await?;
                transaction.commit().await
            }
        });
        proxy.commit_reached().await;
        // A real COMMIT has finished, but its acknowledgement is still withheld.
        assert_that!(
            direct.load_application_history(reader).await.unwrap(),
            eq(&vec![intent.clone()])
        );
        assert_that!(
            direct.load_ranking_documents(reader).await.unwrap(),
            eq(&vec![document.clone()])
        );
        if cancel {
            executing.abort();
            assert_that!(executing.await.unwrap_err().is_cancelled(), eq(true));
            proxy.disconnect();
        } else {
            proxy.disconnect();
            assert_that!(
                matches!(
                    executing.await.unwrap(),
                    Err(StoreError::CommitUncertain(_))
                ),
                eq(true)
            );
        }
        let mut retry = direct.begin_command(reader).await.unwrap();
        assert_that!(
            matches!(
                retry.append_ranking(&document, 0).await,
                Err(StoreError::Domain(
                    book_smartz_domain::DomainError::StaleRevision { actual: 1, .. }
                ))
            ),
            eq(true)
        );
        assert_that!(retry.commit().await.is_err(), eq(true));
        assert_that!(
            direct.load_ranking_documents(reader).await.unwrap(),
            eq(&vec![document])
        );
        assert_that!(
            direct.load_application_history(reader).await.unwrap(),
            eq(&vec![intent])
        );
    }
}

#[googletest::test]
#[tokio::test]
async fn composition_and_existing_command_writers_share_revision_serialization() {
    use book_smartz_storage::StoreError;
    use std::sync::Arc;
    use tokio::sync::Barrier;
    let fixture = Fixture::new().await;
    let store = fixture.store(recorder());
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    let book = Book::new(
        BookId::new(Uuid::new_v4()),
        OpenLibraryWorkId::try_from("OL1W").unwrap(),
    );
    store.register_book(&book, op).await.unwrap();
    let reader = ReaderId::new(Uuid::new_v4());
    let document = first_document(reader, &book, Uuid::new_v4());
    let intent = application_event(reader, 1, Uuid::new_v4());
    let barrier = Arc::new(Barrier::new(2));
    let composed = tokio::spawn({
        let store = store.clone();
        let barrier = barrier.clone();
        async move {
            barrier.wait().await;
            let mut transaction = store.begin_command(reader).await?;
            transaction.append_application(&intent).await?;
            transaction.append_ranking(&document, 0).await?;
            transaction.commit().await
        }
    });
    barrier.wait().await;
    let legacy = store
        .execute(
            reader,
            CommandContext {
                expected_revision: 0,
                event_id: EventId::new(Uuid::new_v4()),
                time: chrono::DateTime::parse_from_rfc3339("2026-10-17T00:00:00Z")
                    .unwrap()
                    .to_utc(),
            },
            Command::Start { book_id: book.id() },
            op,
        )
        .await;
    let composed = composed.await.unwrap();
    let application_count = usize::from(composed.is_ok());
    let results = [composed, legacy.map(|_| ())];
    assert_that!(
        results.iter().filter(|result| result.is_ok()).count(),
        eq(1)
    );
    assert_that!(
        results
            .iter()
            .filter(|result| matches!(
                result,
                Err(StoreError::Domain(
                    book_smartz_domain::DomainError::StaleRevision { actual: 1, .. }
                ))
            ))
            .count(),
        eq(1)
    );
    assert_that!(
        store.load_ranking_documents(reader).await.unwrap().len(),
        eq(1)
    );
    assert_that!(
        store.load_application_history(reader).await.unwrap().len(),
        eq(application_count)
    );
}

#[googletest::test]
#[tokio::test]
async fn failed_or_repeated_appends_cannot_commit_an_earlier_staged_decision() {
    let fixture = Fixture::new().await;
    let store = fixture.store(recorder());
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    let book = Book::new(
        BookId::new(Uuid::new_v4()),
        OpenLibraryWorkId::try_from("OL1W").unwrap(),
    );
    store.register_book(&book, op).await.unwrap();
    let reader = ReaderId::new(Uuid::new_v4());
    let id = Uuid::new_v4();
    let intent = application_event(reader, 1, id);
    let mut transaction = store.begin_command(reader).await.unwrap();
    transaction.append_application(&intent).await.unwrap();
    transaction.commit().await.unwrap();
    let document = first_document(reader, &book, Uuid::new_v4());
    let mut transaction = store.begin_command(reader).await.unwrap();
    transaction.append_ranking(&document, 0).await.unwrap();
    // Next sequence with the same source/ID must fail database uniqueness.
    assert_that!(
        matches!(
            transaction
                .append_application(&application_event(reader, 2, id))
                .await,
            Err(book_smartz_storage::StoreError::Database(_))
        ),
        eq(true)
    );
    assert_that!(transaction.commit().await.is_err(), eq(true));
    let mut transaction = store.begin_command(reader).await.unwrap();
    assert_that!(transaction.ranking().projection().revision(), eq(0));
    transaction.append_ranking(&document, 0).await.unwrap();
    assert_that!(
        transaction.append_ranking(&document, 1).await.is_err(),
        eq(true)
    );
    assert_that!(transaction.commit().await.is_err(), eq(true));
    // Reacquire before inspecting to synchronize with the rollback.
    let transaction = store.begin_command(reader).await.unwrap();
    assert_that!(transaction.ranking().projection().revision(), eq(0));
    drop(transaction);
    assert_that!(
        store.load_ranking_documents(reader).await.unwrap().len(),
        eq(0)
    );
    assert_that!(
        store.load_application_history(reader).await.unwrap(),
        eq(&vec![intent])
    );
    assert_that!(
        store.book_by_id(book.id(), op).await.unwrap(),
        eq(&Some(book))
    );
}

#[googletest::test]
#[tokio::test]
async fn application_history_rejects_corrupt_indexes_and_gaps() {
    use book_smartz_storage::StoreError;
    let fixture = Fixture::new().await;
    let store = fixture.store(recorder());
    store.migrate(OperationContext::default()).await.unwrap();
    let reader = ReaderId::new(Uuid::new_v4());
    let id = Uuid::new_v4();
    let event = application_event(reader, 1, id);
    let mut transaction = store.begin_command(reader).await.unwrap();
    transaction.append_application(&event).await.unwrap();
    transaction.commit().await.unwrap();
    let invalid_index =
        sqlx::query("UPDATE book_smartz.application_events SET event_id = $1 WHERE reader_id = $2")
            .bind(Uuid::new_v4())
            .bind(reader.as_uuid())
            .execute(fixture.pool())
            .await;
    assert_that!(invalid_index.is_err(), eq(true));
    // Privileged fixture access simulates corruption beyond normal constraints.
    sqlx::query(
        "ALTER TABLE book_smartz.application_events DROP CONSTRAINT valid_application_envelope",
    )
    .execute(fixture.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE book_smartz.application_events SET event_id = $1 WHERE reader_id = $2")
        .bind(Uuid::new_v4())
        .bind(reader.as_uuid())
        .execute(fixture.pool())
        .await
        .unwrap();
    assert_that!(
        matches!(
            store.load_application_history(reader).await,
            Err(StoreError::CorruptHistory(_))
        ),
        eq(true)
    );
    let gapped = application_event(reader, 2, id);
    sqlx::query("UPDATE book_smartz.application_events SET event_id = $1, sequence = '00000000000000000002', envelope = $2 WHERE reader_id = $3")
        .bind(id).bind(gapped.envelope()).bind(reader.as_uuid()).execute(fixture.pool()).await.unwrap();
    assert_that!(
        matches!(
            store.load_application_history(reader).await,
            Err(StoreError::CorruptHistory(_))
        ),
        eq(true)
    );
    assert_that!(
        matches!(
            store.begin_command(reader).await,
            Err(StoreError::CorruptHistory(_))
        ),
        eq(true)
    );
}
