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

struct Broken(bool);
impl Observer for Broken {
    fn observe(&self, _: &Observation) -> Result<(), ObservationDeliveryError> {
        if self.0 {
            panic!("SECRET_RAW_PAYLOAD");
        }
        Err(ObservationDeliveryError)
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
    for observer in [recorder(), Arc::new(Broken(false)), Arc::new(Broken(true))] {
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
