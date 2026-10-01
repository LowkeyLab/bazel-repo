//! PostgreSQL persistence for Book Smartz.
#![forbid(unsafe_code)]

mod books;
mod error;
mod migration;
mod observation;
mod ranking;
mod wire;

pub use error::StoreError;
pub use observation::{
    Failure, Observation, ObservationDeliveryError, Observer, Operation, Outcome, Rejection,
    SharedObserver,
};

use sqlx::PgPool;
use std::time::Instant;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperationContext {
    pub correlation_id: Option<Uuid>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Registration {
    Created,
    AlreadyPresent,
}

pub struct CommandResult {
    pub event: book_smartz_domain::RankingEvent,
    pub projection: book_smartz_domain::RankingProjection,
}

#[derive(Clone)]
pub struct Store {
    pub(crate) pool: PgPool,
    pub(crate) observer: SharedObserver,
}

impl Store {
    #[must_use]
    pub fn new(pool: PgPool, observer: SharedObserver) -> Self {
        Self { pool, observer }
    }

    /// Applies versioned migrations in the isolated `book_smartz` schema.
    ///
    /// # Errors
    /// Returns a migration or database error if bootstrap or migration fails.
    pub async fn migrate(&self, operation: OperationContext) -> Result<(), StoreError> {
        let started = Instant::now();
        let result = migration::migrate(&self.pool).await;
        let outcome = match &result {
            Ok(()) => Outcome::Applied,
            Err(error) => Outcome::Failed(error.failure()),
        };
        observation::dispatch(
            &self.observer,
            &Observation {
                operation: Operation::Migration,
                outcome,
                duration: started.elapsed(),
                correlation_id: operation.correlation_id,
                reader_id: None,
                event_id: None,
                revision: None,
                event_count: None,
            },
        );
        result
    }
}
