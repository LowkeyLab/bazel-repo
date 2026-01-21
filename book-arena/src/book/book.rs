use author::AuthorId;
use sqlx;
use std::pin::Pin;
use std::sync::Arc;
use uuid;

#[derive(Debug, Clone, PartialEq, Hash)]
pub struct BookId(uuid::Uuid);

impl BookId {
    pub fn new() -> Self {
        BookId(uuid::Uuid::new_v4())
    }
}

impl From<uuid::Uuid> for BookId {
    fn from(id: uuid::Uuid) -> Self {
        BookId(id)
    }
}

impl From<BookId> for uuid::Uuid {
    fn from(book_id: BookId) -> Self {
        book_id.0
    }
}

impl Default for BookId {
    fn default() -> Self {
        BookId::new()
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Default)]
pub struct Book {
    pub id: BookId,
    pub title: String,
    pub authors: Vec<AuthorId>,
    pub genres: Vec<String>,
    pub published_year: Option<String>,
}

impl Book {
    pub fn new() -> Self {
        Default::default()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_book_id_creation() {
        let book_id = super::BookId::new();
        assert_ne!(book_id.0, super::BookId::new().0);
    }
}
