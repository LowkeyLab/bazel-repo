//! Compiled caller composition example; all identity and configuration inputs are explicit.
use book_smartz_domain::{Book, Command, CommandContext, ReaderId};
use book_smartz_storage::{OperationContext, Store, StoreError, tracing_observer};
use sqlx::{
    ConnectOptions,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::time::Duration;
use tracing::{Subscriber, instrument::WithSubscriber};

/// Migrates, registers a book, executes one placement, reloads, and shuts down.
/// The supplied context must describe the reader's expected revision.
///
/// # Errors
/// Returns pool, migration, identity, command, or load errors unchanged.
pub async fn run(
    database_url: &str,
    book: Book,
    reader: ReaderId,
    context: CommandContext,
    subscriber: impl Subscriber + Send + Sync + 'static,
) -> Result<(), StoreError> {
    let options = database_url
        .parse::<PgConnectOptions>()?
        .disable_statement_logging();
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(5))
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                sqlx::raw_sql("SET lock_timeout = '5s'; SET statement_timeout = '15s'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await?;
    let store = Store::new(pool.clone(), tracing_observer());
    let result = async {
        let operation = OperationContext::default();
        store.migrate(operation).await?;
        store.register_book(&book, operation).await?;
        let _accepted = store
            .execute(
                reader,
                context,
                Command::Start { book_id: book.id() },
                operation,
            )
            .await?;
        let _recovered = store.load_ranking(reader, operation).await?;
        Ok(())
    }
    .with_subscriber(subscriber)
    .await;
    // Close even when an operation failed. The caller handles uncertain-commit recovery.
    pool.close().await;
    result
}
