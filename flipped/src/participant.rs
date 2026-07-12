use std::marker::PhantomData;

use crate::id::ParticipantId;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub enum ParticipantRole {
    Examiner,
    TestTaker,
}

mod sealed {
    pub trait Sealed {}
}

pub trait Role: sealed::Sealed {
    const KIND: ParticipantRole;
}

#[derive(Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Examiner;

impl sealed::Sealed for Examiner {}

impl Role for Examiner {
    const KIND: ParticipantRole = ParticipantRole::Examiner;
}

#[derive(Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct TestTaker;

impl sealed::Sealed for TestTaker {}

impl Role for TestTaker {
    const KIND: ParticipantRole = ParticipantRole::TestTaker;
}

#[derive(Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Participant<R: Role> {
    id: ParticipantId,
    _role: PhantomData<R>,
}

impl<R: Role> Participant<R> {
    #[must_use]
    pub fn new(id: ParticipantId) -> Self {
        Self {
            id,
            _role: PhantomData,
        }
    }

    #[must_use]
    pub const fn id(&self) -> ParticipantId {
        self.id
    }

    #[must_use]
    pub const fn role(&self) -> ParticipantRole {
        R::KIND
    }
}

impl<R: Role> Clone for Participant<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: Role> Copy for Participant<R> {}

pub type ExaminerParticipant = Participant<Examiner>;
pub type TestTakerParticipant = Participant<TestTaker>;
