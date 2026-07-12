use crate::error::FlippedError;
use crate::id::CardId;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct CardSide(String);

impl CardSide {
    pub fn new(text: impl Into<String>) -> Result<Self, FlippedError> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(FlippedError::BlankCardSide);
        }

        Ok(Self(text))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Flashcard {
    id: CardId,
    front: CardSide,
    back: CardSide,
}

impl Flashcard {
    pub fn new(front: impl Into<String>, back: impl Into<String>) -> Result<Self, FlippedError> {
        Self::with_id(CardId::new(), front, back)
    }

    pub fn with_id(
        id: CardId,
        front: impl Into<String>,
        back: impl Into<String>,
    ) -> Result<Self, FlippedError> {
        Ok(Self {
            id,
            front: CardSide::new(front)?,
            back: CardSide::new(back)?,
        })
    }

    #[must_use]
    pub const fn id(&self) -> CardId {
        self.id
    }

    #[must_use]
    pub const fn front(&self) -> &CardSide {
        &self.front
    }

    #[must_use]
    pub const fn back(&self) -> &CardSide {
        &self.back
    }
}
