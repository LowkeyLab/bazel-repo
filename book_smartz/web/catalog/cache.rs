//! Shared optional metadata with bounded retention and isolated migrations.
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::{
    PgPool, Postgres, SqlSafeStr, Transaction,
    migrate::{Migration, MigrationType, Migrator},
    types::Json,
};

use book_smartz_domain::{OpenLibraryWorkId, ReaderId};

use super::{CatalogWork, LookupFailure, Provider, admit};

#[derive(Clone)]
pub struct Cache {
    pool: PgPool,
}
impl Cache {
    pub(crate) const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub(crate) async fn lookup(
        &self,
        provider: &Provider,
        reader: ReaderId,
        key: &OpenLibraryWorkId,
    ) -> Result<CatalogWork, LookupFailure> {
        if key.as_str().len() > 64 {
            return Err(LookupFailure::InvalidWorkId);
        }
        let cached = self.read(key).await;
        if let Some(entry) = cached.as_ref()
            && is_fresh(entry.fresh_until, Utc::now())
        {
            return Ok(entry.work.clone());
        }
        let refreshed = async {
            if !admit(&self.pool, reader)
                .await
                .map_err(|_| LookupFailure::AdmissionUnavailable)?
            {
                return Err(LookupFailure::Limited);
            }
            let work = provider.lookup(key).await?;
            self.retain_with_fallback(vec![work], cached.as_ref().map(|entry| &entry.work))
                .await
                .into_iter()
                .next()
                .ok_or(LookupFailure::Malformed)
        }
        .await;
        match refreshed {
            Ok(work) => Ok(work),
            Err(error) => cached.map(|entry| entry.work).ok_or(error),
        }
    }

    async fn transaction(&self) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query("SET LOCAL statement_timeout = '150ms'")
            .execute(&mut *transaction)
            .await?;
        sqlx::query("SET LOCAL lock_timeout = '100ms'")
            .execute(&mut *transaction)
            .await?;
        Ok(transaction)
    }

    async fn read(&self, key: &OpenLibraryWorkId) -> Option<Entry> {
        let mut entry = None;
        let _ = tokio::time::timeout(Duration::from_millis(500), async {
            let mut transaction = self.transaction().await?;
            let row: Option<(Json<CatalogWork>, String, DateTime<Utc>)> = sqlx::query_as("SELECT metadata, source, fresh_until FROM book_smartz_metadata.works WHERE work_id = $1")
                .bind(key.as_str()).fetch_optional(&mut *transaction).await?;
            if let Some((Json(work), source, fresh_until)) = row
                && work.valid() && work.work_id == key.as_str() && source == work.source() {
                entry = Some(Entry {work, fresh_until});
                sqlx::query("SELECT pg_advisory_xact_lock(706165241, 6)").execute(&mut *transaction).await?;
                sqlx::query("UPDATE book_smartz_metadata.works SET last_used_at = clock_timestamp() WHERE work_id = $1")
                    .bind(key.as_str()).execute(&mut *transaction).await?;
            }
            transaction.commit().await?;
            Ok::<_, sqlx::Error>(())
        }).await;
        entry
    }

    /// Enriches only valid provider results. Cache failures preserve those results.
    pub(crate) async fn retain(&self, works: Vec<CatalogWork>) -> Vec<CatalogWork> {
        self.retain_with_fallback(works, None).await
    }

    async fn retain_with_fallback(
        &self,
        works: Vec<CatalogWork>,
        fallback: Option<&CatalogWork>,
    ) -> Vec<CatalogWork> {
        let fresh: Vec<_> = works.into_iter().filter(CatalogWork::valid).collect();
        let mut results: Vec<_> = fresh.iter().map(|work| merge(work, fallback)).collect();
        if results.is_empty() {
            return results;
        }
        let fetched_at = Utc::now();
        // Both PostgreSQL limits and cancellation bound optional persistence.
        let _ = tokio::time::timeout(Duration::from_millis(500), async {
            let mut transaction = self.transaction().await?;
            // Serialize metadata writes independently of admission and ranking.
            sqlx::query("SELECT pg_advisory_xact_lock(706165241, 6)").execute(&mut *transaction).await?;
            for (fresh, work) in fresh.iter().zip(&mut results) {
                let key = OpenLibraryWorkId::try_from(fresh.work_id.as_str()).expect("validated provider work");
                let cached: Option<(Json<CatalogWork>, String)> = sqlx::query_as("SELECT metadata, source FROM book_smartz_metadata.works WHERE work_id = $1")
                    .bind(key.as_str()).fetch_optional(&mut *transaction).await?;
                let cached = cached.and_then(|(Json(work), source)| (source == work.source()).then_some(work));
                *work = merge(fresh, cached.as_ref().filter(|cached| cached.valid() && cached.work_id == fresh.work_id).or(fallback));
                sqlx::query("INSERT INTO book_smartz_metadata.works (work_id, metadata, source, fetched_at, fresh_until, last_used_at) VALUES ($1, $2, $3, $4, $5, clock_timestamp()) ON CONFLICT (work_id) DO UPDATE SET metadata = EXCLUDED.metadata, source = EXCLUDED.source, fetched_at = EXCLUDED.fetched_at, fresh_until = EXCLUDED.fresh_until, last_used_at = EXCLUDED.last_used_at")
                    .bind(key.as_str()).bind(Json(&*work)).bind(work.source()).bind(fetched_at).bind(fresh_until(fetched_at)).execute(&mut *transaction).await?;
            }
            sqlx::query("DELETE FROM book_smartz_metadata.works WHERE work_id IN (SELECT work_id FROM book_smartz_metadata.works ORDER BY last_used_at DESC, work_id DESC OFFSET 10000)").execute(&mut *transaction).await?;
            transaction.commit().await?;
            Ok::<_, sqlx::Error>(())
        }).await;
        results
    }
}

struct Entry {
    work: CatalogWork,
    fresh_until: DateTime<Utc>,
}

fn is_fresh(fresh_until: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now < fresh_until
}

fn fresh_until(fetched_at: DateTime<Utc>) -> DateTime<Utc> {
    fetched_at + chrono::Duration::hours(24)
}

fn merge(fresh: &CatalogWork, cached: Option<&CatalogWork>) -> CatalogWork {
    let mut work = fresh.clone();
    if let Some(cached) = cached.filter(|cached| cached.valid() && cached.work_id == fresh.work_id)
    {
        if work.authors.is_empty() {
            work.authors.clone_from(&cached.authors);
        }
        work.cover = work.cover.or(cached.cover);
        work.year = work.year.or(cached.year);
    }
    work
}

/// Applies the isolated metadata migration.
///
/// # Errors
/// Returns an error if PostgreSQL or migration verification fails.
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    let mut connection = pool.acquire().await?;
    connection.close_on_drop();
    sqlx::query("BEGIN").execute(&mut *connection).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(706165241, 2)")
        .execute(&mut *connection)
        .await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS book_smartz_metadata")
        .execute(&mut *connection)
        .await?;
    sqlx::query("COMMIT").execute(&mut *connection).await?;
    sqlx::query("SET search_path TO book_smartz_metadata, pg_catalog")
        .execute(&mut *connection)
        .await?;
    Migrator::with_migrations(vec![Migration::new(
        1,
        "shared work metadata".into(),
        MigrationType::Simple,
        include_str!("cache.sql").into_sql_str(),
        false,
    )])
    .run(&mut *connection)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::CatalogWork;
    use super::{fresh_until, merge};
    use googletest::{assert_that, matchers::eq};
    use serde_json::json;

    #[googletest::test]
    fn metadata_expires_at_the_exact_freshness_boundary() -> anyhow::Result<()> {
        let expiry = chrono::DateTime::parse_from_rfc3339("2026-10-28T12:00:00Z")?
            .with_timezone(&chrono::Utc);
        assert_that!(
            super::is_fresh(expiry, expiry - chrono::Duration::nanoseconds(1)),
            eq(true)
        );
        assert_that!(super::is_fresh(expiry, expiry), eq(false));
        assert_that!(
            super::is_fresh(expiry, expiry + chrono::Duration::nanoseconds(1)),
            eq(false)
        );
        Ok(())
    }

    #[googletest::test]
    fn fresh_fields_win_and_missing_fields_retain_valid_metadata() -> anyhow::Result<()> {
        let cached: CatalogWork = serde_json::from_value(
            json!({"work_id":"OL43W","title":"Old title","authors":["Old author"],"year":1954,"cover":11}),
        )?;
        for (fresh, expected) in [
            (
                json!({"work_id":"OL43W","title":"New title","authors":["New author"],"year":2026,"cover":22}),
                json!({"work_id":"OL43W","title":"New title","authors":["New author"],"year":2026,"cover":22}),
            ),
            (
                json!({"work_id":"OL43W","title":"New title","authors":[],"year":null,"cover":null}),
                json!({"work_id":"OL43W","title":"New title","authors":["Old author"],"year":1954,"cover":11}),
            ),
        ] {
            let fresh = serde_json::from_value(fresh)?;
            assert_that!(
                serde_json::to_value(merge(&fresh, Some(&cached)))?,
                eq(&expected)
            );
        }
        Ok(())
    }

    #[googletest::test]
    fn invalid_or_different_cached_works_cannot_supply_metadata() -> anyhow::Result<()> {
        let fresh = json!({"work_id":"OL43W","title":"Provider title","authors":[],"year":null,"cover":null});
        let work = serde_json::from_value(fresh.clone())?;
        for (id, title, authors) in [
            ("OL44W", "Other work", vec!["Other author"]),
            ("OL1M", "Edition", vec!["Other author"]),
            ("OL43W", " ", vec!["Other author"]),
            ("OL43W", "Title", vec![" "]),
        ] {
            let cached = serde_json::from_value(
                json!({"work_id":id,"title":title,"authors":authors,"year":1954,"cover":11}),
            )?;
            assert_that!(
                serde_json::to_value(merge(&work, Some(&cached)))?,
                eq(&fresh)
            );
        }
        assert_that!(serde_json::to_value(merge(&work, None))?, eq(&fresh));
        Ok(())
    }

    #[googletest::test]
    fn successful_retrieval_has_exactly_twenty_four_hours_of_freshness() -> anyhow::Result<()> {
        let retrieved = chrono::DateTime::parse_from_rfc3339("2026-10-27T12:00:00Z")?
            .with_timezone(&chrono::Utc);
        let expected = chrono::DateTime::parse_from_rfc3339("2026-10-28T12:00:00Z")?
            .with_timezone(&chrono::Utc);
        assert_that!(fresh_until(retrieved), eq(expected));
        Ok(())
    }
}
