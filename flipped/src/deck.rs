use crate::card::Flashcard;
use crate::error::FlippedError;
use crate::id::DeckId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deck {
    id: DeckId,
    title: Option<String>,
    cards: Vec<Flashcard>,
}

impl Deck {
    pub fn new(title: Option<String>, cards: Vec<Flashcard>) -> Result<Self, FlippedError> {
        Self::with_id(DeckId::new(), title, cards)
    }

    pub fn with_id(
        id: DeckId,
        title: Option<String>,
        cards: Vec<Flashcard>,
    ) -> Result<Self, FlippedError> {
        if cards.is_empty() {
            return Err(FlippedError::EmptyDeck);
        }

        Ok(Self { id, title, cards })
    }

    #[must_use]
    pub const fn id(&self) -> DeckId {
        self.id
    }

    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    #[must_use]
    pub fn cards(&self) -> &[Flashcard] {
        &self.cards
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cards.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    #[must_use]
    pub(crate) fn card_at(&self, index: usize) -> Option<&Flashcard> {
        self.cards.get(index)
    }
}
