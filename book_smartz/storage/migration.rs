use crate::StoreError;
use sqlx::{
    PgPool, SqlSafeStr,
    migrate::{Migration, MigrationType, Migrator},
};

pub(crate) async fn migrate(pool: &PgPool) -> Result<(), StoreError> {
    let mut connection = pool.acquire().await?;
    // The migration search path and SQLx session lock never return to the pool.
    connection.close_on_drop();
    sqlx::query("BEGIN").execute(&mut *connection).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(706165241, 1)")
        .execute(&mut *connection)
        .await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS book_smartz")
        .execute(&mut *connection)
        .await?;
    sqlx::query("COMMIT").execute(&mut *connection).await?;
    sqlx::query("SET search_path TO book_smartz, pg_catalog")
        .execute(&mut *connection)
        .await?;
    let migrator = Migrator::with_migrations(vec![Migration::new(
        1,
        "initial book smartz storage".into(),
        MigrationType::Simple,
        include_str!("../migrations/001_initial.sql").into_sql_str(),
        false,
    )]);
    migrator.run(&mut *connection).await?;
    Ok(())
}
