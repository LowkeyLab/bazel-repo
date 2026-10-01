use crate::observation::Failure;
use book_smartz_domain::{DomainError, IdentityError};

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
    #[must_use]
    pub fn failure(&self) -> Failure {
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
