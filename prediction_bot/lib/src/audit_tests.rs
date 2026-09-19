use googletest::{
    assert_that,
    matchers::{contains_substring, eq, matches_pattern, not},
};
use std::{borrow::Cow, fmt, io};

use super::audit::{
    Failure, FailureCategory, Outcome, Rejection, Stage, canonical_command_key, discord_failure,
    store_outcome,
};
use super::domain::DomainError;
use super::store::StoreError;

#[derive(Debug)]
struct TestDatabaseError(&'static str);

impl fmt::Display for TestDatabaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sentinel-private-database-message")
    }
}

impl std::error::Error for TestDatabaseError {}

impl sqlx::error::DatabaseError for TestDatabaseError {
    fn message(&self) -> &str {
        "sentinel-private-database-message"
    }

    fn code(&self) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(self.0))
    }

    fn as_error(&self) -> &(dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn as_error_mut(&mut self) -> &mut (dyn std::error::Error + Send + Sync + 'static) {
        self
    }

    fn into_error(self: Box<Self>) -> Box<dyn std::error::Error + Send + Sync + 'static> {
        self
    }

    fn kind(&self) -> sqlx::error::ErrorKind {
        sqlx::error::ErrorKind::Other
    }
}

#[googletest::test]
fn replay_validation_is_an_operational_failure() {
    let error = StoreError::Domain(DomainError::Invalid("insufficient points"));
    assert_that!(
        store_outcome(Stage::Replay, &error),
        matches_pattern!(Outcome::Failed(matches_pattern!(Failure {
            category: eq(&FailureCategory::History),
            ..
        })))
    );
    assert_that!(
        store_outcome(Stage::Decide, &error),
        eq(&Outcome::Rejected(Rejection::InsufficientPoints))
    );
}

#[googletest::test]
fn configuration_details_do_not_enter_the_event_contract() {
    let error = StoreError::Database(sqlx::Error::Configuration(
        "postgres://sentinel-secret@private-host/database".into(),
    ));
    assert_that!(
        store_outcome(Stage::Acquire, &error),
        eq(&Outcome::Failed(Failure {
            category: FailureCategory::Configuration,
            sqlstate: None,
            http_status: None,
            discord_code: None,
        }))
    );
}

#[googletest::test]
fn decide_rejections_are_typed_without_copying_reasons() {
    for (reason, rejection) in [
        ("insufficient points", Rejection::InsufficientPoints),
        ("member not enrolled", Rejection::NotEnrolled),
        ("unknown market", Rejection::MarketUnavailable),
        (
            "market is not open for betting",
            Rejection::MarketUnavailable,
        ),
        ("moderator required", Rejection::PermissionDenied),
        ("sentinel-private-reason", Rejection::InvalidInput),
    ] {
        let error = StoreError::Domain(DomainError::Invalid(reason));
        assert_that!(
            store_outcome(Stage::Decide, &error),
            eq(&Outcome::Rejected(rejection))
        );
    }
}

#[googletest::test]
fn operational_errors_use_safe_categories() {
    let cases = [
        (
            StoreError::Database(sqlx::Error::PoolTimedOut),
            FailureCategory::PoolTimeout,
        ),
        (
            StoreError::Database(sqlx::Error::Io(io::Error::other("sentinel-private-host"))),
            FailureCategory::Connection,
        ),
        (
            StoreError::Domain(DomainError::Overflow),
            FailureCategory::Overflow,
        ),
        (
            StoreError::History("sentinel-private-history"),
            FailureCategory::History,
        ),
    ];

    for (error, category) in cases {
        assert_that!(
            store_outcome(Stage::Append, &error),
            eq(&Outcome::Failed(Failure {
                category,
                sqlstate: None,
                http_status: None,
                discord_code: None,
            }))
        );
    }
}

#[googletest::test]
fn database_errors_allowlist_sqlstate_without_copying_messages() {
    for (code, category, sqlstate) in [
        ("23514", FailureCategory::Constraint, Some("23514")),
        ("08006", FailureCategory::Connection, Some("08006")),
        ("57014", FailureCategory::Timeout, Some("57014")),
        ("SECRET", FailureCategory::Database, None),
    ] {
        let error = StoreError::Database(sqlx::Error::Database(Box::new(TestDatabaseError(code))));
        assert_that!(
            store_outcome(Stage::Commit, &error),
            eq(&Outcome::Failed(Failure {
                category,
                sqlstate: sqlstate.map(str::to_owned),
                http_status: None,
                discord_code: None,
            }))
        );
    }
}

#[googletest::test]
fn discord_failures_use_safe_categories() {
    assert_that!(
        discord_failure(&serenity::Error::Io(io::Error::other(
            "sentinel-private-host"
        ))),
        eq(&Failure {
            category: FailureCategory::Transport,
            sqlstate: None,
            http_status: None,
            discord_code: None,
        })
    );
    assert_that!(
        discord_failure(&serenity::Error::Other("sentinel-private-message")),
        eq(&Failure {
            category: FailureCategory::Unknown,
            sqlstate: None,
            http_status: None,
            discord_code: None,
        })
    );
}

#[googletest::test]
fn command_keys_must_be_canonical_before_entering_events() {
    for key in ["discord:1", "discord:18446744073709551615", "grant:7:-20"] {
        assert_that!(canonical_command_key(key), eq(&Some(key.to_owned())));
    }

    for key in [
        "discord:0",
        "discord:01",
        "discord:sentinel-secret",
        "grant:0:20",
        "grant:7:+20",
        "grant:07:20",
        "grant:7:020",
        "grant:7:20:extra",
    ] {
        assert_that!(canonical_command_key(key), eq(&None));
    }
}

#[googletest::test]
fn announcement_logging_serializes_safe_correlation_and_retry_decisions() {
    use crate::audit::{AnnouncementDecision, AuditEvent, logging_listener};
    use std::sync::{Arc, Mutex};
    #[derive(Clone)]
    struct Output(Arc<Mutex<Vec<u8>>>);
    impl io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let output = Output(bytes.clone());
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_ansi(false)
        .with_writer(move || output.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, || {
        logging_listener().on_event(&AuditEvent::AnnouncementAttemptCompleted {
            guild: 10,
            revision: 4,
            channel_id: 20,
            configuration_version: 2,
            decision: AnnouncementDecision::Retry,
            stage: Stage::Deliver,
            outcome: Outcome::Failed(discord_failure(&serenity::Error::Io(io::Error::other(
                "sentinel-private-token",
            )))),
        });
        logging_listener().on_event(&AuditEvent::AnnouncementWorkerFailed {
            stage: Stage::Discover,
            outcome: store_outcome(
                Stage::Discover,
                &StoreError::Database(sqlx::Error::PoolClosed),
            ),
        });
    });
    let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
    assert_that!(output, not(contains_substring("sentinel")));
    let rows: Vec<serde_json::Value> = output
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_that!(rows.len(), eq(2));
    assert_that!(rows[0]["level"], eq("WARN"));
    let fields = &rows[0]["fields"];
    assert_that!(fields["event.name"], eq("announcement_attempt_completed"));
    assert_that!(fields["announcement.guild"], eq(10));
    assert_that!(fields["announcement.revision"], eq(4));
    assert_that!(fields["announcement.channel_id"], eq(20));
    assert_that!(fields["announcement.configuration_version"], eq(2));
    assert_that!(fields["announcement.decision"], eq("retry"));
    assert_that!(fields["failure.category"], eq("transport"));
    assert_that!(rows[1]["level"], eq("ERROR"));
    assert_that!(
        rows[1]["fields"]["event.name"],
        eq("announcement_worker_failed")
    );
    assert_that!(rows[1]["fields"]["operation.stage"], eq("discover"));
}
