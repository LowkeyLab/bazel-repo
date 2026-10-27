//! Shared deployment and reader admission for provider requests.
use sqlx::PgPool;

use book_smartz_domain::ReaderId;

/// Try admission without waiting on another instance. A committed permit is
/// consumed even on provider failure; uncertain commits never dispatch a request.
pub async fn admit(pool: &PgPool, reader: ReaderId) -> std::result::Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(706165241, 3)")
        .fetch_one(&mut *transaction)
        .await?;
    if !locked {
        return Ok(false);
    }
    let available: bool = sqlx::query_scalar("SELECT next_search_at <= clock_timestamp() FROM book_smartz_catalog.search_gate WHERE singleton").fetch_one(&mut *transaction).await?;
    if !available {
        return Ok(false);
    }
    sqlx::query("DELETE FROM book_smartz_catalog.search_admissions WHERE admitted_at <= clock_timestamp() - interval '1 minute'").execute(&mut *transaction).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM book_smartz_catalog.search_admissions WHERE reader_id = $1",
    )
    .bind(reader.as_uuid())
    .fetch_one(&mut *transaction)
    .await?;
    if count >= 10 {
        return Ok(false);
    }
    sqlx::query("INSERT INTO book_smartz_catalog.search_admissions (reader_id, admitted_at) VALUES ($1, clock_timestamp())").bind(reader.as_uuid()).execute(&mut *transaction).await?;
    sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at = clock_timestamp() + interval '1 second' WHERE singleton").execute(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(true)
}
