use crate::{EventKind, EventMetadata, RankingEvent, RankingProjection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomaticPauseReason {
    NoEligibleOpponents,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainNotice {
    BookRanked {
        metadata: EventMetadata,
        candidate: crate::BookId,
        position: usize,
        entry_count: usize,
    },
    PlacementAutomaticallyPaused {
        metadata: EventMetadata,
        candidate: crate::BookId,
        reason: AutomaticPauseReason,
    },
}

impl DomainNotice {
    pub fn metadata(&self) -> &EventMetadata {
        match self {
            Self::BookRanked { metadata, .. }
            | Self::PlacementAutomaticallyPaused { metadata, .. } => metadata,
        }
    }

    pub fn candidate(&self) -> crate::BookId {
        match self {
            Self::BookRanked { candidate, .. }
            | Self::PlacementAutomaticallyPaused { candidate, .. } => *candidate,
        }
    }
}

pub fn derive_notices(
    before: &RankingProjection,
    event: &RankingEvent,
    after: &RankingProjection,
) -> Vec<DomainNotice> {
    if after.entries().len() > before.entries().len() {
        let position = after
            .entries()
            .iter()
            .position(|entry| entry.book_id() == event.candidate())
            .expect("valid completed transition contains candidate");
        return vec![DomainNotice::BookRanked {
            metadata: *event.metadata(),
            candidate: event.candidate(),
            position,
            entry_count: after.entries().len(),
        }];
    }
    if matches!(event.kind(), EventKind::ComparisonAnswered { .. })
        && before.pending().is_some_and(|pending| !pending.is_paused())
        && after.pending().is_some_and(|pending| pending.is_paused())
    {
        return vec![DomainNotice::PlacementAutomaticallyPaused {
            metadata: *event.metadata(),
            candidate: event.candidate(),
            reason: AutomaticPauseReason::NoEligibleOpponents,
        }];
    }
    Vec::new()
}
