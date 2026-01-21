use author::AuthorId;
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

pub struct Service<'a, Repo>
where
    Repo: GetById<'a> + Store<'a>,
{
    repo: &'a Repo,
}

impl<'a> Service<'a, Repo> {
    pub fn new(repo: &'a Repo) -> Self {
        Service { repo }
    }
}

impl Default for Service<'_, Repo> {
    fn default() -> Self {
        Service { repo: &Repo }
    }
}

pub struct Repo;

pub trait Store<'a> {
    fn store(
        &'a self,
        book: Book,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ()>> + Send>>;
}

impl Store<'_> for Repo {
    fn store(
        &self,
        book: Book,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ()>> + Send>> {
        // Implementation to store a book in the database
        Box::pin(async move { Ok(()) })
    }
}

pub trait GetById<'a> {
    fn get_by_id(
        &'a self,
        book_id: &'a BookId,
    ) -> Pin<Box<dyn std::future::Future<Output = Option<Book>> + Send>>;
}

impl GetById<'_> for Repo {
    fn get_by_id(
        &self,
        book_id: &BookId,
    ) -> Pin<Box<dyn std::future::Future<Output = Option<Book>> + Send>> {
        // Implementation to retrieve a book by its ID from the database
        Box::pin(async move { None })
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
