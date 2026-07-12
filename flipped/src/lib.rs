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
    use super::*;

    fn card(front: &str, back: &str) -> Flashcard {
        Flashcard::new(front, back).expect("valid card")
    }

    fn deck() -> Deck {
        Deck::new(
            Some("Spanish basics".to_owned()),
            vec![card("hola", "hello"), card("adiós", "goodbye")],
        )
        .expect("valid deck")
    }

    fn ready_session(
        examiner: ExaminerParticipant,
        test_taker: TestTakerParticipant,
    ) -> ReadySession {
        Session::new(deck())
            .join_examiner(examiner)
            .join_test_taker(test_taker)
    }

    fn active_session(
        examiner: ExaminerParticipant,
        test_taker: TestTakerParticipant,
    ) -> ActiveSession {
        ready_session(examiner, test_taker)
            .start(&examiner)
            .expect("examiner starts")
    }

    #[test]
    fn validates_blank_card_sides() {
        assert_eq!(
            Flashcard::new("", "answer").expect_err("blank front should fail"),
            FlippedError::BlankCardSide
        );
        assert_eq!(
            Flashcard::new("front", "   ").expect_err("blank back should fail"),
            FlippedError::BlankCardSide
        );
    }

    #[test]
    fn validates_non_empty_decks() {
        assert_eq!(
            Deck::new(None, vec![]).expect_err("empty deck should fail"),
            FlippedError::EmptyDeck
        );
    }

    #[test]
    fn generated_ids_are_uuid_v7() {
        assert_eq!(SessionId::new().as_uuid().get_version_num(), 7);
        assert_eq!(DeckId::new().as_uuid().get_version_num(), 7);
        assert_eq!(CardId::new().as_uuid().get_version_num(), 7);
        assert_eq!(ParticipantId::new().as_uuid().get_version_num(), 7);
    }

    #[test]
    fn joins_participants_and_starts_when_examiner_commands() {
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = Session::new(deck());

        assert_eq!(session.status(), SessionStatus::Empty);
        let session = session.join_examiner(examiner);
        assert_eq!(session.status(), SessionStatus::HasExaminer);
        let session = session.join_test_taker(test_taker);
        assert_eq!(session.status(), SessionStatus::Ready);

        let session = session.start(&examiner).expect("examiner starts");
        assert_eq!(
            session.status(),
            SessionStatus::InProgress {
                current_card_index: 0
            }
        );
    }

    #[test]
    fn joins_in_either_role_order() {
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());

        let session = Session::new(deck()).join_test_taker(test_taker);
        assert_eq!(session.status(), SessionStatus::HasTestTaker);

        let session = session.join_examiner(examiner);
        assert_eq!(session.examiner(), &examiner);
        assert_eq!(session.test_taker(), &test_taker);
        assert_eq!(session.status(), SessionStatus::Ready);
    }

    #[test]
    fn exposes_role_specific_card_views() {
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = active_session(examiner, test_taker);

        let examiner_view = session
            .examiner_view(&examiner)
            .expect("examiner sees current card");
        assert_eq!(examiner_view.front, "hola");
        assert_eq!(examiner_view.back, "hello");
        assert_eq!(examiner_view.position, 1);
        assert_eq!(examiner_view.total, 2);

        let test_taker_view = session
            .test_taker_view(&test_taker)
            .expect("test taker sees current card");
        assert_eq!(test_taker_view.front, "hola");
        assert_eq!(test_taker_view.position, 1);
        assert_eq!(test_taker_view.total, 2);
    }

    #[test]
    fn advances_until_completed() {
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = active_session(examiner, test_taker);

        let session = match session.advance(&examiner).expect("advance to second card") {
            AdvanceOutcome::InProgress(session) => session,
            AdvanceOutcome::Completed(_) => panic!("two-card deck should not complete yet"),
        };
        assert_eq!(
            session.status(),
            SessionStatus::InProgress {
                current_card_index: 1
            }
        );
        assert_eq!(
            session
                .test_taker_view(&test_taker)
                .expect("second card visible")
                .front,
            "adiós"
        );

        let session = match session
            .advance(&examiner)
            .expect("complete after last card")
        {
            AdvanceOutcome::InProgress(_) => panic!("second advance should complete"),
            AdvanceOutcome::Completed(session) => session,
        };
        assert_eq!(session.status(), SessionStatus::Completed);
    }

    #[test]
    fn rejects_commands_from_unjoined_examiner() {
        let joined_examiner = ExaminerParticipant::new(ParticipantId::new());
        let unjoined_examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = ready_session(joined_examiner, test_taker);

        assert_eq!(
            session
                .start(&unjoined_examiner)
                .expect_err("unjoined examiner rejected"),
            FlippedError::UnknownParticipant
        );
    }

    #[test]
    fn terminates_active_sessions() {
        let examiner = ExaminerParticipant::new(ParticipantId::new());
        let test_taker = TestTakerParticipant::new(ParticipantId::new());
        let session = active_session(examiner, test_taker);

        let session = session.end(&examiner).expect("examiner ends session");

        assert_eq!(session.status(), SessionStatus::Terminated);
        assert_eq!(session.examiner(), &examiner);
        assert_eq!(session.test_taker(), &test_taker);
    }

    #[test]
    fn any_session_reports_dynamic_status() {
        let session = AnySession::Empty(Session::new(deck()));

        assert_eq!(session.status(), SessionStatus::Empty);
    }
}
