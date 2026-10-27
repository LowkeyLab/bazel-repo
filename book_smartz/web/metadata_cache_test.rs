//! Metadata cache behavior with isolated real PostgreSQL.
use std::{
    str::FromStr,
    time::{Duration, Instant},
};

use googletest::{assert_that, matchers::eq};
use serde_json::json;
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, runners::AsyncRunner},
};

// Compile the concrete production boundary without adding a test-only interface.
#[allow(
    dead_code,
    reason = "The provider shares this production module with metadata decisions."
)]
#[path = "catalog/metadata.rs"]
pub mod metadata;
use metadata::{Cache, CatalogWork};

struct Database {
    pool: PgPool,
    url: String,
    _container: ContainerAsync<Postgres>,
}
impl Database {
    async fn new() -> anyhow::Result<Self> {
        let container = test_images::postgres().await.start().await?;
        let url = format!(
            "postgres://postgres:postgres@{}:{}/postgres",
            container.get_host().await?,
            container.get_host_port_ipv4(5432).await?
        );
        let pool = PgPool::connect(&url).await?;
        metadata::migrate(&pool).await?;
        Ok(Self {
            pool,
            url,
            _container: container,
        })
    }
}
fn work(id: &str, title: &str, authors: &[&str]) -> CatalogWork {
    serde_json::from_value(
        json!({"work_id":id,"title":title,"authors":authors,"year":null,"cover":null}),
    )
    .expect("valid fixture shape")
}

#[googletest::test]
#[tokio::test]
async fn metadata_is_shared_without_registration_and_retains_provider_freshness()
-> anyhow::Result<()> {
    let db = Database::new().await?;
    let first = Cache::new(db.pool.clone());
    let initial: CatalogWork = serde_json::from_value(
        json!({"work_id":"OL43W","title":"Old title","authors":["Retained author"],"year":1954,"cover":258_027}),
    )?;
    first.retain(vec![initial]).await;
    let independent = PgPool::connect(&db.url).await?;
    let second = Cache::new(independent.clone());
    let result = second
        .retain(vec![work("OL43W", "Provider correction", &[])])
        .await;
    assert_that!(result.len(), eq(1));
    assert_that!(result[0].title.as_str(), eq("Provider correction"));
    assert_that!(result[0].authors, eq(&vec!["Retained author".to_owned()]));
    assert_that!(result[0].cover, eq(Some(258_027)));
    assert_that!(result[0].year, eq(Some(1954)));
    let freshness: bool = sqlx::query_scalar("SELECT fresh_until = fetched_at + interval '24 hours' FROM book_smartz_metadata.works WHERE work_id = 'OL43W'").fetch_one(&db.pool).await?;
    assert_that!(freshness, eq(true));
    assert_that!(
        first
            .retain(vec![
                work("OL1M", "Invalid edition", &[]),
                work("OL2W", " ", &[])
            ])
            .await
            .len(),
        eq(0)
    );
    independent.close().await;
    db.pool.close().await;
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn least_recently_used_removal_and_independent_writers_preserve_capacity()
-> anyhow::Result<()> {
    let db = Database::new().await?;
    sqlx::query("INSERT INTO book_smartz_metadata.works SELECT 'OL' || n || 'W', jsonb_build_object('work_id', 'OL' || n || 'W', 'title', 'Retained title', 'authors', jsonb_build_array('Retained author'), 'year', NULL, 'cover', NULL), 'https://openlibrary.org/works/OL' || n || 'W', now(), now() + interval '24 hours', timestamptz '2000-01-01' + n * interval '1 second' FROM generate_series(1, 10000) n").execute(&db.pool).await?;
    let cache = Cache::new(db.pool.clone());
    let touched = cache.retain(vec![work("OL1W", "Touched work", &[])]).await;
    assert_that!(touched[0].authors, eq(&vec!["Retained author".to_owned()]));
    cache.retain(vec![work("OL10001W", "New work", &[])]).await;
    let preserved = cache.retain(vec![work("OL1W", "Touched again", &[])]).await;
    assert_that!(
        preserved[0].authors,
        eq(&vec!["Retained author".to_owned()])
    );
    let removed = cache.retain(vec![work("OL2W", "Removed work", &[])]).await;
    assert_that!(removed[0].authors.is_empty(), eq(true));
    let independent = PgPool::connect(&db.url).await?;
    let other = Cache::new(independent.clone());
    let left = (10002..10012)
        .map(|n| work(&format!("OL{n}W"), "Left writer", &[]))
        .collect();
    let right = (10012..10022)
        .map(|n| work(&format!("OL{n}W"), "Right writer", &[]))
        .collect();
    let (left, right) = tokio::join!(cache.retain(left), other.retain(right));
    assert_that!(left.len(), eq(10));
    assert_that!(right.len(), eq(10));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM book_smartz_metadata.works")
        .fetch_one(&db.pool)
        .await?;
    assert_that!(count, eq(10000));
    independent.close().await;
    db.pool.close().await;
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn cache_permissions_preserve_valid_provider_results_and_failed_writes_keep_freshness()
-> anyhow::Result<()> {
    let db = Database::new().await?;
    let cache = Cache::new(db.pool.clone());
    cache
        .retain(vec![work("OL43W", "Original title", &["Retained author"])])
        .await;
    let before: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT fetched_at, fresh_until FROM book_smartz_metadata.works WHERE work_id = 'OL43W'",
    )
    .fetch_one(&db.pool)
    .await?;
    sqlx::query("CREATE ROLE metadata_reader LOGIN PASSWORD 'fixture-only-password'")
        .execute(&db.pool)
        .await?;
    sqlx::query("GRANT USAGE ON SCHEMA book_smartz_metadata TO metadata_reader")
        .execute(&db.pool)
        .await?;
    sqlx::query("GRANT SELECT ON book_smartz_metadata.works TO metadata_reader")
        .execute(&db.pool)
        .await?;
    let restricted = PgPoolOptions::new()
        .connect_with(
            PgConnectOptions::from_str(&db.url)?
                .username("metadata_reader")
                .password("fixture-only-password"),
        )
        .await?;
    let reader = Cache::new(restricted.clone());
    let result = reader.retain(vec![work("OL43W", "Fresh title", &[])]).await;
    assert_that!(result[0].title.as_str(), eq("Fresh title"));
    assert_that!(result[0].authors, eq(&vec!["Retained author".to_owned()]));
    let after: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT fetched_at, fresh_until FROM book_smartz_metadata.works WHERE work_id = 'OL43W'",
    )
    .fetch_one(&db.pool)
    .await?;
    assert_that!(after, eq(before));
    sqlx::query("REVOKE SELECT ON book_smartz_metadata.works FROM metadata_reader")
        .execute(&db.pool)
        .await?;
    let result = reader
        .retain(vec![work("OL43W", "Fresh title", &["Provider author"])])
        .await;
    assert_that!(result[0].authors, eq(&vec!["Provider author".to_owned()]));
    restricted.close().await;
    db.pool.close().await;
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn a_cache_table_lock_has_a_bounded_effect_and_preserves_retained_metadata()
-> anyhow::Result<()> {
    let db = Database::new().await?;
    let cache = Cache::new(db.pool.clone());
    cache
        .retain(vec![work("OL43W", "Original title", &["Retained author"])])
        .await;
    let mut locked = db.pool.begin().await?;
    sqlx::query("LOCK TABLE book_smartz_metadata.works IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *locked)
        .await?;
    let started = Instant::now();
    let result = cache.retain(vec![work("OL43W", "Fresh title", &[])]).await;
    assert_that!(started.elapsed() < Duration::from_secs(2), eq(true));
    assert_that!(result[0].title.as_str(), eq("Fresh title"));
    assert_that!(result[0].authors.is_empty(), eq(true));
    locked.rollback().await?;
    let recovered = cache.retain(vec![work("OL43W", "Fresh title", &[])]).await;
    assert_that!(
        recovered[0].authors,
        eq(&vec!["Retained author".to_owned()])
    );
    db.pool.close().await;
    Ok(())
}
