use std::time::Instant;

use book_smartz_domain::{
    Book, BookId, BookRegistry, Command, CommandContext, DomainError, EventKind, OpenLibraryWorkId,
    Ranking, RankingEvent, ReaderId, ReplayErrorReason, decode_event, encode_event,
};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction, types::Json};
use uuid::Uuid;

use crate::{
    CommandResult, Observation, Operation, OperationContext, Outcome, Rejection, Store, StoreError,
    observation, wire,
};

type EventRow = (Uuid, String, Uuid, Uuid, Option<Uuid>, Json<Value>);

fn decode_row(row: EventRow, reader: ReaderId) -> Result<RankingEvent, StoreError> {
    let (row_reader, sequence, event_id, candidate, opponent, Json(envelope)) = row;
    let sequence = wire::parse_sequence_key(&sequence)?;
    let text = serde_json::to_string(&envelope)
        .map_err(|_| StoreError::CorruptHistory("invalid stored event JSON"))?;
    let document = decode_event(&text)
        .map_err(|_| StoreError::CorruptHistory("invalid stored event envelope"))?;
    let event = document.event();
    let event_opponent = match event.kind() {
        EventKind::ComparisonAnswered { opponent, .. } => Some(*opponent.as_uuid()),
        _ => None,
    };
    if row_reader != *reader.as_uuid()
        || event.metadata().reader_id != reader
        || sequence != event.metadata().sequence.value()
        || event_id != *event.metadata().id.as_uuid()
        || candidate != *event.candidate().as_uuid()
        || opponent != event_opponent
    {
        return Err(StoreError::CorruptHistory(
            "stored event index differs from envelope",
        ));
    }
    Ok(event.clone())
}

async fn history_from_pool(pool: &PgPool, reader: ReaderId) -> Result<Ranking, StoreError> {
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT reader_id, sequence::text, event_id, candidate_book_id, opponent_book_id, envelope \
         FROM book_smartz.ranking_events WHERE reader_id = $1 ORDER BY sequence",
    )
    .bind(reader.as_uuid())
    .fetch_all(pool)
    .await?;
    replay_rows(rows, reader)
}

async fn history_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    reader: ReaderId,
) -> Result<Ranking, StoreError> {
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT reader_id, sequence::text, event_id, candidate_book_id, opponent_book_id, envelope \
         FROM book_smartz.ranking_events WHERE reader_id = $1 ORDER BY sequence",
    )
    .bind(reader.as_uuid())
    .fetch_all(&mut **transaction)
    .await?;
    replay_rows(rows, reader)
}

fn replay_rows(rows: Vec<EventRow>, reader: ReaderId) -> Result<Ranking, StoreError> {
    let events = rows
        .into_iter()
        .map(|row| decode_row(row, reader))
        .collect::<Result<Vec<_>, _>>()?;
    Ranking::from_history(reader, events)
        .map_err(|_| StoreError::CorruptHistory("invalid ranking transition"))
}

async fn referenced_books(
    transaction: &mut Transaction<'_, Postgres>,
    command: Command,
) -> Result<BookRegistry, StoreError> {
    let referenced = match command {
        Command::Start { book_id } => Some(book_id),
        Command::Answer { opponent, .. } => Some(opponent),
        Command::Pause | Command::Resume => None,
    };
    let mut registry = BookRegistry::new();
    if let Some(id) = referenced {
        let row: Option<(Uuid, String)> =
            sqlx::query_as("SELECT id, work_id FROM book_smartz.books WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_optional(&mut **transaction)
                .await?;
        if let Some((id, work_id)) = row {
            let work_id = OpenLibraryWorkId::try_from(work_id.as_str())
                .map_err(|_| StoreError::CorruptHistory("invalid stored book work ID"))?;
            registry
                .register(Book::new(BookId::new(id), work_id))
                .map_err(|_| StoreError::CorruptHistory("invalid stored book identity"))?;
        }
    }
    Ok(registry)
}

async fn execute_in_transaction(
    pool: &PgPool,
    reader: ReaderId,
    context: CommandContext,
    command: Command,
) -> Result<CommandResult, StoreError> {
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        "INSERT INTO book_smartz.reader_streams (reader_id) VALUES ($1) ON CONFLICT DO NOTHING",
    )
    .bind(reader.as_uuid())
    .execute(&mut *transaction)
    .await?;
    sqlx::query("SELECT reader_id FROM book_smartz.reader_streams WHERE reader_id = $1 FOR UPDATE")
        .bind(reader.as_uuid())
        .fetch_one(&mut *transaction)
        .await?;
    let mut ranking = history_in_transaction(&mut transaction, reader).await?;
    let registry = referenced_books(&mut transaction, command).await?;
    let event = ranking.execute(&registry, context, command)?;
    let encoded = encode_event(&event)
        .map_err(|_| StoreError::CorruptHistory("invalid proposed event envelope"))?;
    let document = decode_event(&encoded)
        .map_err(|_| StoreError::CorruptHistory("invalid proposed event envelope"))?;
    if document.event() != &event {
        return Err(StoreError::CorruptHistory(
            "proposed event failed roundtrip",
        ));
    }
    let envelope: Value = serde_json::from_str(&encoded)
        .map_err(|_| StoreError::CorruptHistory("invalid proposed event JSON"))?;
    let opponent = match event.kind() {
        EventKind::ComparisonAnswered { opponent, .. } => Some(*opponent.as_uuid()),
        _ => None,
    };
    sqlx::query(
        "INSERT INTO book_smartz.ranking_events \
         (reader_id, sequence, event_id, candidate_book_id, opponent_book_id, envelope) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(reader.as_uuid())
    .bind(wire::sequence_key(event.metadata().sequence.value())?)
    .bind(event.metadata().id.as_uuid())
    .bind(event.candidate().as_uuid())
    .bind(opponent)
    .bind(Json(envelope))
    .execute(&mut *transaction)
    .await?;
    transaction
        .commit()
        .await
        .map_err(StoreError::from_commit)?;
    Ok(CommandResult {
        event,
        projection: ranking.projection().clone(),
    })
}

fn command_outcome(result: &Result<CommandResult, StoreError>) -> Outcome {
    match result {
        Ok(_) => Outcome::Committed,
        Err(StoreError::Domain(error)) => Outcome::Rejected(match error {
            DomainError::StaleRevision { .. } => Rejection::StaleRevision,
            DomainError::UnknownBook => Rejection::UnknownBook,
            DomainError::InvalidTransition(ReplayErrorReason::DuplicateEventId) => {
                Rejection::DuplicateEventId
            }
            _ => Rejection::InvalidTransition,
        }),
        Err(StoreError::CommitUncertain(_)) => Outcome::CommitUncertain,
        Err(error) => Outcome::Failed(error.failure()),
    }
}

impl Store {
    /// Loads one reader's ranking from its ordered, validated event history.
    ///
    /// # Errors
    /// Returns a database error or corruption if the stored history cannot be replayed.
    pub async fn load_ranking(
        &self,
        reader: ReaderId,
        operation: OperationContext,
    ) -> Result<Ranking, StoreError> {
        let started = Instant::now();
        let result = history_from_pool(&self.pool, reader).await;
        observation::dispatch(
            &self.observer,
            &Observation {
                operation: Operation::RankingLoad,
                outcome: match &result {
                    Ok(_) => Outcome::Loaded,
                    Err(error) => Outcome::Failed(error.failure()),
                },
                duration: started.elapsed(),
                correlation_id: operation.correlation_id,
                reader_id: Some(*reader.as_uuid()),
                event_id: None,
                revision: result
                    .as_ref()
                    .ok()
                    .map(|ranking| ranking.projection().revision()),
                event_count: result.as_ref().ok().map(|ranking| ranking.history().len()),
            },
        );
        result
    }

    /// Applies a command to one reader's locked event stream and commits its decision.
    ///
    /// # Errors
    /// Returns domain rejection, corruption, database failure, or uncertain commit.
    pub async fn execute(
        &self,
        reader: ReaderId,
        context: CommandContext,
        command: Command,
        operation: OperationContext,
    ) -> Result<CommandResult, StoreError> {
        let started = Instant::now();
        let result = execute_in_transaction(&self.pool, reader, context, command).await;
        observation::dispatch(
            &self.observer,
            &Observation {
                operation: Operation::RankingCommand,
                outcome: command_outcome(&result),
                duration: started.elapsed(),
                correlation_id: operation.correlation_id,
                reader_id: Some(*reader.as_uuid()),
                event_id: Some(*context.event_id.as_uuid()),
                revision: result
                    .as_ref()
                    .map_or(Some(context.expected_revision), |value| {
                        Some(value.projection.revision())
                    }),
                event_count: None,
            },
        );
        result
    }
}
