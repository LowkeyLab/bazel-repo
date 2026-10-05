use chrono::{DateTime, Utc};

use crate::{
    BookId, BookRegistry, ComparisonChoice, DomainError, EventId, EventKind, EventMetadata,
    RankingEvent, RankingProjection, ReaderId, ReplayError, ReplayErrorReason, Sequence,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Start {
        book_id: BookId,
    },
    Answer {
        opponent: BookId,
        choice: ComparisonChoice,
    },
    Pause,
    Resume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandContext {
    pub expected_revision: u64,
    pub event_id: EventId,
    pub time: DateTime<Utc>,
}

/// Validates a command and proposes its event without changing the projection.
///
/// # Errors
/// Returns a domain error for stale revisions, unknown books, invalid transitions, or overflow.
pub fn decide(
    projection: &RankingProjection,
    registry: &BookRegistry,
    context: CommandContext,
    command: Command,
) -> Result<RankingEvent, DomainError> {
    let actual = projection.revision();
    if context.expected_revision != actual {
        return Err(DomainError::StaleRevision {
            expected: context.expected_revision,
            actual,
        });
    }

    let sequence = if actual == 0 {
        Sequence::new(1)?
    } else {
        Sequence::new(actual)?.successor()?
    };

    let (candidate, kind) = match command {
        Command::Start { book_id } => {
            if registry.get(book_id).is_none() {
                return Err(DomainError::UnknownBook);
            }
            (book_id, EventKind::PlacementStarted)
        }
        Command::Answer { opponent, choice } => {
            if registry.get(opponent).is_none() {
                return Err(DomainError::UnknownBook);
            }
            let candidate = projection
                .pending()
                .ok_or(DomainError::InvalidTransition(
                    ReplayErrorReason::NoActivePlacement,
                ))?
                .candidate();
            (
                candidate,
                EventKind::ComparisonAnswered { opponent, choice },
            )
        }
        Command::Pause | Command::Resume => {
            let candidate = projection
                .pending()
                .ok_or(DomainError::InvalidTransition(
                    ReplayErrorReason::NoActivePlacement,
                ))?
                .candidate();
            let kind = match command {
                Command::Pause => EventKind::PlacementPaused,
                Command::Resume => EventKind::PlacementResumed,
                _ => unreachable!(),
            };
            (candidate, kind)
        }
    };
    let event = RankingEvent::new(
        EventMetadata {
            id: context.event_id,
            reader_id: projection.reader_id(),
            sequence,
            time: context.time,
        },
        candidate,
        kind,
    );
    projection
        .apply(&event)
        .map_err(|error| DomainError::InvalidTransition(*error.reason()))?;
    Ok(event)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ranking {
    history: Vec<RankingEvent>,
    projection: RankingProjection,
}

impl Ranking {
    /// Creates an empty ranking for a reader.
    ///
    /// # Panics
    /// Panics only if replaying an empty history violates the domain invariant.
    #[must_use]
    pub fn new(reader_id: ReaderId) -> Self {
        Self {
            history: Vec::new(),
            projection: RankingProjection::replay(reader_id, &[])
                .expect("empty ranking history is valid"),
        }
    }

    /// Rebuilds a ranking from its authoritative history.
    ///
    /// # Errors
    /// Returns a replay error at the first invalid event.
    pub fn from_history(
        reader_id: ReaderId,
        history: Vec<RankingEvent>,
    ) -> Result<Self, ReplayError> {
        let projection = RankingProjection::replay(reader_id, &history)?;
        Ok(Self {
            history,
            projection,
        })
    }

    /// Accepts one command and appends its event if valid.
    ///
    /// # Errors
    /// Returns a domain error without changing history for rejected commands.
    pub fn execute(
        &mut self,
        registry: &BookRegistry,
        context: CommandContext,
        command: Command,
    ) -> Result<RankingEvent, DomainError> {
        let event = decide(&self.projection, registry, context, command)?;
        let next = self
            .projection
            .apply(&event)
            .map_err(|error| DomainError::InvalidTransition(*error.reason()))?;
        self.history.push(event.clone());
        self.projection = next;
        Ok(event)
    }

    #[must_use]
    pub fn history(&self) -> &[RankingEvent] {
        &self.history
    }

    #[must_use]
    pub const fn projection(&self) -> &RankingProjection {
        &self.projection
    }
}
