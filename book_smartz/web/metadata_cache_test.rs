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
    async fn with_reader() -> anyhow::Result<(Self, book_smartz_domain::ReaderId)> {
        let db = Self::new().await?;
        let files = runfiles::Runfiles::create()?;
        let binary = runfiles::rlocation!(files, std::env::var("APPLICATION_BINARY")?)
            .ok_or_else(|| anyhow::anyhow!("The application binary is absent."))?;
        let cwd = tempfile::tempdir()?;
        let status = std::process::Command::new(binary)
            .arg("migrate")
            .env("DATABASE_URL", &db.url)
            .current_dir(cwd.path())
            .status()?;
        anyhow::ensure!(status.success(), "The production migration failed.");
        let reader = book_smartz_domain::ReaderId::new(uuid::Uuid::new_v4());
        sqlx::query("INSERT INTO book_smartz_web.accounts (issuer, subject, reader_id) VALUES ('fixture', $1, $2)").bind(reader.as_uuid().to_string()).bind(reader.as_uuid()).execute(&db.pool).await?;
        Ok((db, reader))
    }
}
fn provider(server: &wiremock::MockServer) -> anyhow::Result<metadata::Provider> {
    metadata::Provider::new(
        format!("{}/search.json", server.uri()).parse()?,
        "maintainer@example.test",
    )
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
    let (db, reader) = Database::with_reader().await?;
    sqlx::query("INSERT INTO book_smartz_metadata.works SELECT 'OL' || n || 'W', jsonb_build_object('work_id', 'OL' || n || 'W', 'title', 'Retained title', 'authors', jsonb_build_array('Retained author'), 'year', NULL, 'cover', NULL), 'https://openlibrary.org/works/OL' || n || 'W', now(), now() + interval '24 hours', timestamptz '2000-01-01' + n * interval '1 second' FROM generate_series(1, 10000) n").execute(&db.pool).await?;
    let cache = Cache::new(db.pool.clone());
    let touched = cache.retain(vec![work("OL1W", "Touched work", &[])]).await;
    assert_that!(touched[0].authors, eq(&vec!["Retained author".to_owned()]));
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/search.json"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(
            json!({"start":0,"numFound":1,"docs":[{"key":"OL10001W","title":"New work"}]}),
        ))
        .mount(&server)
        .await;
    cache
        .lookup(
            &provider(&server)?,
            reader,
            &book_smartz_domain::OpenLibraryWorkId::try_from("OL10001W")?,
        )
        .await?;
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

#[googletest::test]
#[tokio::test]
async fn fresh_known_metadata_never_requests_the_provider() -> anyhow::Result<()> {
    let db = Database::new().await?;
    let cache = Cache::new(db.pool.clone());
    cache
        .retain(vec![work("OL43W", "Retained title", &["Retained author"])])
        .await;
    let before: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT fetched_at, fresh_until FROM book_smartz_metadata.works WHERE work_id = 'OL43W'",
    )
    .fetch_one(&db.pool)
    .await?;
    sqlx::query("UPDATE book_smartz_metadata.works SET last_used_at = timestamptz '2000-01-01' WHERE work_id = 'OL43W'").execute(&db.pool).await?;
    let server = wiremock::MockServer::start().await;
    let provider = provider(&server)?;
    let key = book_smartz_domain::OpenLibraryWorkId::try_from("OL43W")?;
    let result = cache
        .lookup(
            &provider,
            book_smartz_domain::ReaderId::new(uuid::Uuid::nil()),
            &key,
        )
        .await?;
    assert_that!(result.title.as_str(), eq("Retained title"));
    assert_that!(result.authors, eq(&vec!["Retained author".to_owned()]));
    assert_that!(server.received_requests().await.unwrap().len(), eq(0));
    let after: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT fetched_at, fresh_until FROM book_smartz_metadata.works WHERE work_id = 'OL43W'",
    )
    .fetch_one(&db.pool)
    .await?;
    assert_that!(after, eq(before));
    let touched: bool = sqlx::query_scalar("SELECT last_used_at > timestamptz '2000-01-01' FROM book_smartz_metadata.works WHERE work_id = 'OL43W'").fetch_one(&db.pool).await?;
    assert_that!(touched, eq(true));
    db.pool.close().await;
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn expired_known_metadata_refreshes_through_the_provider_and_retains_missing_fields()
-> anyhow::Result<()> {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{header, method, path, query_param},
    };
    let (db, reader) = Database::with_reader().await?;
    let cache = Cache::new(db.pool.clone());
    cache
        .retain(vec![work("OL43W", "Old title", &["Retained author"])])
        .await;
    sqlx::query("UPDATE book_smartz_metadata.works SET fetched_at = now() - interval '24 hours', fresh_until = now() WHERE work_id = 'OL43W'").execute(&db.pool).await?;
    let server = wiremock::MockServer::start().await;
    Mock::given(method("GET")).and(path("/search.json"))
        .and(query_param("q", "key:(/works/OL43W)"))
        .and(query_param("limit", "20"))
        .and(header("user-agent", "Book Smartz (maintainer@example.test)"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":"/works/OL43W","title":"Provider correction"}]})))
        .mount(&server).await;
    let provider = provider(&server)?;
    let key = book_smartz_domain::OpenLibraryWorkId::try_from("OL43W")?;
    let result = cache.lookup(&provider, reader, &key).await?;
    assert_that!(result.title.as_str(), eq("Provider correction"));
    assert_that!(result.authors, eq(&vec!["Retained author".to_owned()]));
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    let refreshed: bool = sqlx::query_scalar("SELECT fetched_at > now() - interval '1 hour' AND fresh_until = fetched_at + interval '24 hours' FROM book_smartz_metadata.works WHERE work_id = 'OL43W'").fetch_one(&db.pool).await?;
    assert_that!(refreshed, eq(true));
    db.pool.close().await;
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn failed_refresh_returns_stale_without_extending_freshness() -> anyhow::Result<()> {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let (db, reader) = Database::with_reader().await?;
    let cache = Cache::new(db.pool.clone());
    cache
        .retain(vec![work("OL43W", "Retained title", &["Retained author"])])
        .await;
    sqlx::query("UPDATE book_smartz_metadata.works SET fetched_at = now() - interval '25 hours', fresh_until = now() - interval '1 hour' WHERE work_id = 'OL43W'").execute(&db.pool).await?;
    let before: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT fetched_at, fresh_until FROM book_smartz_metadata.works WHERE work_id = 'OL43W'",
    )
    .fetch_one(&db.pool)
    .await?;
    let server = wiremock::MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let result = cache
        .lookup(
            &provider(&server)?,
            reader,
            &book_smartz_domain::OpenLibraryWorkId::try_from("OL43W")?,
        )
        .await?;
    assert_that!(result.title.as_str(), eq("Retained title"));
    assert_that!(result.authors, eq(&vec!["Retained author".to_owned()]));
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    let after: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT fetched_at, fresh_until FROM book_smartz_metadata.works WHERE work_id = 'OL43W'",
    )
    .fetch_one(&db.pool)
    .await?;
    assert_that!(after, eq(before));
    db.pool.close().await;
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn unusable_provider_results_fail_explicitly_without_cache_entries() -> anyhow::Result<()> {
    use metadata::LookupFailure;
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let (db, reader) = Database::with_reader().await?;
    let cache = Cache::new(db.pool.clone());
    let server = wiremock::MockServer::start().await;
    let client = provider(&server)?;
    let key = book_smartz_domain::OpenLibraryWorkId::try_from("OL43W")?;
    for (response, expected) in [
        (ResponseTemplate::new(503), LookupFailure::Unavailable),
        (ResponseTemplate::new(429), LookupFailure::Limited),
        (ResponseTemplate::new(200).set_body_string("{}"), LookupFailure::Malformed),
        (ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":0,"docs":[]})), LookupFailure::NotFound),
        (ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":"OL44W","title":"Other work"}]})), LookupFailure::Malformed),
        (ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":"OL1M","title":"Edition"}]})), LookupFailure::Malformed),
        (ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":"invalid","title":"Invalid identity"}]})), LookupFailure::Malformed),
        (ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":"OL43W","title":" "}]})), LookupFailure::Malformed),
        (ResponseTemplate::new(200).set_body_string("x".repeat(1_048_577)), LookupFailure::Oversized),
        (ResponseTemplate::new(200).set_delay(Duration::from_secs(6)).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":"OL43W","title":"Delayed response"}]})), LookupFailure::Timeout),
    ] {
        server.reset().await;
        sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at = '-infinity'").execute(&db.pool).await?;
        Mock::given(method("GET")).and(path("/search.json")).respond_with(response).mount(&server).await;
        assert_that!(cache.lookup(&client, reader, &key).await.err(), eq(Some(expected)));
        assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM book_smartz_metadata.works")
        .fetch_one(&db.pool)
        .await?;
    assert_that!(count, eq(0));
    server.reset().await;
    let too_long = format!("OL{}W", "9".repeat(64));
    let key = book_smartz_domain::OpenLibraryWorkId::try_from(too_long.as_str())?;
    assert_that!(
        cache.lookup(&client, reader, &key).await.err(),
        eq(Some(LookupFailure::InvalidWorkId))
    );
    assert_that!(server.received_requests().await.unwrap().len(), eq(0));
    db.pool.close().await;
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn cache_specific_failures_preserve_usable_provider_metadata() -> anyhow::Result<()> {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let (db, reader) = Database::with_reader().await?;
    let cache = Cache::new(db.pool.clone());
    cache
        .retain(vec![work("OL43W", "Old title", &["Retained author"])])
        .await;
    sqlx::query("UPDATE book_smartz_metadata.works SET fetched_at = now() - interval '25 hours', fresh_until = now() - interval '1 hour' WHERE work_id = 'OL43W'").execute(&db.pool).await?;
    let before: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT fetched_at, fresh_until FROM book_smartz_metadata.works WHERE work_id = 'OL43W'",
    )
    .fetch_one(&db.pool)
    .await?;
    sqlx::query("CREATE ROLE lookup_reader LOGIN PASSWORD 'fixture-only-password'")
        .execute(&db.pool)
        .await?;
    sqlx::query("GRANT USAGE ON SCHEMA book_smartz_metadata, book_smartz_catalog, book_smartz_web TO lookup_reader").execute(&db.pool).await?;
    sqlx::query(
        "GRANT SELECT ON book_smartz_metadata.works, book_smartz_web.accounts TO lookup_reader",
    )
    .execute(&db.pool)
    .await?;
    sqlx::query("GRANT SELECT, UPDATE ON book_smartz_catalog.search_gate TO lookup_reader")
        .execute(&db.pool)
        .await?;
    sqlx::query(
        "GRANT SELECT, INSERT, DELETE ON book_smartz_catalog.search_admissions TO lookup_reader",
    )
    .execute(&db.pool)
    .await?;
    let restricted = PgPoolOptions::new()
        .connect_with(
            PgConnectOptions::from_str(&db.url)?
                .username("lookup_reader")
                .password("fixture-only-password"),
        )
        .await?;
    let limited = Cache::new(restricted.clone());
    let server = wiremock::MockServer::start().await;
    let client = provider(&server)?;
    let key = book_smartz_domain::OpenLibraryWorkId::try_from("OL43W")?;
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"start":0,"numFound":1,"docs":[{"key":"OL43W","title":"Provider correction"}]}),
        ))
        .mount(&server)
        .await;
    let result = limited.lookup(&client, reader, &key).await?;
    assert_that!(result.title.as_str(), eq("Provider correction"));
    assert_that!(result.authors, eq(&vec!["Retained author".to_owned()]));
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    let after: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT fetched_at, fresh_until FROM book_smartz_metadata.works WHERE work_id = 'OL43W'",
    )
    .fetch_one(&db.pool)
    .await?;
    assert_that!(after, eq(before));
    sqlx::query("REVOKE SELECT ON book_smartz_metadata.works FROM lookup_reader")
        .execute(&db.pool)
        .await?;
    sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at = '-infinity'")
        .execute(&db.pool)
        .await?;
    server.reset().await;
    Mock::given(method("GET")).and(path("/search.json")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":"OL43W","title":"Fresh without cache","author_name":["Provider author"]}]}))).mount(&server).await;
    let result = limited.lookup(&client, reader, &key).await?;
    assert_that!(result.title.as_str(), eq("Fresh without cache"));
    assert_that!(result.authors, eq(&vec!["Provider author".to_owned()]));
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    sqlx::query("UPDATE book_smartz_metadata.works SET metadata = jsonb_set(metadata, '{work_id}', to_jsonb('OL44W'::text)), fetched_at = now(), fresh_until = now() + interval '24 hours' WHERE work_id = 'OL43W'").execute(&db.pool).await?;
    sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at = '-infinity'")
        .execute(&db.pool)
        .await?;
    server.reset().await;
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"start":0,"numFound":1,"docs":[{"key":"OL43W","title":"Verified work"}]}),
        ))
        .mount(&server)
        .await;
    let corrected = cache.lookup(&client, reader, &key).await?;
    assert_that!(corrected.work_id.as_str(), eq("OL43W"));
    assert_that!(corrected.title.as_str(), eq("Verified work"));
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    restricted.close().await;
    db.pool.close().await;
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn lookups_share_deployment_and_reader_admission() -> anyhow::Result<()> {
    use metadata::LookupFailure;
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let (db, reader) = Database::with_reader().await?;
    let cache = Cache::new(db.pool.clone());
    let independent = PgPool::connect(&db.url).await?;
    let other = Cache::new(independent.clone());
    let server = wiremock::MockServer::start().await;
    let client = provider(&server)?;
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"start":0,"numFound":1,"docs":[{"key":"OL43W","title":"Known work"}]}),
        ))
        .mount(&server)
        .await;
    let key = book_smartz_domain::OpenLibraryWorkId::try_from("OL43W")?;
    cache.lookup(&client, reader, &key).await?;
    server.reset().await;
    sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at = '-infinity'")
        .execute(&db.pool)
        .await?;
    assert_that!(
        other.lookup(&client, reader, &key).await?.title.as_str(),
        eq("Known work")
    );
    assert_that!(server.received_requests().await.unwrap().len(), eq(0));
    sqlx::query(
        "UPDATE book_smartz_catalog.search_gate SET next_search_at = now() + interval '1 hour'",
    )
    .execute(&db.pool)
    .await?;
    let absent = book_smartz_domain::OpenLibraryWorkId::try_from("OL44W")?;
    assert_that!(
        other.lookup(&client, reader, &absent).await.err(),
        eq(Some(LookupFailure::Limited))
    );
    sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at = '-infinity'")
        .execute(&db.pool)
        .await?;
    sqlx::query("DELETE FROM book_smartz_catalog.search_admissions")
        .execute(&db.pool)
        .await?;
    sqlx::query("INSERT INTO book_smartz_catalog.search_admissions (reader_id, admitted_at) SELECT $1, clock_timestamp() FROM generate_series(1, 10)").bind(reader.as_uuid()).execute(&db.pool).await?;
    assert_that!(
        other.lookup(&client, reader, &absent).await.err(),
        eq(Some(LookupFailure::Limited))
    );
    assert_that!(server.received_requests().await.unwrap().len(), eq(0));
    independent.close().await;
    db.pool.close().await;
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn provider_io_holds_no_cache_transaction_and_cancellation_preserves_freshness()
-> anyhow::Result<()> {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let (db, reader) = Database::with_reader().await?;
    let cache = Cache::new(db.pool.clone());
    cache
        .retain(vec![work("OL43W", "Retained title", &["Retained author"])])
        .await;
    sqlx::query("UPDATE book_smartz_metadata.works SET fetched_at = now() - interval '25 hours', fresh_until = now() - interval '1 hour' WHERE work_id = 'OL43W'").execute(&db.pool).await?;
    let before: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT fetched_at, fresh_until FROM book_smartz_metadata.works WHERE work_id = 'OL43W'",
    )
    .fetch_one(&db.pool)
    .await?;
    let server = wiremock::MockServer::start().await;
    Mock::given(method("GET")).and(path("/search.json")).respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(6)).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":"OL43W","title":"Delayed correction"}]}))).mount(&server).await;
    let client = std::sync::Arc::new(provider(&server)?);
    let held_client = client.clone();
    let held_cache = cache.clone();
    let key = book_smartz_domain::OpenLibraryWorkId::try_from("OL43W")?;
    let pending = tokio::spawn(async move { held_cache.lookup(&held_client, reader, &key).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while server.received_requests().await.unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_that!(pending.is_finished(), eq(false));
    cache
        .retain(vec![work("OL44W", "Independent work", &[])])
        .await;
    let independent = cache
        .lookup(
            &client,
            reader,
            &book_smartz_domain::OpenLibraryWorkId::try_from("OL44W")?,
        )
        .await?;
    assert_that!(independent.title.as_str(), eq("Independent work"));
    assert_that!(pending.is_finished(), eq(false));
    pending.abort();
    let cancelled = pending
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("The lookup completed before cancellation."))?;
    assert_that!(cancelled.is_cancelled(), eq(true));
    let after: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
        "SELECT fetched_at, fresh_until FROM book_smartz_metadata.works WHERE work_id = 'OL43W'",
    )
    .fetch_one(&db.pool)
    .await?;
    assert_that!(after, eq(before));
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    db.pool.close().await;
    Ok(())
}
