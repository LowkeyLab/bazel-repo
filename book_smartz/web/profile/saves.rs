//! Concrete PostgreSQL coordination for profile submissions, independent of ranking streams.
use sqlx::{
    PgPool, Postgres, SqlSafeStr, Transaction,
    migrate::{Migration, MigrationType, Migrator},
};
use uuid::Uuid;

use book_smartz_domain::ReaderId;

use super::{
    offers::{Offer, SaveId},
    text::Text,
};

#[derive(Clone)]
pub struct Attempt {
    pub offer: Offer,
    pub text: Option<Text>,
    pub outcome: String,
}
type Row = (Uuid, i64, Option<String>, Option<String>, String, bool);
fn attempt(row: Row) -> Attempt {
    Attempt {
        offer: Offer {
            id: SaveId(row.0),
            revision: row.1,
        },
        text: row
            .2
            .zip(row.3)
            .map(|(name, biography)| Text { name, biography }),
        outcome: row.4,
    }
}
#[derive(Clone)]
pub enum Outcome {
    Saved,
    Failed,
    Conflict,
    Unconfirmed,
    Blocked(Attempt),
    ChangedSubmission,
}

async fn locked(
    pool: &PgPool,
    reader: ReaderId,
) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    // This account-row lock also coordinates future Google imports. Ranking uses its own stream lock.
    sqlx::query(
        "SELECT reader_id FROM book_smartz_web.accounts WHERE reader_id=$1 FOR NO KEY UPDATE",
    )
    .bind(reader.as_uuid())
    .fetch_one(&mut *tx)
    .await?;
    Ok(tx)
}
async fn find(
    tx: &mut Transaction<'_, Postgres>,
    reader: ReaderId,
    id: SaveId,
) -> Result<Option<Attempt>, sqlx::Error> {
    let row: Option<Row> = sqlx::query_as("SELECT submission_id,revision,display_name,biography,outcome,acknowledged FROM book_smartz_profile_saves.submissions WHERE reader_id=$1 AND submission_id=$2")
        .bind(reader.as_uuid()).bind(id.0).fetch_optional(&mut **tx).await?;
    Ok(row.map(attempt))
}
pub async fn pending(pool: &PgPool, reader: ReaderId) -> Result<Option<Attempt>, sqlx::Error> {
    let row: Option<Row> = sqlx::query_as("SELECT submission_id,revision,display_name,biography,outcome,acknowledged FROM book_smartz_profile_saves.submissions WHERE reader_id=$1 AND NOT acknowledged")
        .bind(reader.as_uuid()).fetch_optional(pool).await?;
    Ok(row.map(attempt))
}
pub async fn current(pool: &PgPool, reader: ReaderId) -> Result<(Text, i64), sqlx::Error> {
    let row: Option<(String,String,i64)> = sqlx::query_as("SELECT display_name,biography,revision FROM book_smartz_profiles.profiles WHERE reader_id=$1")
        .bind(reader.as_uuid()).fetch_optional(pool).await?;
    Ok(row.map_or_else(
        || (Text::default(), 0),
        |(name, biography, revision)| (Text { name, biography }, revision),
    ))
}
async fn pending_locked(
    tx: &mut Transaction<'_, Postgres>,
    reader: ReaderId,
) -> Result<Option<Attempt>, sqlx::Error> {
    let row: Option<Row> = sqlx::query_as("SELECT submission_id,revision,display_name,biography,outcome,acknowledged FROM book_smartz_profile_saves.submissions WHERE reader_id=$1 AND NOT acknowledged")
        .bind(reader.as_uuid()).fetch_optional(&mut **tx).await?;
    Ok(row.map(attempt))
}
async fn acknowledge(
    pool: &PgPool,
    reader: ReaderId,
    offer: Offer,
) -> Result<Outcome, sqlx::Error> {
    let mut tx = locked(pool, reader).await?;
    let Some(row) = find(&mut tx, reader, offer.id).await? else {
        return Ok(Outcome::Unconfirmed);
    };
    let result = match row.outcome.as_str() {
        "saved" => Outcome::Saved,
        "failed" => Outcome::Failed,
        "conflict" => Outcome::Conflict,
        _ => return Ok(Outcome::Unconfirmed),
    };
    sqlx::query("UPDATE book_smartz_profile_saves.submissions SET acknowledged=true WHERE reader_id=$1 AND submission_id=$2")
        .bind(reader.as_uuid()).bind(offer.id.0).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(result)
}
async fn prepare(
    pool: &PgPool,
    reader: ReaderId,
    offer: Offer,
    text: &Text,
) -> Result<Option<Outcome>, sqlx::Error> {
    let mut tx = locked(pool, reader).await?;
    if let Some(row) = find(&mut tx, reader, offer.id).await? {
        if row.offer.revision != offer.revision
            || row.text.as_ref().is_some_and(|saved| saved != text)
        {
            return Ok(Some(Outcome::ChangedSubmission));
        }
        if row.outcome != "pending" {
            drop(tx);
            return acknowledge(pool, reader, offer).await.map(Some);
        }
    } else {
        if let Some(row) = pending_locked(&mut tx, reader).await? {
            return Ok(Some(Outcome::Blocked(row)));
        }
        sqlx::query("INSERT INTO book_smartz_profile_saves.submissions (reader_id,submission_id,revision,display_name,biography,outcome) VALUES ($1,$2,$3,$4,$5,'pending')")
            .bind(reader.as_uuid()).bind(offer.id.0).bind(offer.revision).bind(&text.name).bind(&text.biography).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(None)
}
pub async fn save(pool: &PgPool, reader: ReaderId, offer: Offer, text: &Text) -> Outcome {
    match prepare(pool, reader, offer, text).await {
        Ok(Some(result)) => return result,
        Err(_) => return Outcome::Unconfirmed,
        Ok(None) => {}
    }
    let Ok(mut tx) = locked(pool, reader).await else {
        return Outcome::Unconfirmed;
    };
    let result=async {
        let Some(row)=find(&mut tx,reader,offer.id).await? else { return Ok(false); };
        if row.outcome!="pending" { return Ok(false); }
        let revision: Option<i64>=sqlx::query_scalar("SELECT revision FROM book_smartz_profiles.profiles WHERE reader_id=$1")
            .bind(reader.as_uuid()).fetch_optional(&mut *tx).await?;
        let conflict=revision.unwrap_or(0)!=offer.revision;
        if !conflict {
            let Some(next)=offer.revision.checked_add(1) else { return Err(sqlx::Error::Protocol("profile revision exhausted".into())); };
            sqlx::query("INSERT INTO book_smartz_profiles.profiles (reader_id,display_name,biography,revision,import_attempted) VALUES ($1,$2,$3,$4,true) ON CONFLICT (reader_id) DO UPDATE SET display_name=EXCLUDED.display_name,biography=EXCLUDED.biography,revision=EXCLUDED.revision,import_attempted=true")
                .bind(reader.as_uuid()).bind(&text.name).bind(&text.biography).bind(next).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE book_smartz_profile_saves.submissions SET outcome=$3 WHERE reader_id=$1 AND submission_id=$2")
            .bind(reader.as_uuid()).bind(offer.id.0).bind(if conflict {"conflict"} else {"saved"}).execute(&mut *tx).await?;
        Ok::<_,sqlx::Error>(true)
    }.await;
    match result {
        Err(_) => {
            if tx.rollback().await.is_err() {
                return Outcome::Unconfirmed;
            }
            return check(pool, reader, offer).await;
        }
        Ok(_) => {
            if tx.commit().await.is_err() {
                return Outcome::Unconfirmed;
            }
        }
    }
    acknowledge(pool, reader, offer)
        .await
        .unwrap_or(Outcome::Unconfirmed)
}
pub async fn check(pool: &PgPool, reader: ReaderId, offer: Offer) -> Outcome {
    let result=async {
        let mut tx=locked(pool,reader).await?;
        match find(&mut tx,reader,offer.id).await? {
            Some(row) if row.outcome!="pending" => { drop(tx); return acknowledge(pool,reader,offer).await; }
            Some(_) => {
                // The lock excludes both in-flight application and preparation commits.
                // A terminal rejection prevents a late writer from applying this submission.
                sqlx::query("UPDATE book_smartz_profile_saves.submissions SET outcome='failed',acknowledged=true WHERE reader_id=$1 AND submission_id=$2")
                    .bind(reader.as_uuid()).bind(offer.id.0).execute(&mut *tx).await?;
            }
            None => {
                if let Some(row)=pending_locked(&mut tx,reader).await? { return Ok(Outcome::Blocked(row)); }
                sqlx::query("INSERT INTO book_smartz_profile_saves.submissions (reader_id,submission_id,revision,outcome,acknowledged) VALUES ($1,$2,$3,'failed',true)")
                    .bind(reader.as_uuid()).bind(offer.id.0).bind(offer.revision).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
        Ok::<_,sqlx::Error>(Outcome::Failed)
    }.await;
    result.unwrap_or(Outcome::Unconfirmed)
}
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    let mut c = pool.acquire().await?;
    c.close_on_drop();
    sqlx::query("BEGIN").execute(&mut *c).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(706165241,2)")
        .execute(&mut *c)
        .await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS book_smartz_profile_saves")
        .execute(&mut *c)
        .await?;
    sqlx::query("COMMIT").execute(&mut *c).await?;
    sqlx::query("SET search_path TO book_smartz_profile_saves,pg_catalog")
        .execute(&mut *c)
        .await?;
    Migrator::with_migrations(vec![Migration::new(
        1,
        "profile save evidence".into(),
        MigrationType::Simple,
        include_str!("saves.sql").into_sql_str(),
        false,
    )])
    .run(&mut *c)
    .await?;
    Ok(())
}
pub async fn ready(pool: &PgPool) -> anyhow::Result<()> {
    let version: i64 = sqlx::query_scalar(
        "SELECT max(version) FROM book_smartz_profile_saves._sqlx_migrations WHERE success",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(version == 1, "incompatible profile save schema");
    sqlx::query("SELECT p.revision,p.import_attempted,s.reader_id,s.submission_id,s.revision,s.display_name,s.biography,s.outcome,s.acknowledged FROM book_smartz_profiles.profiles p CROSS JOIN book_smartz_profile_saves.submissions s LIMIT 0").execute(pool).await?;
    Ok(())
}
