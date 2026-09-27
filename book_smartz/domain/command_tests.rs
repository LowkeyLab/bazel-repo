use std::collections::HashSet;

use chrono::{DateTime, Utc};
use googletest::prelude::*;
use uuid::Uuid;

use crate::{
    Book, BookId, BookRegistry, Command, CommandContext, ComparisonChoice, DomainError, EventId,
    OpenLibraryWorkId, Ranking, RankingProjection, ReaderId, ReplayErrorReason, Sequence, decide,
};

fn reader() -> ReaderId {
    ReaderId::new(Uuid::from_u128(900))
}

fn book(id: u128) -> BookId {
    BookId::new(Uuid::from_u128(id))
}

fn time(tick: u64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000 + tick as i64, 0).expect("valid time")
}

fn registry(ids: impl IntoIterator<Item = u128>) -> BookRegistry {
    let mut books = BookRegistry::new();
    for id in ids {
        books
            .register(Book::new(
                book(id),
                OpenLibraryWorkId::try_from(format!("OL{id}W").as_str()).unwrap(),
            ))
            .unwrap();
    }
    books
}

fn context(ranking: &Ranking) -> CommandContext {
    let next = ranking.projection().revision() + 1;
    CommandContext {
        expected_revision: ranking.projection().revision(),
        event_id: EventId::new(Uuid::from_u128(next.into())),
        time: time(next),
    }
}

fn execute(ranking: &mut Ranking, books: &BookRegistry, command: Command) {
    let ctx = context(ranking);
    let before = ranking.history().len();
    let event = ranking.execute(books, ctx, command).unwrap();
    assert_that!(ranking.history().len(), eq(before + 1));
    assert_that!(ranking.history().last(), eq(Some(&event)));
}

fn ids(ranking: &Ranking) -> Vec<BookId> {
    ranking
        .projection()
        .entries()
        .iter()
        .map(|entry| entry.book_id())
        .collect()
}

fn append_at_bottom(ranking: &mut Ranking, books: &BookRegistry, id: u128) {
    execute(ranking, books, Command::Start { book_id: book(id) });
    while let Some(opponent) = ranking.projection().next_opponent() {
        execute(
            ranking,
            books,
            Command::Answer {
                opponent,
                choice: ComparisonChoice::PreferOpponent,
            },
        );
    }
}

#[googletest::test]
fn rejected_command_keeps_history_and_projection() {
    let books = registry([1, 2, 3]);
    let mut ranking = Ranking::new(reader());
    let ctx = context(&ranking);
    let unknown = ranking.execute(&books, ctx, Command::Start { book_id: book(99) });
    assert_that!(unknown, eq(&Err(DomainError::UnknownBook)));
    assert_that!(ranking.history().len(), eq(0));

    execute(&mut ranking, &books, Command::Start { book_id: book(1) });
    let stale = CommandContext {
        expected_revision: 0,
        event_id: EventId::new(Uuid::from_u128(20)),
        time: time(20),
    };
    let duplicate_id = CommandContext {
        expected_revision: 1,
        event_id: ranking.history()[0].metadata().id,
        time: time(21),
    };
    let checks = [
        (
            stale,
            Command::Start { book_id: book(2) },
            DomainError::StaleRevision {
                expected: 0,
                actual: 1,
            },
        ),
        (
            duplicate_id,
            Command::Start { book_id: book(2) },
            DomainError::InvalidTransition(ReplayErrorReason::DuplicateEventId),
        ),
        (
            context(&ranking),
            Command::Start { book_id: book(1) },
            DomainError::InvalidTransition(ReplayErrorReason::DuplicateBook),
        ),
        (
            context(&ranking),
            Command::Pause,
            DomainError::InvalidTransition(ReplayErrorReason::NoActivePlacement),
        ),
        (
            context(&ranking),
            Command::Resume,
            DomainError::InvalidTransition(ReplayErrorReason::NoActivePlacement),
        ),
    ];
    for (ctx, command, expected) in checks {
        let before_history = ranking.history().to_vec();
        let before_projection = ranking.projection().clone();
        assert_that!(ranking.execute(&books, ctx, command), eq(&Err(expected)));
        assert_that!(ranking.history(), eq(before_history.as_slice()));
        assert_that!(ranking.projection(), eq(&before_projection));
    }

    execute(&mut ranking, &books, Command::Start { book_id: book(2) });
    let checks = [
        (
            Command::Start { book_id: book(3) },
            ReplayErrorReason::PendingPlacement,
        ),
        (
            Command::Answer {
                opponent: book(3),
                choice: ComparisonChoice::Skip,
            },
            ReplayErrorReason::WrongOpponent,
        ),
        (Command::Resume, ReplayErrorReason::NotPaused),
    ];
    for (command, reason) in checks {
        let before_history = ranking.history().to_vec();
        let before_projection = ranking.projection().clone();
        let ctx = context(&ranking);
        assert_that!(
            ranking.execute(&books, ctx, command),
            eq(&Err(DomainError::InvalidTransition(reason)))
        );
        assert_that!(ranking.history(), eq(before_history.as_slice()));
        assert_that!(ranking.projection(), eq(&before_projection));
    }
    let before_history = ranking.history().to_vec();
    let before_projection = ranking.projection().clone();
    let ctx = context(&ranking);
    assert_that!(
        ranking.execute(
            &books,
            ctx,
            Command::Answer {
                opponent: book(99),
                choice: ComparisonChoice::Skip,
            },
        ),
        eq(&Err(DomainError::UnknownBook))
    );
    assert_that!(ranking.history(), eq(before_history.as_slice()));
    assert_that!(ranking.projection(), eq(&before_projection));
    execute(&mut ranking, &books, Command::Pause);
    let before_history = ranking.history().to_vec();
    let before_projection = ranking.projection().clone();
    let ctx = context(&ranking);
    assert_that!(
        ranking.execute(&books, ctx, Command::Pause),
        eq(&Err(DomainError::InvalidTransition(
            ReplayErrorReason::PlacementPaused
        )))
    );
    assert_that!(ranking.history(), eq(before_history.as_slice()));
    assert_that!(ranking.projection(), eq(&before_projection));
}

#[googletest::test]
fn skip_changes_opponent_without_implying_preference() {
    let books = registry(1..=6);
    let mut ranking = Ranking::new(reader());
    for id in 1..=5 {
        append_at_bottom(&mut ranking, &books, id);
    }
    execute(&mut ranking, &books, Command::Start { book_id: book(6) });
    assert_that!(ranking.projection().next_opponent(), eq(Some(book(3))));
    let bounds = ranking.projection().pending().unwrap().bounds();
    execute(
        &mut ranking,
        &books,
        Command::Answer {
            opponent: book(3),
            choice: ComparisonChoice::Skip,
        },
    );
    assert_that!(ranking.projection().pending().unwrap().bounds(), eq(bounds));
    assert_that!(ranking.projection().next_opponent(), eq(Some(book(2))));
    execute(
        &mut ranking,
        &books,
        Command::Answer {
            opponent: book(2),
            choice: ComparisonChoice::PreferCandidate,
        },
    );
    assert_that!(ranking.projection().next_opponent(), eq(Some(book(1))));
    assert_that!(
        ids(&ranking),
        eq(&vec![book(1), book(2), book(3), book(4), book(5)])
    );
}

#[googletest::test]
fn all_useful_opponents_skipped_pauses() {
    let books = registry([1, 2]);
    let mut ranking = Ranking::new(reader());
    execute(&mut ranking, &books, Command::Start { book_id: book(1) });
    execute(&mut ranking, &books, Command::Start { book_id: book(2) });
    let started = ranking.projection().pending().unwrap().started_at();
    execute(
        &mut ranking,
        &books,
        Command::Answer {
            opponent: book(1),
            choice: ComparisonChoice::Skip,
        },
    );
    assert_that!(
        ranking.projection().pending().unwrap().is_paused(),
        eq(true)
    );
    assert_that!(ranking.projection().pending().unwrap().bounds(), eq((0, 1)));
    assert_that!(ids(&ranking), eq(&vec![book(1)]));
    execute(&mut ranking, &books, Command::Resume);
    assert_that!(ranking.projection().next_opponent(), eq(Some(book(1))));
    assert_that!(ranking.projection().pending().unwrap().bounds(), eq((0, 1)));
    assert_that!(
        ranking.projection().pending().unwrap().started_at(),
        eq(started)
    );
}

#[googletest::test]
fn explicit_pause_preserves_winning_decisions() {
    let books = registry(1..=4);
    let mut ranking = Ranking::new(reader());
    for id in 1..=3 {
        append_at_bottom(&mut ranking, &books, id);
    }
    let original_times: Vec<_> = ranking
        .projection()
        .entries()
        .iter()
        .map(|entry| entry.added_at())
        .collect();
    execute(&mut ranking, &books, Command::Start { book_id: book(4) });
    let started = ranking.projection().pending().unwrap().started_at();
    let opponent = ranking.projection().next_opponent().unwrap();
    execute(
        &mut ranking,
        &books,
        Command::Answer {
            opponent,
            choice: ComparisonChoice::PreferOpponent,
        },
    );
    let bounds = ranking.projection().pending().unwrap().bounds();
    execute(&mut ranking, &books, Command::Pause);
    assert_that!(ranking.projection().next_opponent(), eq(None));
    execute(&mut ranking, &books, Command::Resume);
    assert_that!(ranking.projection().pending().unwrap().bounds(), eq(bounds));
    assert_that!(
        ranking.projection().pending().unwrap().started_at(),
        eq(started)
    );
    assert_that!(
        ranking
            .projection()
            .entries()
            .iter()
            .map(|entry| entry.added_at())
            .collect::<Vec<_>>(),
        eq(&original_times)
    );
    assert_that!(ranking.projection().next_opponent(), eq(Some(book(3))));
}

#[googletest::test]
fn all_small_insertions_preserve_order() {
    for length in 0..=8_u128 {
        for gap in 0..=length {
            let candidate = 100;
            let books = registry((1..=length).chain([candidate]));
            let mut ranking = Ranking::new(reader());
            for id in 1..=length {
                append_at_bottom(&mut ranking, &books, id);
            }
            execute(
                &mut ranking,
                &books,
                Command::Start {
                    book_id: book(candidate),
                },
            );
            let mut answers = 0;
            while let Some(opponent) = ranking.projection().next_opponent() {
                let index = (1..=length).position(|id| book(id) == opponent).unwrap();
                let choice = if gap as usize <= index {
                    ComparisonChoice::PreferCandidate
                } else {
                    ComparisonChoice::PreferOpponent
                };
                execute(&mut ranking, &books, Command::Answer { opponent, choice });
                answers += 1;
                assert_that!(answers <= length, eq(true));
            }
            let mut expected: Vec<_> = (1..=length).map(book).collect();
            expected.insert(gap as usize, book(candidate));
            let actual = ids(&ranking);
            assert_that!(actual, eq(&expected));
            assert_that!(
                actual.iter().copied().collect::<HashSet<_>>().len(),
                eq(length as usize + 1)
            );
            assert_that!(
                actual.iter().position(|id| *id == book(candidate)),
                eq(Some(gap as usize))
            );
            assert_that!(ranking.projection().pending().is_none(), eq(true));
        }
    }
}

#[googletest::test]
fn revision_increment_rejects_overflow() {
    assert_that!(
        Sequence::new(u64::MAX).unwrap().successor(),
        eq(Err(DomainError::SequenceOverflow))
    );
}

#[googletest::test]
fn decide_is_pure_and_history_replays() {
    let books = registry([1, 2]);
    let mut ranking = Ranking::new(reader());
    let initial = ranking.projection().clone();
    let ctx = context(&ranking);
    let proposed = decide(
        ranking.projection(),
        &books,
        ctx,
        Command::Start { book_id: book(1) },
    )
    .unwrap();
    assert_that!(ranking.projection(), eq(&initial));
    assert_that!(ranking.history().len(), eq(0));
    assert_that!(proposed.metadata().id, eq(ctx.event_id));
    assert_that!(proposed.metadata().time, eq(ctx.time));
    execute(&mut ranking, &books, Command::Start { book_id: book(1) });
    execute(&mut ranking, &books, Command::Start { book_id: book(2) });
    let restored = Ranking::from_history(reader(), ranking.history().to_vec()).unwrap();
    assert_that!(restored.history(), eq(ranking.history()));
    assert_that!(restored.projection(), eq(ranking.projection()));
    assert_that!(
        restored.projection().pending().unwrap().started_at(),
        eq(time(2))
    );
    assert_that!(
        RankingProjection::replay(reader(), ranking.history()).unwrap(),
        eq(ranking.projection())
    );
}
