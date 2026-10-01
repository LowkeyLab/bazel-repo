use crate::observation_capture::{Capture, Value, subscriber};
use crate::{Failure, Observation, Operation, Outcome, Rejection, tracing_observer};
use googletest::{assert_that, matchers::eq};
use std::time::Duration;
use tracing::Level;
use uuid::Uuid;
#[googletest::test]
fn structured_listener_maps_outcomes_and_safe_context() {
    let capture = Capture::default();
    let observer = tracing_observer();
    let id = Uuid::from_u128(42);
    let cases = [
        (Outcome::Applied, Level::INFO),
        (Outcome::Created, Level::INFO),
        (Outcome::AlreadyPresent, Level::DEBUG),
        (Outcome::Found, Level::DEBUG),
        (Outcome::Absent, Level::DEBUG),
        (Outcome::Loaded, Level::DEBUG),
        (Outcome::Committed, Level::INFO),
        (Outcome::Rejected(Rejection::IdentityConflict), Level::DEBUG),
        (Outcome::Failed(Failure::Database), Level::ERROR),
        (Outcome::Failed(Failure::CorruptHistory), Level::ERROR),
        (Outcome::Failed(Failure::Migration), Level::ERROR),
        (Outcome::CommitUncertain, Level::ERROR),
    ];
    tracing::subscriber::with_default(subscriber(capture.clone()), || {
        for (outcome, _) in cases {
            observer
                .observe(&Observation {
                    operation: Operation::RankingLoad,
                    command_kind: Some(crate::CommandKind::Start),
                    outcome,
                    duration: Duration::from_micros(1250),
                    correlation_id: Some(id),
                    reader_id: Some(id),
                    event_id: Some(id),
                    revision: Some(7),
                    event_count: Some(7),
                })
                .unwrap();
        }
    });
    let records = capture.0.lock().unwrap();
    assert_that!(records.len(), eq(cases.len()));
    for (record, (_, level)) in records.iter().zip(cases) {
        assert_that!(record.level, eq(level));
        assert_that!(record.fields.len(), eq(9));
        assert_that!(
            record.fields.get("command_kind"),
            eq(Some(&Value::Text("Start".into())))
        );
        for field in ["correlation_id", "reader_id", "event_id"] {
            assert_that!(
                record.fields.get(field),
                eq(Some(&Value::Text(id.to_string())))
            );
        }
        assert_that!(record.fields.get("revision"), eq(Some(&Value::Number(7))));
        assert_that!(
            record.fields.get("event_count"),
            eq(Some(&Value::Number(7)))
        );
        assert_that!(
            record.fields.get("duration_ms"),
            eq(Some(&Value::Float(1.25)))
        );
    }
}
