use std::sync::Arc;

use book_smartz_domain::{
    Book, BookId, Command, CommandContext, DomainError, EventId, OpenLibraryWorkId, ReaderId,
};
use book_smartz_storage::{OperationContext, Outcome, Registration, StoreError};
use googletest::{assert_that, matchers::eq};
use uuid::Uuid;

use super::support::{Fixture, Recorder};

fn book(id: u128) -> Book {
    Book::new(
        BookId::new(Uuid::from_u128(id)),
        OpenLibraryWorkId::try_from(format!("OL{id}W").as_str()).unwrap(),
    )
}
fn context(revision: u64) -> CommandContext {
    CommandContext {
        expected_revision: revision,
        event_id: EventId::new(Uuid::new_v4()),
        time: chrono::DateTime::parse_from_rfc3339("2026-10-01T12:00:00Z")
            .unwrap()
            .to_utc(),
    }
}
async fn reject_events(fixture: &Fixture, deferred: bool) {
    sqlx::raw_sql("CREATE FUNCTION reject_event() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture-secret'; END $$;")
        .execute(fixture.pool()).await.unwrap();
    let ddl = if deferred {
        "CREATE CONSTRAINT TRIGGER reject_event AFTER INSERT ON book_smartz.ranking_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_event()"
    } else {
        "CREATE TRIGGER reject_event BEFORE INSERT ON book_smartz.ranking_events FOR EACH ROW EXECUTE FUNCTION reject_event()"
    };
    sqlx::query(ddl).execute(fixture.pool()).await.unwrap();
}

#[googletest::test]
#[tokio::test]
async fn failed_append_rolls_back_the_first_stream() {
    let fixture = Fixture::new().await;
    let observations = Arc::new(Recorder::default());
    let store = fixture.store(observations.clone());
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    let a = book(1);
    let b = book(2);
    store.register_book(&a, op).await.unwrap();
    store.register_book(&b, op).await.unwrap();
    let existing = ReaderId::new(Uuid::new_v4());
    store
        .execute(existing, context(0), Command::Start { book_id: a.id() }, op)
        .await
        .unwrap();
    let before = store.load_ranking(existing, op).await.unwrap();
    reject_events(&fixture, false).await;
    observations.0.lock().unwrap().clear();
    let fresh = ReaderId::new(Uuid::new_v4());
    for (reader, revision) in [(fresh, 0), (existing, 1)] {
        let error = store
            .execute(
                reader,
                context(revision),
                Command::Start { book_id: b.id() },
                op,
            )
            .await
            .err()
            .unwrap();
        assert_that!(matches!(error, StoreError::Database(_)), eq(true));
    }
    // Acquiring the affected stream lock synchronizes with asynchronous rollback.
    let mut tx = fixture.pool().begin().await.unwrap();
    sqlx::query("SELECT reader_id FROM book_smartz.reader_streams FOR UPDATE")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_that!(
        store.load_ranking(existing, op).await.unwrap().history(),
        eq(before.history())
    );
    assert_that!(
        store.load_ranking(fresh, op).await.unwrap().history().len(),
        eq(0)
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM book_smartz.reader_streams WHERE reader_id = $1")
            .bind(fresh.as_uuid())
            .fetch_one(fixture.pool())
            .await
            .unwrap();
    assert_that!(count, eq(0));
    assert_that!(
        observations
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|o| o.outcome == Outcome::Committed),
        eq(false)
    );
}

#[googletest::test]
#[tokio::test]
async fn closed_pool_and_definite_commit_failure_are_safe() {
    let fixture = Fixture::new().await;
    let observations = Arc::new(Recorder::default());
    let store = fixture.store(observations.clone());
    let op = OperationContext::default();
    store.migrate(op).await.unwrap();
    let a = book(1);
    store.register_book(&a, op).await.unwrap();
    reject_events(&fixture, true).await;
    observations.0.lock().unwrap().clear();
    let reader = ReaderId::new(Uuid::new_v4());
    let error = store
        .execute(reader, context(0), Command::Start { book_id: a.id() }, op)
        .await
        .err()
        .unwrap();
    assert_that!(error.to_string().contains("fixture-secret"), eq(false));
    assert_that!(matches!(error, StoreError::Database(_)), eq(true));
    assert_that!(
        store
            .load_ranking(reader, op)
            .await
            .unwrap()
            .history()
            .len(),
        eq(0)
    );
    sqlx::query("CREATE CONSTRAINT TRIGGER reject_book AFTER INSERT ON book_smartz.books DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_event()")
        .execute(fixture.pool()).await.unwrap();
    let b = book(2);
    let error = store.register_book(&b, op).await.err().unwrap();
    assert_that!(error.to_string(), eq("database operation failed"));
    assert_that!(
        matches!(error, StoreError::Database(sqlx::Error::Database(_))),
        eq(true)
    );
    assert_that!(
        store.book_by_id(b.id(), op).await.unwrap().is_none(),
        eq(true)
    );
    fixture.pool().close().await;
    for error in [
        store.book_by_id(a.id(), op).await.err().unwrap(),
        store.book_by_work_id(a.work_id(), op).await.err().unwrap(),
        store.load_ranking(reader, op).await.err().unwrap(),
        store.register_book(&a, op).await.err().unwrap(),
        store
            .execute(reader, context(0), Command::Start { book_id: a.id() }, op)
            .await
            .err()
            .unwrap(),
    ] {
        assert_that!(error.to_string(), eq("database operation failed"));
        assert_that!(matches!(error, StoreError::Database(_)), eq(true));
    }
    assert_that!(
        format!("{:?}", observations.0.lock().unwrap()).contains("fixture-secret"),
        eq(false)
    );
    assert_that!(
        observations
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|o| o.outcome == Outcome::Committed),
        eq(false)
    );
}

#[googletest::test]
#[tokio::test]
async fn lost_commit_acknowledgment_is_recoverable() {
    use super::commit_proxy::CommitProxy;
    use book_smartz_storage::Store;
    let fixture = Fixture::new().await;
    let observations = Arc::new(Recorder::default());
    let direct = fixture.store(observations.clone());
    let op = OperationContext::default();
    direct.migrate(op).await.unwrap();
    let a = book(1);
    let proxy = CommitProxy::new(fixture.pool()).await;
    let store = Store::new(proxy.pool.clone(), observations.clone());
    let registering = tokio::spawn({
        let a = a.clone();
        async move { store.register_book(&a, op).await }
    });
    proxy.commit_reached().await;
    assert_that!(
        direct.book_by_id(a.id(), op).await.unwrap(),
        eq(&Some(a.clone()))
    );
    proxy.disconnect();
    assert_that!(
        matches!(
            registering.await.unwrap(),
            Err(StoreError::CommitUncertain(_))
        ),
        eq(true)
    );
    assert_that!(
        direct.register_book(&a, op).await.unwrap(),
        eq(Registration::AlreadyPresent)
    );

    let proxy = CommitProxy::new(fixture.pool()).await;
    let store = Store::new(proxy.pool.clone(), observations.clone());
    let reader = ReaderId::new(Uuid::new_v4());
    let context = context(0);
    let command = Command::Start { book_id: a.id() };
    let executing = tokio::spawn(async move { store.execute(reader, context, command, op).await });
    proxy.commit_reached().await;
    let ranking = direct.load_ranking(reader, op).await.unwrap();
    assert_that!(ranking.history().len(), eq(1));
    let event = &ranking.history()[0];
    assert_that!(event.metadata().id, eq(context.event_id));
    assert_that!(event.metadata().time, eq(context.time));
    assert_that!(event.candidate(), eq(a.id()));
    proxy.disconnect();
    assert_that!(
        matches!(
            executing.await.unwrap(),
            Err(StoreError::CommitUncertain(_))
        ),
        eq(true)
    );
    assert_that!(
        matches!(
            direct.execute(reader, context, command, op).await,
            Err(StoreError::Domain(DomainError::StaleRevision { .. }))
        ),
        eq(true)
    );
    assert_that!(
        direct.load_ranking(reader, op).await.unwrap().history(),
        eq(ranking.history())
    );
    assert_that!(
        observations
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|o| o.outcome == Outcome::CommitUncertain)
            .count(),
        eq(2)
    );
    assert_that!(
        observations
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|o| o.outcome == Outcome::Committed || o.outcome == Outcome::Created),
        eq(false)
    );
    eprintln!(
        "fault evidence: PostgreSQL COMMIT completions withheld for registration and ranking; direct reads confirmed both durable; original-context retries did not duplicate"
    );
}

#[googletest::test]
#[tokio::test]
async fn cancellation_while_awaiting_commit_is_recoverable() {
    use super::commit_proxy::CommitProxy;
    use book_smartz_storage::Store;
    let fixture = Fixture::new().await;
    let observations = Arc::new(Recorder::default());
    let direct = fixture.store(observations.clone());
    let op = OperationContext::default();
    direct.migrate(op).await.unwrap();
    let a = book(1);
    direct.register_book(&a, op).await.unwrap();
    observations.0.lock().unwrap().clear();
    let proxy = CommitProxy::new(fixture.pool()).await;
    let store = Store::new(proxy.pool.clone(), observations.clone());
    let reader = ReaderId::new(Uuid::new_v4());
    let context = context(0);
    let command = Command::Start { book_id: a.id() };
    let executing = tokio::spawn(async move { store.execute(reader, context, command, op).await });
    proxy.commit_reached().await;
    executing.abort();
    assert_that!(executing.await.err().unwrap().is_cancelled(), eq(true));
    proxy.disconnect();
    let ranking = direct.load_ranking(reader, op).await.unwrap();
    assert_that!(ranking.history().len(), eq(1));
    assert_that!(ranking.history()[0].metadata().id, eq(context.event_id));
    assert_that!(ranking.history()[0].metadata().time, eq(context.time));
    assert_that!(ranking.history()[0].candidate(), eq(a.id()));
    assert_that!(
        matches!(
            direct.execute(reader, context, command, op).await,
            Err(StoreError::Domain(DomainError::StaleRevision { .. }))
        ),
        eq(true)
    );
    assert_that!(
        direct.load_ranking(reader, op).await.unwrap().history(),
        eq(ranking.history())
    );
    assert_that!(
        observations
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|o| o.outcome == Outcome::Committed),
        eq(false)
    );
    eprintln!(
        "fault evidence: canceled caller while real PostgreSQL COMMIT completion withheld; durable event recovered and original-context retry rejected"
    );
}

#[googletest::test]
#[tokio::test]
async fn unknown_completion_and_connection_errors_preserve_commit_uncertainty() {
    use super::commit_proxy::CommitProxy;
    use book_smartz_storage::Store;
    let fixture = Fixture::new().await;
    let observations = Arc::new(Recorder::default());
    let direct = fixture.store(observations.clone());
    let op = OperationContext::default();
    direct.migrate(op).await.unwrap();
    for (index, (severity, code)) in [
        ("ERROR", "08007"),
        ("ERROR", "40003"),
        ("ERROR", "08006"),
        ("ERROR", "08P01"),
        ("FATAL", "57P01"),
        ("FATAL", "23514"),
        ("PANIC", "XX000"),
    ]
    .into_iter()
    .enumerate()
    {
        let proxy = CommitProxy::with_response(fixture.pool(), Some((severity, code))).await;
        let store = Store::new(proxy.pool.clone(), observations.clone());
        let book = book(index as u128 + 1);
        let registering = tokio::spawn(async move { store.register_book(&book, op).await });
        proxy.commit_reached().await;
        proxy.disconnect();
        let error = registering.await.unwrap().err().unwrap();
        assert_that!(error.to_string().contains("fixture-secret"), eq(false));
        let StoreError::CommitUncertain(sqlx::Error::Database(cause)) = error else {
            panic!("{severity}/{code} must retain its database cause as uncertain");
        };
        assert_that!(cause.code().as_deref(), eq(Some(code)));
    }
    assert_that!(
        observations
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|o| o.outcome == Outcome::CommitUncertain)
            .count(),
        eq(7)
    );
}
