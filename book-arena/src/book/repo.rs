use book::{Book, BookId};
use sqlx;
use std::pin::Pin;
use std::sync::Arc;
use uuid;

#[derive(Debug)]
pub struct Repo {
    pool: Arc<sqlx::Pool<sqlx::Postgres>>,
}

#[derive(sqlx::FromRow, Debug)]
struct Dao {
    pub id: uuid::Uuid,
    title: String,
    published_year: Option<String>,
}

impl From<Dao> for Book {
    fn from(dao: Dao) -> Self {
        Book {
            id: dao.id.into(),
            title: dao.title,
            authors: Vec::new(),
            genres: Vec::new(),
            published_year: dao.published_year,
        }
    }
}

pub trait GetById<'a> {
    fn get_by_id(
        &'a self,
        id: BookId,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Book>, ()>> + Send + 'a>>;
}

pub trait Store<'a> {
    fn store(
        &'a self,
        book: Book,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ()>> + Send + 'a>>;
}

impl<'a> GetById<'a> for Repo {
    fn get_by_id(
        &'a self,
        id: BookId,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<Option<Book>, ()>> + Send + 'a>> {
        Box::pin(async move {
            let record = sqlx::query_as::<_, Dao>(
                "SELECT id, title, published_year FROM books WHERE id = $1",
            )
            .bind::<uuid::Uuid>(id.into())
            .fetch_optional(&*self.pool)
            .await
            .map_err(|_| ())?;

            if let Some(dao) = record {
                Ok(Some(dao.into()))
            } else {
                Ok(None)
            }
        })
    }
}

impl<'a> Store<'a> for Repo {
    fn store(
        &'a self,
        book: Book,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), ()>> + Send + 'a>> {
        Box::pin(async move {
            sqlx::query("INSERT INTO books (id, title, published_year) VALUES ($1, $2, $3)")
                .bind::<uuid::Uuid>(book.id.into())
                .bind::<String>(book.title)
                .bind::<Option<String>>(book.published_year)
                .execute(&*self.pool)
                .await
                .map_err(|_| ())?;

            Ok(())
        })
    }
}
