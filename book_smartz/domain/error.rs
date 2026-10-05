#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    #[error("invalid Open Library work ID")]
    InvalidWorkId,
    #[error("book ID already refers to a different work")]
    BookIdConflict,
    #[error("work ID already refers to a different book")]
    WorkIdConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DomainError {
    #[error("sequence must be positive")]
    InvalidSequence,
    #[error("sequence overflow")]
    SequenceOverflow,
    #[error("book is not registered")]
    UnknownBook,
    #[error("stale revision: expected {expected}, current {actual}")]
    StaleRevision { expected: u64, actual: u64 },
    #[error("invalid command transition: {0}")]
    InvalidTransition(ReplayErrorReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReplayErrorReason {
    #[error("expected sequence {expected}")]
    SequenceMismatch { expected: u64 },
    #[error("sequence overflow")]
    SequenceOverflow,
    #[error("event ID already applied")]
    DuplicateEventId,
    #[error("event belongs to another reader")]
    WrongReader,
    #[error("book is already ranked")]
    DuplicateBook,
    #[error("a placement is already pending")]
    PendingPlacement,
    #[error("no placement is active")]
    NoActivePlacement,
    #[error("event candidate does not match pending placement")]
    WrongCandidate,
    #[error("event opponent is not the selected opponent")]
    WrongOpponent,
    #[error("placement is paused")]
    PlacementPaused,
    #[error("placement is not paused")]
    NotPaused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid event at sequence {attempted_sequence}: {reason}")]
pub struct ReplayError {
    attempted_sequence: u64,
    reason: ReplayErrorReason,
}

impl ReplayError {
    pub(crate) const fn new(attempted_sequence: u64, reason: ReplayErrorReason) -> Self {
        Self {
            attempted_sequence,
            reason,
        }
    }

    #[must_use]
    pub const fn attempted_sequence(&self) -> u64 {
        self.attempted_sequence
    }

    #[must_use]
    pub const fn reason(&self) -> &ReplayErrorReason {
        &self.reason
    }
}
