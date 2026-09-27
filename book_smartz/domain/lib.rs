pub mod command;
pub mod error;
pub mod event;
pub mod identity;
pub mod projection;

#[cfg(test)]
mod command_tests;

pub use command::{Command, CommandContext, Ranking, decide};
pub use error::{DomainError, IdentityError, ReplayError, ReplayErrorReason};
pub use event::{ComparisonChoice, EventKind, EventMetadata, RankingEvent, Sequence};
pub use identity::{Book, BookId, BookRegistry, EventId, OpenLibraryWorkId, ReaderId};
pub use projection::{PlacementSession, RankingEntry, RankingProjection};

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::{
        Book, BookId, BookRegistry, ComparisonChoice, DomainError, EventId, EventKind,
        EventMetadata, IdentityError, OpenLibraryWorkId, RankingEvent, RankingProjection, ReaderId,
        ReplayErrorReason, Sequence,
    };

    fn reader() -> ReaderId {
        ReaderId::new(Uuid::from_u128(100))
    }

    fn at(second: u32) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&format!("2026-09-27T12:00:{second:02}Z"))
            .expect("valid time")
            .with_timezone(&Utc)
    }

    fn event(sequence: u64, candidate: u128, kind: EventKind, second: u32) -> RankingEvent {
        event_with(sequence, sequence, reader(), candidate, kind, second)
    }

    fn event_with(
        sequence: u64,
        id: u64,
        reader_id: ReaderId,
        candidate: u128,
        kind: EventKind,
        second: u32,
    ) -> RankingEvent {
        RankingEvent::new(
            EventMetadata {
                id: EventId::new(Uuid::from_u128(id.into())),
                reader_id,
                sequence: Sequence::new(sequence).expect("positive sequence"),
                time: at(second),
            },
            BookId::new(Uuid::from_u128(candidate)),
            kind,
        )
    }

    fn start(sequence: u64, candidate: u128, second: u32) -> RankingEvent {
        event(sequence, candidate, EventKind::PlacementStarted, second)
    }

    fn answer(
        sequence: u64,
        candidate: u128,
        opponent: u128,
        choice: ComparisonChoice,
        second: u32,
    ) -> RankingEvent {
        event(
            sequence,
            candidate,
            EventKind::ComparisonAnswered {
                opponent: BookId::new(Uuid::from_u128(opponent)),
                choice,
            },
            second,
        )
    }

    #[googletest::test]
    fn first_start_places_book() {
        let projection = RankingProjection::replay(reader(), &[start(1, 1, 1)]).unwrap();
        assert_that!(projection.revision(), eq(1));
        assert_that!(projection.entries().len(), eq(1));
        assert_that!(
            projection.entries()[0].book_id(),
            eq(BookId::new(Uuid::from_u128(1)))
        );
        assert_that!(projection.entries()[0].added_at(), eq(at(1)));
        assert_that!(projection.pending().is_none(), eq(true));
    }

    #[googletest::test]
    fn comparison_history_derives_order() {
        let events = [
            start(1, 1, 1),
            start(2, 2, 2),
            answer(3, 2, 1, ComparisonChoice::PreferCandidate, 3),
        ];
        let projection = RankingProjection::replay(reader(), &events).unwrap();
        assert_that!(projection.revision(), eq(3));
        assert_that!(
            projection
                .entries()
                .iter()
                .map(|entry| entry.book_id())
                .collect::<Vec<_>>(),
            eq(&vec![
                BookId::new(Uuid::from_u128(2)),
                BookId::new(Uuid::from_u128(1))
            ])
        );
        assert_that!(projection.entries()[0].added_at(), eq(at(3)));
        assert_that!(projection.entries()[1].added_at(), eq(at(1)));
        assert_that!(projection.pending().is_none(), eq(true));
    }

    #[googletest::test]
    fn replay_equals_incremental_application() {
        let events = [
            start(1, 1, 1),
            start(2, 2, 2),
            answer(3, 2, 1, ComparisonChoice::PreferOpponent, 3),
            start(4, 3, 4),
            answer(5, 3, 1, ComparisonChoice::Skip, 5),
            answer(6, 3, 2, ComparisonChoice::Skip, 6),
            event(7, 3, EventKind::PlacementResumed, 7),
        ];
        for count in [0, 2, 3, 4, 6, 7] {
            let replayed = RankingProjection::replay(reader(), &events[..count]).unwrap();
            let mut incremental = RankingProjection::replay(reader(), &[]).unwrap();
            for event in &events[..count] {
                incremental = incremental.apply(event).unwrap();
            }
            assert_that!(replayed, eq(&incremental));
        }
        let pending = RankingProjection::replay(reader(), &events).unwrap();
        assert_that!(pending.pending().unwrap().bounds(), eq((0, 2)));
        assert_that!(pending.pending().unwrap().started_at(), eq(at(4)));
        assert_that!(pending.pending().unwrap().last_activity_at(), eq(at(7)));
        assert_that!(
            pending.next_opponent(),
            eq(Some(BookId::new(Uuid::from_u128(1))))
        );
    }

    #[googletest::test]
    fn invalid_history_is_rejected() {
        let wrong_reader = ReaderId::new(Uuid::from_u128(200));
        let cases = [
            (
                vec![start(2, 1, 1)],
                2,
                ReplayErrorReason::SequenceMismatch { expected: 1 },
            ),
            (
                vec![start(1, 1, 1), start(1, 2, 2)],
                1,
                ReplayErrorReason::SequenceMismatch { expected: 2 },
            ),
            (
                vec![start(1, 1, 1), start(3, 2, 2)],
                3,
                ReplayErrorReason::SequenceMismatch { expected: 2 },
            ),
            (
                vec![
                    start(1, 1, 1),
                    event_with(2, 1, reader(), 2, EventKind::PlacementStarted, 2),
                ],
                2,
                ReplayErrorReason::DuplicateEventId,
            ),
            (
                vec![event_with(
                    1,
                    1,
                    wrong_reader,
                    1,
                    EventKind::PlacementStarted,
                    1,
                )],
                1,
                ReplayErrorReason::WrongReader,
            ),
            (
                vec![start(1, 1, 1), start(2, 1, 2)],
                2,
                ReplayErrorReason::DuplicateBook,
            ),
            (
                vec![start(1, 1, 1), start(2, 2, 2), start(3, 3, 3)],
                3,
                ReplayErrorReason::PendingPlacement,
            ),
            (
                vec![start(1, 1, 1), answer(2, 2, 1, ComparisonChoice::Skip, 2)],
                2,
                ReplayErrorReason::NoActivePlacement,
            ),
            (
                vec![
                    start(1, 1, 1),
                    start(2, 2, 2),
                    answer(3, 3, 1, ComparisonChoice::Skip, 3),
                ],
                3,
                ReplayErrorReason::WrongCandidate,
            ),
            (
                vec![
                    start(1, 1, 1),
                    start(2, 2, 2),
                    answer(3, 2, 3, ComparisonChoice::Skip, 3),
                ],
                3,
                ReplayErrorReason::WrongOpponent,
            ),
            (
                vec![
                    start(1, 1, 1),
                    start(2, 2, 2),
                    event(3, 2, EventKind::PlacementPaused, 3),
                    answer(4, 2, 1, ComparisonChoice::Skip, 4),
                ],
                4,
                ReplayErrorReason::PlacementPaused,
            ),
            (
                vec![
                    start(1, 1, 1),
                    start(2, 2, 2),
                    event(3, 2, EventKind::PlacementResumed, 3),
                ],
                3,
                ReplayErrorReason::NotPaused,
            ),
            (
                vec![
                    start(1, 1, 1),
                    start(2, 2, 2),
                    event(3, 2, EventKind::PlacementPaused, 3),
                    event(4, 2, EventKind::PlacementPaused, 4),
                ],
                4,
                ReplayErrorReason::PlacementPaused,
            ),
        ];
        for (events, sequence, reason) in cases {
            let error = RankingProjection::replay(reader(), &events).unwrap_err();
            assert_that!(error.attempted_sequence(), eq(sequence));
            assert_that!(error.reason(), eq(&reason));
        }
    }

    #[googletest::test]
    fn timestamps_do_not_order_decisions() {
        let events = [
            start(1, 1, 3),
            start(2, 2, 3),
            answer(3, 2, 1, ComparisonChoice::PreferCandidate, 1),
        ];
        let projection = RankingProjection::replay(reader(), &events).unwrap();
        assert_that!(
            projection
                .entries()
                .iter()
                .map(|entry| entry.book_id())
                .collect::<Vec<_>>(),
            eq(&vec![
                BookId::new(Uuid::from_u128(2)),
                BookId::new(Uuid::from_u128(1))
            ])
        );
        assert_that!(projection.entries()[0].added_at(), eq(at(1)));
        assert_that!(projection.entries()[1].added_at(), eq(at(3)));
    }

    #[googletest::test]
    fn sequence_successor_rejects_overflow() {
        assert_that!(Sequence::new(0), eq(Err(DomainError::InvalidSequence)));
        assert_that!(
            Sequence::new(u64::MAX).unwrap().successor(),
            eq(Err(DomainError::SequenceOverflow))
        );
    }

    #[googletest::test]
    fn midpoint_tie_break_and_narrowed_bounds_preserve_order() {
        let events = [
            start(1, 1, 1),
            start(2, 2, 2),
            answer(3, 2, 1, ComparisonChoice::PreferOpponent, 3),
            start(4, 3, 4),
            answer(5, 3, 1, ComparisonChoice::PreferOpponent, 5),
            answer(6, 3, 2, ComparisonChoice::PreferOpponent, 6),
            start(7, 4, 7),
            answer(8, 4, 2, ComparisonChoice::PreferOpponent, 8),
            answer(9, 4, 3, ComparisonChoice::PreferOpponent, 9),
            start(10, 5, 10),
            answer(11, 5, 2, ComparisonChoice::PreferOpponent, 11),
            answer(12, 5, 3, ComparisonChoice::PreferOpponent, 12),
            answer(13, 5, 4, ComparisonChoice::PreferOpponent, 13),
            start(14, 6, 14),
        ];
        let ranked = RankingProjection::replay(reader(), &events).unwrap();
        assert_that!(
            ranked.next_opponent(),
            eq(Some(BookId::new(Uuid::from_u128(3))))
        );
        let skipped = ranked
            .apply(&answer(15, 6, 3, ComparisonChoice::Skip, 15))
            .unwrap();
        assert_that!(
            skipped.next_opponent(),
            eq(Some(BookId::new(Uuid::from_u128(2))))
        );
        let narrowed = skipped
            .apply(&answer(16, 6, 2, ComparisonChoice::PreferCandidate, 16))
            .unwrap();
        assert_that!(narrowed.pending().unwrap().bounds(), eq((0, 1)));
        assert_that!(
            narrowed.next_opponent(),
            eq(Some(BookId::new(Uuid::from_u128(1))))
        );
        let complete = narrowed
            .apply(&answer(17, 6, 1, ComparisonChoice::PreferOpponent, 17))
            .unwrap();
        assert_that!(
            complete
                .entries()
                .iter()
                .map(|entry| entry.book_id())
                .collect::<Vec<_>>(),
            eq(&vec![
                BookId::new(Uuid::from_u128(1)),
                BookId::new(Uuid::from_u128(6)),
                BookId::new(Uuid::from_u128(2)),
                BookId::new(Uuid::from_u128(3)),
                BookId::new(Uuid::from_u128(4)),
                BookId::new(Uuid::from_u128(5)),
            ])
        );
        assert_that!(complete.entries()[1].added_at(), eq(at(17)));
        assert_that!(complete.pending().is_none(), eq(true));
    }

    #[googletest::test]
    fn explicit_pause_resume_preserves_bounds_and_times() {
        let events = [
            start(1, 1, 1),
            start(2, 2, 2),
            event(3, 2, EventKind::PlacementPaused, 3),
        ];
        let paused = RankingProjection::replay(reader(), &events).unwrap();
        assert_that!(paused.pending().unwrap().is_paused(), eq(true));
        assert_that!(paused.next_opponent(), eq(None));
        let resumed = paused
            .apply(&event(4, 2, EventKind::PlacementResumed, 4))
            .unwrap();
        assert_that!(resumed.pending().unwrap().bounds(), eq((0, 1)));
        assert_that!(resumed.pending().unwrap().started_at(), eq(at(2)));
        assert_that!(resumed.pending().unwrap().last_activity_at(), eq(at(4)));
        assert_that!(
            resumed.next_opponent(),
            eq(Some(BookId::new(Uuid::from_u128(1))))
        );
    }

    fn book(id: u128, work_id: &str) -> Book {
        Book::new(
            BookId::new(Uuid::from_u128(id)),
            OpenLibraryWorkId::try_from(work_id).expect("valid work ID"),
        )
    }

    #[googletest::test]
    fn register_book_is_idempotent() {
        let mut registry = BookRegistry::new();
        let original = book(1, "OL45804W");

        assert_that!(registry.register(original.clone()), eq(&Ok(())));
        assert_that!(registry.register(original.clone()), eq(&Ok(())));
        assert_that!(registry.get(original.id()), eq(Some(&original)));
    }

    #[googletest::test]
    fn registration_rejects_conflicting_identity() {
        let mut registry = BookRegistry::new();
        let original = book(1, "OL45804W");
        registry
            .register(original.clone())
            .expect("first registration");

        assert_that!(
            registry.register(book(2, "OL45804W")),
            eq(&Err(IdentityError::WorkIdConflict))
        );
        assert_that!(
            registry.register(book(1, "OL45805W")),
            eq(&Err(IdentityError::BookIdConflict))
        );
        assert_that!(registry.get(original.id()), eq(Some(&original)));
        assert_that!(registry.get(BookId::new(Uuid::from_u128(2))), eq(None));
    }

    #[googletest::test]
    fn work_id_requires_canonical_work_form() {
        let valid = OpenLibraryWorkId::try_from("OL45804W").expect("canonical work ID");
        assert_that!(valid.as_str(), eq("OL45804W"));

        for invalid in [
            "OL7353617M",
            "/works/OL45804W",
            "OLW",
            "OL0W",
            "OL045804W",
            "OL٤٥٨٠٤W",
        ] {
            assert_that!(OpenLibraryWorkId::try_from(invalid).is_err(), eq(true));
        }
    }
}
