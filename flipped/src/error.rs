use crate::participant::ParticipantRole;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FlippedError {
    #[error("deck must contain at least one card")]
    EmptyDeck,

    #[error("card side cannot be blank")]
    BlankCardSide,

    #[error("role is already occupied: {0:?}")]
    DuplicateRole(ParticipantRole),

    #[error("participant is not known")]
    UnknownParticipant,

    #[error("command requires an examiner")]
    RequiresExaminer,

    #[error("both participants must join before starting")]
    ParticipantsNotReady,

    #[error("invalid state transition")]
    InvalidStateTransition,

    #[error("there is no current card")]
    NoCurrentCard,
}
