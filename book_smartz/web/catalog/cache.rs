//! Shared optional metadata with bounded retention and isolated migrations.
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::{
    PgPool, SqlSafeStr,
    migrate::{Migration, MigrationType, Migrator},
    types::Json,
};

use book_smartz_domain::OpenLibraryWorkId;

use super::CatalogWork;

#[derive(Clone)]
pub struct Cache {
    pool: PgPool,
}
impl Cache {
    pub(crate) const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Enriches only valid provider results. Cache failures preserve those results.
    pub(crate) async fn retain(&self, works: Vec<CatalogWork>) -> Vec<CatalogWork> {
        let mut results: Vec<_> = works.into_iter().filter(CatalogWork::valid).collect();
        if results.is_empty() {
            return results;
        }
        let fetched_at = Utc::now();
        // Both PostgreSQL limits and cancellation bound optional persistence.
        let _ = tokio::time::timeout(Duration::from_millis(500), async {
            let mut transaction = self.pool.begin().await?;
            sqlx::query("SET LOCAL statement_timeout = '150ms'").execute(&mut *transaction).await?;
            sqlx::query("SET LOCAL lock_timeout = '100ms'").execute(&mut *transaction).await?;
            // Serialize metadata writes independently of admission and ranking.
            sqlx::query("SELECT pg_advisory_xact_lock(706165241, 6)").execute(&mut *transaction).await?;
            for work in &mut results {
                let key = OpenLibraryWorkId::try_from(work.work_id.as_str()).expect("validated provider work");
                let cached: Option<(Json<CatalogWork>, String)> = sqlx::query_as("SELECT metadata, source FROM book_smartz_metadata.works WHERE work_id = $1")
                    .bind(key.as_str()).fetch_optional(&mut *transaction).await?;
                let cached = cached.and_then(|(Json(work), source)| (source == work.source()).then_some(work));
                *work = merge(work, cached.as_ref());
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
