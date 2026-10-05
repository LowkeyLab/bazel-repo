use crate::observation::Failure;
use book_smartz_domain::{DomainError, IdentityError};
use sqlx::postgres::{PgDatabaseError, PgSeverity};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error("stored ranking history is invalid: {0}")]
    CorruptHistory(&'static str),
    #[error("database operation failed")]
    Database(#[source] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
    #[error("database commit outcome is uncertain")]
    CommitUncertain(#[source] sqlx::Error),
}

impl StoreError {
    /// ERROR normally acknowledges command rejection; connection exceptions,
    /// unknown completion, and session-ending severities cannot confirm rollback.
    pub(crate) fn from_commit(error: sqlx::Error) -> Self {
        let definite_rejection = error
            .as_database_error()
            .and_then(|cause| cause.try_downcast_ref::<PgDatabaseError>())
            .is_some_and(|cause| {
                cause.severity() == PgSeverity::Error
                    && !cause.code().starts_with("08")
                    && cause.code() != "40003"
            });
        if definite_rejection {
            Self::Database(error)
        } else {
            Self::CommitUncertain(error)
        }
    }

    #[must_use]
    pub const fn failure(&self) -> Failure {
        match self {
            Self::Identity(_) | Self::Domain(_) => Failure::Rejected,
            Self::CorruptHistory(_) => Failure::CorruptHistory,
            Self::Database(_) => Failure::Database,
            Self::Migration(_) => Failure::Migration,
            Self::CommitUncertain(_) => Failure::CommitUncertain,
        }
    }
}

impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<sqlx::migrate::MigrateError> for StoreError {
    fn from(error: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(error)
    }
}
