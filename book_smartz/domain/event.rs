use chrono::{DateTime, Utc};

use crate::{BookId, DomainError, EventId, ReaderId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sequence(u64);

impl Sequence {
    /// Creates a positive event sequence.
    ///
    /// # Errors
    /// Returns `InvalidSequence` for zero.
    pub fn new(value: u64) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::InvalidSequence);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn value(&self) -> u64 {
        self.0
    }

    /// Returns the next sequence.
    ///
    /// # Errors
    /// Returns `SequenceOverflow` after the maximum sequence.
    pub fn successor(&self) -> Result<Self, DomainError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(DomainError::SequenceOverflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventMetadata {
    pub id: EventId,
    pub reader_id: ReaderId,
    pub sequence: Sequence,
    pub time: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonChoice {
    PreferCandidate,
    PreferOpponent,
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    PlacementStarted,
    ComparisonAnswered {
        opponent: BookId,
        choice: ComparisonChoice,
    },
    PlacementPaused,
    PlacementResumed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankingEvent {
    metadata: EventMetadata,
    candidate: BookId,
    kind: EventKind,
}

impl RankingEvent {
    #[must_use]
    pub fn new(metadata: EventMetadata, candidate: BookId, kind: EventKind) -> Self {
        Self {
            metadata,
            candidate,
            kind,
        }
    }

    #[must_use]
    pub fn metadata(&self) -> &EventMetadata {
        &self.metadata
    }

    #[must_use]
    pub fn candidate(&self) -> BookId {
        self.candidate
    }

    #[must_use]
    pub fn kind(&self) -> &EventKind {
        &self.kind
    }
}
