use std::collections::HashSet;

use chrono::{DateTime, Utc};

use crate::{
    BookId, ComparisonChoice, EventId, EventKind, RankingEvent, ReaderId, ReplayError,
    ReplayErrorReason,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankingEntry {
    book_id: BookId,
    added_at: DateTime<Utc>,
}

impl RankingEntry {
    #[must_use]
    pub const fn book_id(&self) -> BookId {
        self.book_id
    }

    #[must_use]
    pub const fn added_at(&self) -> DateTime<Utc> {
        self.added_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementSession {
    candidate: BookId,
    lo: usize,
    hi: usize,
    skipped: HashSet<BookId>,
    paused: bool,
    started_at: DateTime<Utc>,
    last_activity_at: DateTime<Utc>,
}

impl PlacementSession {
    #[must_use]
    pub const fn candidate(&self) -> BookId {
        self.candidate
    }

    #[must_use]
    pub const fn bounds(&self) -> (usize, usize) {
        (self.lo, self.hi)
    }

    #[must_use]
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    #[must_use]
    pub const fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    #[must_use]
    pub const fn last_activity_at(&self) -> DateTime<Utc> {
        self.last_activity_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankingProjection {
    reader_id: ReaderId,
    revision: u64,
    entries: Vec<RankingEntry>,
    pending: Option<PlacementSession>,
    seen_event_ids: HashSet<EventId>,
}

impl RankingProjection {
    /// Reconstructs current state from an ordered authoritative history.
    ///
    /// # Errors
    /// Returns a replay error at the first invalid event.
    pub fn replay(reader_id: ReaderId, events: &[RankingEvent]) -> Result<Self, ReplayError> {
        let mut projection = Self {
            reader_id,
            revision: 0,
            entries: Vec::new(),
            pending: None,
            seen_event_ids: HashSet::new(),
        };
        for event in events {
            projection = projection.apply(event)?;
        }
        Ok(projection)
    }

    /// Validates and applies one event, leaving this projection unchanged.
    ///
    /// # Errors
    /// Returns a replay error for an invalid sequence, identity, pair, or transition.
    pub fn apply(&self, event: &RankingEvent) -> Result<Self, ReplayError> {
        let sequence = event.metadata().sequence.value();
        let invalid = |reason| ReplayError::new(sequence, reason);
        if event.metadata().reader_id != self.reader_id {
            return Err(invalid(ReplayErrorReason::WrongReader));
        }
        let expected = self
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid(ReplayErrorReason::SequenceOverflow))?;
        if sequence != expected {
            return Err(invalid(ReplayErrorReason::SequenceMismatch { expected }));
        }
        if self.seen_event_ids.contains(&event.metadata().id) {
            return Err(invalid(ReplayErrorReason::DuplicateEventId));
        }

        let mut next = self.clone();
        next.apply_kind(event).map_err(invalid)?;
        next.revision = sequence;
        next.seen_event_ids.insert(event.metadata().id);
        Ok(next)
    }

    fn apply_kind(&mut self, event: &RankingEvent) -> Result<(), ReplayErrorReason> {
        match event.kind() {
            EventKind::PlacementStarted => {
                if self
                    .entries
                    .iter()
                    .any(|entry| entry.book_id == event.candidate())
                {
                    return Err(ReplayErrorReason::DuplicateBook);
                }
                if self.pending.is_some() {
                    return Err(ReplayErrorReason::PendingPlacement);
                }
                if self.entries.is_empty() {
                    self.entries.push(RankingEntry {
                        book_id: event.candidate(),
                        added_at: event.metadata().time,
                    });
                } else {
                    self.pending = Some(PlacementSession {
                        candidate: event.candidate(),
                        lo: 0,
                        hi: self.entries.len(),
                        skipped: HashSet::new(),
                        paused: false,
                        started_at: event.metadata().time,
                        last_activity_at: event.metadata().time,
                    });
                }
            }
            EventKind::ComparisonAnswered { opponent, choice } => {
                let selected = self.next_opponent();
                let pending = self
                    .pending
                    .as_mut()
                    .ok_or(ReplayErrorReason::NoActivePlacement)?;
                if pending.candidate != event.candidate() {
                    return Err(ReplayErrorReason::WrongCandidate);
                }
                if pending.paused {
                    return Err(ReplayErrorReason::PlacementPaused);
                }
                if selected != Some(*opponent) {
                    return Err(ReplayErrorReason::WrongOpponent);
                }
                let j = self
                    .entries
                    .iter()
                    .position(|entry| entry.book_id == *opponent)
                    .expect("selected opponent is ranked");
                match choice {
                    ComparisonChoice::PreferCandidate => pending.hi = j,
                    ComparisonChoice::PreferOpponent => pending.lo = j + 1,
                    ComparisonChoice::Skip => {
                        pending.skipped.insert(*opponent);
                    }
                }
                pending.last_activity_at = event.metadata().time;
                if pending.lo == pending.hi {
                    let insertion = pending.lo;
                    self.entries.insert(
                        insertion,
                        RankingEntry {
                            book_id: event.candidate(),
                            added_at: event.metadata().time,
                        },
                    );
                    self.pending = None;
                } else if self.next_opponent().is_none() {
                    self.pending.as_mut().expect("pending placement").paused = true;
                }
            }
            EventKind::PlacementPaused | EventKind::PlacementResumed => {
                let pending = self
                    .pending
                    .as_mut()
                    .ok_or(ReplayErrorReason::NoActivePlacement)?;
                if pending.candidate != event.candidate() {
                    return Err(ReplayErrorReason::WrongCandidate);
                }
                match event.kind() {
                    EventKind::PlacementPaused if pending.paused => {
                        return Err(ReplayErrorReason::PlacementPaused);
                    }
                    EventKind::PlacementPaused => pending.paused = true,
                    EventKind::PlacementResumed if !pending.paused => {
                        return Err(ReplayErrorReason::NotPaused);
                    }
                    EventKind::PlacementResumed => {
                        pending.paused = false;
                        pending.skipped.clear();
                    }
                    _ => unreachable!(),
                }
                pending.last_activity_at = event.metadata().time;
            }
        }
        Ok(())
    }

    #[must_use]
    pub const fn reader_id(&self) -> ReaderId {
        self.reader_id
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn entries(&self) -> &[RankingEntry] {
        &self.entries
    }

    #[must_use]
    pub const fn pending(&self) -> Option<&PlacementSession> {
        self.pending.as_ref()
    }

    #[must_use]
    pub fn next_opponent(&self) -> Option<BookId> {
        let pending = self.pending.as_ref()?;
        if pending.paused {
            return None;
        }
        let midpoint = pending.lo + (pending.hi - pending.lo - 1) / 2;
        (pending.lo..pending.hi)
            .filter(|&j| !pending.skipped.contains(&self.entries[j].book_id))
            .min_by_key(|&j| (j.abs_diff(midpoint), j))
            .map(|j| self.entries[j].book_id)
    }
}
