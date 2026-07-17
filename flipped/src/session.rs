use crate::deck::Deck;
use crate::error::FlippedError;
use crate::id::{CardId, SessionId};
use crate::participant::{ExaminerParticipant, TestTakerParticipant};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ExaminerCardView {
    pub card_id: CardId,
    pub position: usize,
    pub total: usize,
    pub front: String,
    pub back: String,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct TestTakerCardView {
    pub card_id: CardId,
    pub position: usize,
    pub total: usize,
    pub front: String,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum SessionStatus {
    Empty,
    HasExaminer,
    HasTestTaker,
    Ready,
    InProgress { current_card_index: usize },
    Completed,
    Terminated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session<S> {
    id: SessionId,
    deck: Deck,
    state: S,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Empty;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct HasExaminer {
    examiner: ExaminerParticipant,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct HasTestTaker {
    test_taker: TestTakerParticipant,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Ready {
    examiner: ExaminerParticipant,
    test_taker: TestTakerParticipant,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct InProgress {
    examiner: ExaminerParticipant,
    test_taker: TestTakerParticipant,
    current_card_index: usize,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Completed {
    examiner: ExaminerParticipant,
    test_taker: TestTakerParticipant,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct Terminated {
    examiner: ExaminerParticipant,
    test_taker: TestTakerParticipant,
}

pub type EmptySession = Session<Empty>;
pub type SessionWithExaminer = Session<HasExaminer>;
pub type SessionWithTestTaker = Session<HasTestTaker>;
pub type ReadySession = Session<Ready>;
pub type ActiveSession = Session<InProgress>;
pub type CompletedSession = Session<Completed>;
pub type TerminatedSession = Session<Terminated>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvanceOutcome {
    InProgress(ActiveSession),
    Completed(CompletedSession),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnySession {
    Empty(EmptySession),
    HasExaminer(SessionWithExaminer),
    HasTestTaker(SessionWithTestTaker),
    Ready(ReadySession),
    InProgress(ActiveSession),
    Completed(CompletedSession),
    Terminated(TerminatedSession),
}

pub trait SessionLifecycle {
    fn status(&self) -> SessionStatus;
}

impl SessionLifecycle for Empty {
    fn status(&self) -> SessionStatus {
        SessionStatus::Empty
    }
}

impl SessionLifecycle for HasExaminer {
    fn status(&self) -> SessionStatus {
        SessionStatus::HasExaminer
    }
}

impl SessionLifecycle for HasTestTaker {
    fn status(&self) -> SessionStatus {
        SessionStatus::HasTestTaker
    }
}

impl SessionLifecycle for Ready {
    fn status(&self) -> SessionStatus {
        SessionStatus::Ready
    }
}

impl SessionLifecycle for InProgress {
    fn status(&self) -> SessionStatus {
        SessionStatus::InProgress {
            current_card_index: self.current_card_index,
        }
    }
}

impl SessionLifecycle for Completed {
    fn status(&self) -> SessionStatus {
        SessionStatus::Completed
    }
}

impl SessionLifecycle for Terminated {
    fn status(&self) -> SessionStatus {
        SessionStatus::Terminated
    }
}

impl<S> Session<S> {
    #[must_use]
    pub const fn id(&self) -> SessionId {
        self.id
    }

    #[must_use]
    pub const fn deck(&self) -> &Deck {
        &self.deck
    }

    #[must_use]
    pub fn into_parts(self) -> (SessionId, Deck, S) {
        (self.id, self.deck, self.state)
    }
}

impl<S: SessionLifecycle> Session<S> {
    #[must_use]
    pub fn status(&self) -> SessionStatus {
        self.state.status()
    }
}

impl Session<Empty> {
    #[must_use]
    pub fn new(deck: Deck) -> Self {
        Self::with_id(SessionId::new(), deck)
    }

    #[must_use]
    pub fn with_id(id: SessionId, deck: Deck) -> Self {
        Self {
            id,
            deck,
            state: Empty,
        }
    }

    #[must_use]
    pub fn join_examiner(self, examiner: ExaminerParticipant) -> Session<HasExaminer> {
        let (id, deck, _) = self.into_parts();
        Session {
            id,
            deck,
            state: HasExaminer { examiner },
        }
    }

    #[must_use]
    pub fn join_test_taker(self, test_taker: TestTakerParticipant) -> Session<HasTestTaker> {
        let (id, deck, _) = self.into_parts();
        Session {
            id,
            deck,
            state: HasTestTaker { test_taker },
        }
    }
}

impl Session<HasExaminer> {
    #[must_use]
    pub fn join_test_taker(self, test_taker: TestTakerParticipant) -> Session<Ready> {
        let (id, deck, state) = self.into_parts();
        Session {
            id,
            deck,
            state: Ready {
                examiner: state.examiner,
                test_taker,
            },
        }
    }

    #[must_use]
    pub const fn examiner(&self) -> &ExaminerParticipant {
        &self.state.examiner
    }
}

impl Session<HasTestTaker> {
    #[must_use]
    pub fn join_examiner(self, examiner: ExaminerParticipant) -> Session<Ready> {
        let (id, deck, state) = self.into_parts();
        Session {
            id,
            deck,
            state: Ready {
                examiner,
                test_taker: state.test_taker,
            },
        }
    }

    #[must_use]
    pub const fn test_taker(&self) -> &TestTakerParticipant {
        &self.state.test_taker
    }
}

impl Session<Ready> {
    pub fn start(self, by: &ExaminerParticipant) -> Result<Session<InProgress>, FlippedError> {
        self.ensure_examiner(by)?;
        let (id, deck, state) = self.into_parts();
        Ok(Session {
            id,
            deck,
            state: InProgress {
                examiner: state.examiner,
                test_taker: state.test_taker,
                current_card_index: 0,
            },
        })
    }

    #[must_use]
    pub const fn examiner(&self) -> &ExaminerParticipant {
        &self.state.examiner
    }

    #[must_use]
    pub const fn test_taker(&self) -> &TestTakerParticipant {
        &self.state.test_taker
    }

    fn ensure_examiner(&self, by: &ExaminerParticipant) -> Result<(), FlippedError> {
        if self.state.examiner.id() == by.id() {
            Ok(())
        } else {
            Err(FlippedError::UnknownParticipant)
        }
    }
}

impl Session<InProgress> {
    pub fn advance(self, by: &ExaminerParticipant) -> Result<AdvanceOutcome, FlippedError> {
        self.ensure_examiner(by)?;
        let next_index = self.state.current_card_index + 1;
        let (id, deck, state) = self.into_parts();

        if next_index >= deck.len() {
            Ok(AdvanceOutcome::Completed(Session {
                id,
                deck,
                state: Completed {
                    examiner: state.examiner,
                    test_taker: state.test_taker,
                },
            }))
        } else {
            Ok(AdvanceOutcome::InProgress(Session {
                id,
                deck,
                state: InProgress {
                    examiner: state.examiner,
                    test_taker: state.test_taker,
                    current_card_index: next_index,
                },
            }))
        }
    }

    pub fn end(self, by: &ExaminerParticipant) -> Result<Session<Terminated>, FlippedError> {
        self.ensure_examiner(by)?;
        let (id, deck, state) = self.into_parts();
        Ok(Session {
            id,
            deck,
            state: Terminated {
                examiner: state.examiner,
                test_taker: state.test_taker,
            },
        })
    }

    pub fn examiner_view(
        &self,
        by: &ExaminerParticipant,
    ) -> Result<ExaminerCardView, FlippedError> {
        self.ensure_examiner(by)?;
        let (index, card) = self.current_card();

        Ok(ExaminerCardView {
            card_id: card.id(),
            position: index + 1,
            total: self.deck.len(),
            front: card.front().as_str().to_owned(),
            back: card.back().as_str().to_owned(),
        })
    }

    pub fn test_taker_view(
        &self,
        by: &TestTakerParticipant,
    ) -> Result<TestTakerCardView, FlippedError> {
        self.ensure_test_taker(by)?;
        let (index, card) = self.current_card();

        Ok(TestTakerCardView {
            card_id: card.id(),
            position: index + 1,
            total: self.deck.len(),
            front: card.front().as_str().to_owned(),
        })
    }

    #[must_use]
    pub const fn examiner(&self) -> &ExaminerParticipant {
        &self.state.examiner
    }

    #[must_use]
    pub const fn test_taker(&self) -> &TestTakerParticipant {
        &self.state.test_taker
    }

    #[must_use]
    pub const fn current_card_index(&self) -> usize {
        self.state.current_card_index
    }

    fn ensure_examiner(&self, by: &ExaminerParticipant) -> Result<(), FlippedError> {
        if self.state.examiner.id() == by.id() {
            Ok(())
        } else {
            Err(FlippedError::UnknownParticipant)
        }
    }

    fn ensure_test_taker(&self, by: &TestTakerParticipant) -> Result<(), FlippedError> {
        if self.state.test_taker.id() == by.id() {
            Ok(())
        } else {
            Err(FlippedError::UnknownParticipant)
        }
    }

    fn current_card(&self) -> (usize, &crate::card::Flashcard) {
        let index = self.state.current_card_index;
        let card = self
            .deck
            .card_at(index)
            .expect("active session current card index is always in bounds");
        (index, card)
    }
}

impl Session<Completed> {
    pub fn end(self, by: &ExaminerParticipant) -> Result<Session<Terminated>, FlippedError> {
        self.ensure_examiner(by)?;
        let (id, deck, state) = self.into_parts();
        Ok(Session {
            id,
            deck,
            state: Terminated {
                examiner: state.examiner,
                test_taker: state.test_taker,
            },
        })
    }

    #[must_use]
    pub const fn examiner(&self) -> &ExaminerParticipant {
        &self.state.examiner
    }

    #[must_use]
    pub const fn test_taker(&self) -> &TestTakerParticipant {
        &self.state.test_taker
    }

    fn ensure_examiner(&self, by: &ExaminerParticipant) -> Result<(), FlippedError> {
        if self.state.examiner.id() == by.id() {
            Ok(())
        } else {
            Err(FlippedError::UnknownParticipant)
        }
    }
}

impl Session<Terminated> {
    #[must_use]
    pub const fn examiner(&self) -> &ExaminerParticipant {
        &self.state.examiner
    }

    #[must_use]
    pub const fn test_taker(&self) -> &TestTakerParticipant {
        &self.state.test_taker
    }
}

impl AnySession {
    #[must_use]
    pub fn status(&self) -> SessionStatus {
        match self {
            Self::Empty(session) => session.status(),
            Self::HasExaminer(session) => session.status(),
            Self::HasTestTaker(session) => session.status(),
            Self::Ready(session) => session.status(),
            Self::InProgress(session) => session.status(),
            Self::Completed(session) => session.status(),
            Self::Terminated(session) => session.status(),
        }
    }

    #[must_use]
    pub fn id(&self) -> SessionId {
        match self {
            Self::Empty(session) => session.id(),
            Self::HasExaminer(session) => session.id(),
            Self::HasTestTaker(session) => session.id(),
            Self::Ready(session) => session.id(),
            Self::InProgress(session) => session.id(),
            Self::Completed(session) => session.id(),
            Self::Terminated(session) => session.id(),
        }
    }

    #[must_use]
    pub fn deck(&self) -> &Deck {
        match self {
            Self::Empty(session) => session.deck(),
            Self::HasExaminer(session) => session.deck(),
            Self::HasTestTaker(session) => session.deck(),
            Self::Ready(session) => session.deck(),
            Self::InProgress(session) => session.deck(),
            Self::Completed(session) => session.deck(),
            Self::Terminated(session) => session.deck(),
        }
    }

    pub fn join_examiner(self, examiner: ExaminerParticipant) -> Result<Self, FlippedError> {
        match self {
            Self::Empty(session) => Ok(Self::HasExaminer(session.join_examiner(examiner))),
            Self::HasTestTaker(session) => Ok(Self::Ready(session.join_examiner(examiner))),
            Self::HasExaminer(_)
            | Self::Ready(_)
            | Self::InProgress(_)
            | Self::Completed(_)
            | Self::Terminated(_) => Err(FlippedError::DuplicateRole(
                crate::participant::ParticipantRole::Examiner,
            )),
        }
    }

    pub fn start(self, by: &ExaminerParticipant) -> Result<Self, FlippedError> {
        match self {
            Self::Ready(session) => session.start(by).map(Self::InProgress),
            _ => Err(FlippedError::InvalidStateTransition),
        }
    }

    pub fn advance(self, by: &ExaminerParticipant) -> Result<Self, FlippedError> {
        match self {
            Self::InProgress(session) => match session.advance(by)? {
                AdvanceOutcome::InProgress(session) => Ok(Self::InProgress(session)),
                AdvanceOutcome::Completed(session) => Ok(Self::Completed(session)),
            },
            _ => Err(FlippedError::InvalidStateTransition),
        }
    }

    pub fn end(self, by: &ExaminerParticipant) -> Result<Self, FlippedError> {
        match self {
            Self::InProgress(session) => session.end(by).map(Self::Terminated),
            Self::Completed(session) => session.end(by).map(Self::Terminated),
            _ => Err(FlippedError::InvalidStateTransition),
        }
    }

    #[must_use]
    pub fn examiner(&self) -> Option<ExaminerParticipant> {
        match self {
            Self::HasExaminer(session) => Some(*session.examiner()),
            Self::Ready(session) => Some(*session.examiner()),
            Self::InProgress(session) => Some(*session.examiner()),
            Self::Completed(session) => Some(*session.examiner()),
            Self::Terminated(session) => Some(*session.examiner()),
            Self::Empty(_) | Self::HasTestTaker(_) => None,
        }
    }

    #[must_use]
    pub fn test_taker(&self) -> Option<TestTakerParticipant> {
        match self {
            Self::HasTestTaker(session) => Some(*session.test_taker()),
            Self::Ready(session) => Some(*session.test_taker()),
            Self::InProgress(session) => Some(*session.test_taker()),
            Self::Completed(session) => Some(*session.test_taker()),
            Self::Terminated(session) => Some(*session.test_taker()),
            Self::Empty(_) | Self::HasExaminer(_) => None,
        }
    }

    pub fn examiner_view(
        &self,
        by: &ExaminerParticipant,
    ) -> Result<Option<ExaminerCardView>, FlippedError> {
        match self {
            Self::InProgress(session) => session.examiner_view(by).map(Some),
            Self::Ready(_) | Self::Completed(_) | Self::Terminated(_) => Ok(None),
            _ => Err(FlippedError::UnknownParticipant),
        }
    }

    pub fn test_taker_view(
        &self,
        by: &TestTakerParticipant,
    ) -> Result<Option<TestTakerCardView>, FlippedError> {
        match self {
            Self::InProgress(session) => session.test_taker_view(by).map(Some),
            Self::HasTestTaker(_) | Self::Ready(_) | Self::Completed(_) | Self::Terminated(_) => {
                Ok(None)
            }
            _ => Err(FlippedError::UnknownParticipant),
        }
    }
}
