mod card;
mod deck;
mod error;
mod id;
mod participant;
mod session;

pub use card::{CardSide, Flashcard};
pub use deck::Deck;
pub use error::FlippedError;
pub use id::{CardId, DeckId, ParticipantId, SessionId};
pub use participant::{
    Examiner, ExaminerParticipant, Participant, ParticipantRole, Role, TestTaker,
    TestTakerParticipant,
};
pub use session::{
    ActiveSession, AdvanceOutcome, AnySession, Completed, CompletedSession, Empty, EmptySession,
    ExaminerCardView, HasExaminer, HasTestTaker, InProgress, Ready, ReadySession, Session,
    SessionLifecycle, SessionStatus, SessionWithExaminer, SessionWithTestTaker, Terminated,
    TerminatedSession, TestTakerCardView,
};

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn card(front: &str, back: &str) -> Result<Flashcard> {
        match Flashcard::new(front, back) {
            Ok(card) => Ok(card),
            Err(error) => fail!("valid card; unexpected error: {:?}", error)
                .map(|()| unreachable!("fail! unexpectedly succeeded")),
        }
    }

    fn deck() -> Result<Deck> {
        let cards = vec![card("hola", "hello")?, card("adiós", "goodbye")?];
        match Deck::new(Some("Spanish basics".to_owned()), cards) {
            Ok(deck) => Ok(deck),
            Err(error) => fail!("valid deck; unexpected error: {:?}", error)
                .map(|()| unreachable!("fail! unexpectedly succeeded")),
        }
    }

    fn ready_session(
        examiner: ExaminerParticipant,
        test_taker: TestTakerParticipant,
    ) -> Result<ReadySession> {
        Ok(Session::new(deck()?)
            .join_examiner(examiner)
            .join_test_taker(test_taker))
    }

    fn active_session(
        examiner: ExaminerParticipant,
        test_taker: TestTakerParticipant,
    ) -> Result<ActiveSession> {
        match ready_session(examiner, test_taker)?.start(&examiner) {
            Ok(session) => Ok(session),
            Err(error) => fail!("examiner starts; unexpected error: {:?}", error)
                .map(|()| unreachable!("fail! unexpectedly succeeded")),
        }
    }

    #[test]
    fn validates_blank_card_sides() -> Result<()> {
        let error = match Flashcard::new("", "answer") {
            Ok(value) => return fail!("blank front should fail; unexpected value: {:?}", value),
            Err(error) => error,
        };
        verify_that!(error, eq(&FlippedError::BlankCardSide))?;
        let error = match Flashcard::new("front", "   ") {
            Ok(value) => return fail!("blank back should fail; unexpected value: {:?}", value),
            Err(error) => error,
        };
        verify_that!(error, eq(&FlippedError::BlankCardSide))?;
        Ok(())
    }

    #[test]
    fn validates_non_empty_decks() -> Result<()> {
        let error = match Deck::new(None, vec![]) {
            Ok(value) => return fail!("empty deck should fail; unexpected value: {:?}", value),
            Err(error) => error,
        };
        verify_that!(error, eq(&FlippedError::EmptyDeck))?;
        Ok(())
    }

    #[test]
    fn generated_ids_are_uuid_v7() -> Result<()> {
        verify_that!(SessionId::new().as_uuid().get_version_num(), eq(7))?;
        verify_that!(DeckId::new().as_uuid().get_version_num(), eq(7))?;
        verify_that!(CardId::new().as_uuid().get_version_num(), eq(7))?;
        verify_that!(ParticipantId::new().as_uuid().get_version_num(), eq(7))?;
        Ok(())
    }

    #[test]
    fn joins_participants_and_starts_when_examiner_commands() -> Result<()> {
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = Session::new(deck()?);

        verify_that!(session.status(), eq(SessionStatus::Empty))?;
        let session = session.join_examiner(examiner);
        verify_that!(session.status(), eq(SessionStatus::HasExaminer))?;
        let session = session.join_test_taker(test_taker);
        verify_that!(session.status(), eq(SessionStatus::Ready))?;

        let session = match session.start(&examiner) {
            Ok(session) => session,
            Err(error) => return fail!("examiner starts; unexpected error: {:?}", error),
        };
        verify_that!(
            session.status(),
            eq(SessionStatus::InProgress {
                current_card_index: 0
            })
        )?;
        Ok(())
    }

    #[test]
    fn joins_in_either_role_order() -> Result<()> {
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = Session::new(deck()?).join_test_taker(test_taker);
        verify_that!(session.status(), eq(SessionStatus::HasTestTaker))?;

        let session = session.join_examiner(examiner);
        verify_that!(session.examiner(), eq(&examiner))?;
        verify_that!(session.test_taker(), eq(&test_taker))?;
        verify_that!(session.status(), eq(SessionStatus::Ready))?;
        Ok(())
    }

    #[test]
    fn exposes_role_specific_card_views() -> Result<()> {
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = active_session(examiner, test_taker)?;

        let examiner_view = match session.examiner_view(&examiner) {
            Ok(view) => view,
            Err(error) => {
                return fail!("examiner sees current card; unexpected error: {:?}", error);
            }
        };
        verify_that!(examiner_view.front, eq("hola"))?;
        verify_that!(examiner_view.back, eq("hello"))?;
        verify_that!(examiner_view.position, eq(1))?;
        verify_that!(examiner_view.total, eq(2))?;

        let test_taker_view = match session.test_taker_view(&test_taker) {
            Ok(view) => view,
            Err(error) => {
                return fail!(
                    "test taker sees current card; unexpected error: {:?}",
                    error
                );
            }
        };
        verify_that!(test_taker_view.front, eq("hola"))?;
        verify_that!(test_taker_view.position, eq(1))?;
        verify_that!(test_taker_view.total, eq(2))?;
        Ok(())
    }

    #[test]
    fn advances_until_completed() -> Result<()> {
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = active_session(examiner, test_taker)?;

        let outcome = match session.advance(&examiner) {
            Ok(outcome) => outcome,
            Err(error) => return fail!("advance to second card; unexpected error: {:?}", error),
        };
        let session = match outcome {
            AdvanceOutcome::InProgress(session) => session,
            outcome => {
                return fail!(
                    "two-card deck should not complete yet; unexpected outcome: {:?}",
                    outcome
                );
            }
        };
        verify_that!(
            session.status(),
            eq(SessionStatus::InProgress {
                current_card_index: 1
            })
        )?;
        let second_card = match session.test_taker_view(&test_taker) {
            Ok(view) => view,
            Err(error) => return fail!("view second card; unexpected error: {:?}", error),
        };
        verify_that!(second_card.front, eq("adiós"))?;

        let outcome = match session.advance(&examiner) {
            Ok(outcome) => outcome,
            Err(error) => return fail!("complete after last card; unexpected error: {:?}", error),
        };
        let session = match outcome {
            AdvanceOutcome::Completed(session) => session,
            outcome => {
                return fail!(
                    "second advance should complete; unexpected outcome: {:?}",
                    outcome
                );
            }
        };
        verify_that!(session.status(), eq(SessionStatus::Completed))?;
        Ok(())
    }

    #[test]
    fn rejects_commands_from_unjoined_examiner() -> Result<()> {
        let joined_examiner = ExaminerParticipant::new(ParticipantId::new());
        let unjoined_examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = ready_session(joined_examiner, test_taker)?;

        let error = match session.start(&unjoined_examiner) {
            Ok(value) => {
                return fail!("unjoined examiner rejected; unexpected value: {:?}", value);
            }
            Err(error) => error,
        };
        verify_that!(error, eq(&FlippedError::UnknownParticipant))?;
        Ok(())
    }

    #[test]
    fn terminates_active_sessions() -> Result<()> {
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = active_session(examiner, test_taker)?;

        let session = match session.end(&examiner) {
            Ok(session) => session,
            Err(error) => return fail!("examiner ends; unexpected error: {:?}", error),
        };

        verify_that!(session.status(), eq(SessionStatus::Terminated))?;
        verify_that!(session.examiner(), eq(&examiner))?;
        verify_that!(session.test_taker(), eq(&test_taker))?;
        Ok(())
    }

    #[test]
    fn any_session_reports_dynamic_status() -> Result<()> {
        let session = AnySession::Empty(Session::new(deck()?));

        verify_that!(session.status(), eq(SessionStatus::Empty))?;
        Ok(())
    }
}
