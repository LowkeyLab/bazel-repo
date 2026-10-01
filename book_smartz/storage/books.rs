use std::time::Instant;

use book_smartz_domain::{Book, BookId, IdentityError, OpenLibraryWorkId};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    Observation, Operation, OperationContext, Outcome, Registration, Rejection, Store, StoreError,
    observation,
};

fn decode_book(id: Uuid, work_id: &str) -> Result<Book, StoreError> {
    let work_id = OpenLibraryWorkId::try_from(work_id)
        .map_err(|_| StoreError::CorruptHistory("invalid stored book work ID"))?;
    Ok(Book::new(BookId::new(id), work_id))
}

async fn by_id(
    id: BookId,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<Option<Book>, StoreError> {
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, work_id FROM book_smartz.books WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&mut **transaction)
            .await?;
    row.map(|(id, work_id)| decode_book(id, &work_id))
        .transpose()
}

async fn by_work_id(
    id: &OpenLibraryWorkId,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<Option<Book>, StoreError> {
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, work_id FROM book_smartz.books WHERE work_id = $1")
            .bind(id.as_str())
            .fetch_optional(&mut **transaction)
            .await?;
    row.map(|(id, work_id)| decode_book(id, &work_id))
        .transpose()
}

async fn register(pool: &PgPool, book: &Book) -> Result<Registration, StoreError> {
    let mut transaction = pool.begin().await?;
    // Resolve a competing insertion against its committed mapping, even when
    // the caller configured a stronger session isolation level.
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *transaction)
        .await?;
    let inserted: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO book_smartz.books (id, work_id) VALUES ($1, $2) ON CONFLICT DO NOTHING RETURNING id",
    )
    .bind(book.id().as_uuid())
    .bind(book.work_id().as_str())
    .fetch_optional(&mut *transaction)
    .await?;
    let result = if inserted.is_some() {
        Registration::Created
    } else if let Some(existing) = by_id(book.id(), &mut transaction).await? {
        if existing != *book {
            return Err(IdentityError::BookIdConflict.into());
        }
        Registration::AlreadyPresent
    } else if by_work_id(book.work_id(), &mut transaction)
        .await?
        .is_some()
    {
        return Err(IdentityError::WorkIdConflict.into());
    } else {
        return Err(StoreError::Database(sqlx::Error::RowNotFound));
    };
    transaction
        .commit()
        .await
        .map_err(StoreError::from_commit)?;
    Ok(result)
}

fn registration_outcome(result: &Result<Registration, StoreError>) -> Outcome {
    match result {
        Ok(Registration::Created) => Outcome::Created,
        Ok(Registration::AlreadyPresent) => Outcome::AlreadyPresent,
        Err(StoreError::Identity(
            IdentityError::BookIdConflict | IdentityError::WorkIdConflict,
        )) => Outcome::Rejected(Rejection::IdentityConflict),
        Err(StoreError::CommitUncertain(_)) => Outcome::CommitUncertain,
        Err(error) => Outcome::Failed(error.failure()),
    }
}

fn lookup_outcome(result: &Result<Option<Book>, StoreError>) -> Outcome {
    match result {
        Ok(Some(_)) => Outcome::Found,
        Ok(None) => Outcome::Absent,
        Err(error) => Outcome::Failed(error.failure()),
    }
}

impl Store {
    /// Registers an immutable book/work identity mapping.
    ///
    /// # Errors
    /// Returns an identity conflict, database failure, or uncertain commit.
    pub async fn register_book(
        &self,
        book: &Book,
        operation: OperationContext,
    ) -> Result<Registration, StoreError> {
        let started = Instant::now();
        let result = register(&self.pool, book).await;
        observation::dispatch(
            &self.observer,
            &Observation {
                operation: Operation::Registration,
                outcome: registration_outcome(&result),
                command_kind: None,
                duration: started.elapsed(),
                correlation_id: operation.correlation_id,
                reader_id: None,
                event_id: None,
                revision: None,
                event_count: None,
            },
        );
        result
    }

    /// Finds a book by its local identity.
    ///
    /// # Errors
    /// Returns a database error or corruption if the stored work ID is malformed.
    pub async fn book_by_id(
        &self,
        id: BookId,
        operation: OperationContext,
    ) -> Result<Option<Book>, StoreError> {
        let started = Instant::now();
        let result = async {
            let row: Option<(Uuid, String)> =
                sqlx::query_as("SELECT id, work_id FROM book_smartz.books WHERE id = $1")
                    .bind(id.as_uuid())
                    .fetch_optional(&self.pool)
                    .await?;
            row.map(|(id, work_id)| decode_book(id, &work_id))
                .transpose()
        }
        .await;
        observation::dispatch(
            &self.observer,
            &Observation {
                operation: Operation::BookById,
                outcome: lookup_outcome(&result),
                command_kind: None,
                duration: started.elapsed(),
                correlation_id: operation.correlation_id,
                reader_id: None,
                event_id: None,
                revision: None,
                event_count: None,
            },
        );
        result
    }

    /// Finds a book by its Open Library work identity.
    ///
    /// # Errors
    /// Returns a database error or corruption if the stored work ID is malformed.
    pub async fn book_by_work_id(
        &self,
        id: &OpenLibraryWorkId,
        operation: OperationContext,
    ) -> Result<Option<Book>, StoreError> {
        let started = Instant::now();
        let result = async {
            let row: Option<(Uuid, String)> =
                sqlx::query_as("SELECT id, work_id FROM book_smartz.books WHERE work_id = $1")
                    .bind(id.as_str())
                    .fetch_optional(&self.pool)
                    .await?;
            row.map(|(id, work_id)| decode_book(id, &work_id))
                .transpose()
        }
        .await;
        observation::dispatch(
            &self.observer,
            &Observation {
                operation: Operation::BookByWorkId,
                outcome: lookup_outcome(&result),
                command_kind: None,
                duration: started.elapsed(),
                correlation_id: operation.correlation_id,
                reader_id: None,
                event_id: None,
                revision: None,
                event_count: None,
            },
        );
        result
    }
}
