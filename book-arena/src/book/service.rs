use book::{Book, BookId};
use repo::{GetById, Store};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Database error")]
    DatabaseError,
}

pub struct Service<'a, T>
where
    T: GetById<'a> + Store<'a>,
{
    repo: &'a T,
}

impl<'a, T> Service<'a, T>
where
    T: GetById<'a> + Store<'a>,
{
    pub fn new(repo: &'a T) -> Self {
        Service { repo }
    }

    pub async fn get_book_by_id(&'a self, book_id: BookId) -> Result<Option<Book>, Error> {
        self.repo
            .get_by_id(book_id)
            .await
            .map_err(|_| Error::DatabaseError)
    }

    pub async fn store_book(&'a self, book: Book) -> Result<(), Error> {
        self.repo
            .store(book)
            .await
            .map_err(|_| Error::DatabaseError)
    }
}
