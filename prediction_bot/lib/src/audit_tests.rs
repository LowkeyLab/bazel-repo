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

#[test]
fn replay_validation_is_an_operational_failure() {
    let error = StoreError::Domain(DomainError::Invalid("insufficient points"));
    assert!(matches!(
        store_outcome(Stage::Replay, &error),
        Outcome::Failed(Failure {
            category: FailureCategory::History,
            ..
        })
    ));
    assert_eq!(
        store_outcome(Stage::Decide, &error),
        Outcome::Rejected(Rejection::InsufficientPoints)
    );
}

#[test]
fn configuration_details_do_not_enter_the_event_contract() {
    let error = StoreError::Database(sqlx::Error::Configuration(
        "postgres://sentinel-secret@private-host/database".into(),
    ));
    assert_eq!(
        store_outcome(Stage::Acquire, &error),
        Outcome::Failed(Failure {
            category: FailureCategory::Configuration,
            sqlstate: None,
            http_status: None,
            discord_code: None,
        })
    );
}

#[test]
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
        assert_eq!(
            store_outcome(Stage::Decide, &error),
            Outcome::Rejected(rejection)
        );
    }
}

#[test]
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
        assert_eq!(
            store_outcome(Stage::Append, &error),
            Outcome::Failed(Failure {
                category,
                sqlstate: None,
                http_status: None,
                discord_code: None,
            })
        );
    }
}

#[test]
fn database_errors_allowlist_sqlstate_without_copying_messages() {
    for (code, category, sqlstate) in [
        ("23514", FailureCategory::Constraint, Some("23514")),
        ("08006", FailureCategory::Connection, Some("08006")),
        ("57014", FailureCategory::Timeout, Some("57014")),
        ("SECRET", FailureCategory::Database, None),
    ] {
        let error = StoreError::Database(sqlx::Error::Database(Box::new(TestDatabaseError(code))));
        assert_eq!(
            store_outcome(Stage::Commit, &error),
            Outcome::Failed(Failure {
                category,
                sqlstate: sqlstate.map(str::to_owned),
                http_status: None,
                discord_code: None,
            })
        );
    }
}

#[test]
fn discord_failures_use_safe_categories() {
    assert_eq!(
        discord_failure(&serenity::Error::Io(io::Error::other(
            "sentinel-private-host"
        ))),
        Failure {
            category: FailureCategory::Transport,
            sqlstate: None,
            http_status: None,
            discord_code: None,
        }
    );
    assert_eq!(
        discord_failure(&serenity::Error::Other("sentinel-private-message")),
        Failure {
            category: FailureCategory::Unknown,
            sqlstate: None,
            http_status: None,
            discord_code: None,
        }
    );
}

#[test]
fn command_keys_must_be_canonical_before_entering_events() {
    for key in ["discord:1", "discord:18446744073709551615", "grant:7:-20"] {
        assert_eq!(canonical_command_key(key), Some(key.to_owned()));
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
        assert_eq!(canonical_command_key(key), None);
    }
}
