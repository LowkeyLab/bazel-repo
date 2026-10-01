use super::observation_capture::{Capture, Value, subscriber};
use super::support::{Fixture, recorder};
use book_smartz_domain::{
    Book, BookId, Command, CommandContext, EventId, OpenLibraryWorkId, ReaderId,
};
use book_smartz_storage::{
    Observation, ObservationDeliveryError, Observer, OperationContext, Registration,
    tracing_observer,
};
use googletest::{assert_that, matchers::eq};
use std::sync::Arc;
use tracing::instrument::WithSubscriber;
use uuid::Uuid;

enum Broken {
    Error,
    Panic,
}
impl Observer for Broken {
    fn observe(&self, _: &Observation) -> Result<(), ObservationDeliveryError> {
        match self {
            Self::Error => Err(ObservationDeliveryError),
            Self::Panic => panic!("SECRET_RAW_PAYLOAD"),
        }
    }
}
fn book() -> Book {
    Book::new(
        BookId::new(Uuid::from_u128(1)),
        OpenLibraryWorkId::try_from("OL1W").unwrap(),
    )
}
fn context() -> CommandContext {
    CommandContext {
        expected_revision: 0,
        event_id: EventId::new(Uuid::from_u128(2)),
        time: chrono::DateTime::parse_from_rfc3339("2026-10-01T00:00:00Z")
            .unwrap()
            .to_utc(),
    }
}

#[googletest::test]
#[tokio::test]
async fn observer_failure_cannot_change_committed_result() {
    for observer in [recorder(), Arc::new(Broken::Error), Arc::new(Broken::Panic)] {
        let fixture = Fixture::new().await;
        let store = fixture.store(observer);
        let op = OperationContext::default();
        store.migrate(op).await.unwrap();
        assert_that!(
            store.register_book(&book(), op).await.unwrap(),
            eq(Registration::Created)
        );
        let reader = ReaderId::new(Uuid::from_u128(3));
        let result = store
            .execute(
                reader,
                context(),
                Command::Start {
                    book_id: book().id(),
                },
                op,
            )
            .await
            .unwrap();
        assert_that!(result.projection.revision(), eq(1));
        assert_that!(
            store
                .load_ranking(reader, op)
                .await
                .unwrap()
                .projection()
                .revision(),
            eq(1)
        );
        assert_that!(
            fixture
                .store(recorder())
                .load_ranking(reader, op)
                .await
                .unwrap()
                .history(),
            eq(&[result.event])
        );
    }
}

#[googletest::test]
#[tokio::test]
async fn public_composition_observes_first_operation_and_committed_activity() {
    let fixture = Fixture::new().await;
    let capture = Capture::default();
    let store = fixture.store(tracing_observer());
    async {
        let op = OperationContext {
            correlation_id: Some(Uuid::from_u128(99)),
        };
        store.migrate(op).await.unwrap();
        store.register_book(&book(), op).await.unwrap();
        let reader = ReaderId::new(Uuid::from_u128(3));
        store
            .execute(
                reader,
                context(),
                Command::Start {
                    book_id: book().id(),
                },
                op,
            )
            .await
            .unwrap();
        assert_that!(
            fixture
                .store(recorder())
                .load_ranking(reader, op)
                .await
                .unwrap()
                .history()
                .len(),
            eq(1)
        );
        assert_that!(
            store
                .execute(reader, context(), Command::Resume, op)
                .await
                .is_err(),
            eq(true)
        );
        store.load_ranking(reader, op).await.unwrap();
    }
    .with_subscriber(subscriber(capture.clone()))
    .await;
    let records = capture.0.lock().unwrap();
    assert_that!(records[0].level, eq(tracing::Level::INFO));
    assert_that!(records[3].level, eq(tracing::Level::DEBUG));
    let outcomes: Vec<_> = records
        .iter()
        .map(|r| r.fields.get("outcome").unwrap().clone())
        .collect();
    assert_that!(
        outcomes,
        eq(&[
            "Applied",
            "Created",
            "Committed",
            "Rejected(StaleRevision)",
            "Loaded"
        ]
        .map(|s| Value::Text(s.into()))
        .to_vec())
    );
}

#[googletest::test]
#[tokio::test]
async fn tracing_failure_record_excludes_retained_database_secrets() {
    let fixture = Fixture::new().await;
    let setup = fixture.store(recorder());
    let op = OperationContext {
        correlation_id: Some(Uuid::from_u128(99)),
    };
    setup.migrate(op).await.unwrap();
    sqlx::raw_sql(
        "CREATE FUNCTION reject_secret_book() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
           RAISE EXCEPTION USING MESSAGE = 'SECRET_RAW_PAYLOAD password=fixture-secret',
             ERRCODE = '23514';
         END $$;
         CREATE CONSTRAINT TRIGGER reject_secret_book AFTER INSERT ON book_smartz.books
         DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_secret_book()",
    )
    .execute(fixture.pool())
    .await
    .unwrap();

    let capture = Capture::default();
    let store = fixture.store(tracing_observer());
    let error = store
        .register_book(&book(), op)
        .with_subscriber(subscriber(capture.clone()))
        .await
        .unwrap_err();
    let book_smartz_storage::StoreError::Database(sqlx::Error::Database(cause)) = &error else {
        panic!("expected a retained PostgreSQL database cause");
    };
    assert_that!(
        cause.message(),
        eq("SECRET_RAW_PAYLOAD password=fixture-secret")
    );
    for secret in ["SECRET_RAW_PAYLOAD", "fixture-secret"] {
        assert_that!(error.to_string().contains(secret), eq(false));
    }
    assert_that!(
        setup.book_by_id(book().id(), op).await.unwrap().is_none(),
        eq(true)
    );

    let records = capture.0.lock().unwrap();
    assert_that!(records.len(), eq(1));
    let record = &records[0];
    assert_that!(record.level, eq(tracing::Level::ERROR));
    assert_that!(
        record.fields.get("operation"),
        eq(Some(&Value::Text("Registration".into())))
    );
    assert_that!(
        record.fields.get("outcome"),
        eq(Some(&Value::Text("Failed(Database)".into())))
    );
    assert_that!(
        record.fields.get("correlation_id"),
        eq(Some(&Value::Text(Uuid::from_u128(99).to_string())))
    );
    assert_that!(
        record.fields.keys().map(String::as_str).collect::<Vec<_>>(),
        eq(&vec![
            "correlation_id",
            "duration_ms",
            "operation",
            "outcome"
        ])
    );
    for value in record.fields.values() {
        if let Value::Text(text) = value {
            for secret in ["SECRET_RAW_PAYLOAD", "fixture-secret"] {
                assert_that!(text.contains(secret), eq(false));
            }
        }
    }
}
