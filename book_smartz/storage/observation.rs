use std::{sync::Arc, time::Duration};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Migration,
    Registration,
    BookById,
    BookByWorkId,
    RankingLoad,
    RankingCommand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    IdentityConflict,
    StaleRevision,
    DuplicateEventId,
    InvalidTransition,
    UnknownBook,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Rejected,
    CorruptHistory,
    Database,
    Migration,
    CommitUncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    Created,
    AlreadyPresent,
    Found,
    Absent,
    Loaded,
    Committed,
    Rejected(Rejection),
    Failed(Failure),
    CommitUncertain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub operation: Operation,
    pub outcome: Outcome,
    pub duration: Duration,
    pub correlation_id: Option<Uuid>,
    pub reader_id: Option<Uuid>,
    pub event_id: Option<Uuid>,
    pub revision: Option<u64>,
    pub event_count: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
#[error("observation delivery failed")]
pub struct ObservationDeliveryError;

pub trait Observer: Send + Sync {
    fn observe(&self, observation: &Observation) -> Result<(), ObservationDeliveryError>;
}

pub type SharedObserver = Arc<dyn Observer>;

pub(crate) fn dispatch(observer: &SharedObserver, observation: &Observation) {
    // A failed diagnostic sink must not change a completed database result.
    if observer.observe(observation).is_err() {
        tracing::warn!("book_smartz observation delivery failed");
    }
}
