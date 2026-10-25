//! Typed records from the real packaged HTTP composition.
use std::time::Duration;

use googletest::{assert_that, matchers::eq};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use crate::{
    diagnostic_events::{
        EventIdentity, OperationKind, OperationOutcome, OperationRecord, Reason, Severity, Stage,
    },
    test_support::Fixture,
};

async fn next_record(app: &Fixture, after: usize) -> anyhow::Result<OperationRecord> {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let contents = std::fs::read_to_string(app.diagnostics.path())?;
            if let Some(record) = contents
                .lines()
                .filter_map(|line| serde_json::from_str::<OperationRecord>(line).ok())
                .nth(after)
            {
                return Ok::<_, anyhow::Error>(record);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Typed catalog operation record is absent."))?
}

#[googletest::test]
#[tokio::test]
async fn catalog_failure_emits_safe_typed_facts_through_packaged_composition() -> anyhow::Result<()>
{
    let app = Fixture::new().await?;
    let cookie = app.login("private-diagnostic-reader").await?;
    let ranking = app.ranking(&cookie).await?.text().await?;
    let csrf = regex::Regex::new(r#"name="csrf" value="([^"]+)"#)?
        .captures(&ranking)
        .unwrap()[1]
        .to_owned();
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .respond_with(ResponseTemplate::new(503).set_body_string("private provider payload"))
        .mount(&app.provider)
        .await;
    let response = app
        .client
        .post(format!("{}/search", app.base))
        .header("cookie", &cookie)
        .header("origin", "https://reader.example")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(
            url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs([
                    ("csrf", csrf.as_str()),
                    ("query", "private query"),
                    ("page", "1"),
                ])
                .finish(),
        )
        .send()
        .await?;
    assert_that!(response.status().as_u16(), eq(200));
    let record = next_record(&app, 0).await?;
    assert_that!(record.severity, eq(Severity::Warn));
    assert_that!(record.fact.event, eq(EventIdentity::OperationFinished));
    assert_that!(record.fact.operation, eq(OperationKind::CatalogSearch));
    assert_that!(record.fact.outcome, eq(OperationOutcome::DependencyFailure));
    assert_that!(record.fact.stage, eq(Stage::Provider));
    assert_that!(record.fact.reason, eq(Reason::Unavailable));
    assert_that!(record.fact.release.len(), eq(64));
    assert_that!(record.fact.duration_ms < 10_000, eq(true));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    Ok(())
}

async fn submit(
    app: &Fixture,
    base: &str,
    cookie: &str,
    csrf: &str,
    query: &str,
) -> anyhow::Result<reqwest::Response> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([("csrf", csrf), ("query", query), ("page", "1")])
        .finish();
    Ok(app
        .client
        .post(format!("{base}/search"))
        .header("cookie", cookie)
        .header("origin", "https://reader.example")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?)
}

#[googletest::test]
#[tokio::test]
async fn catalog_records_preserve_provider_rejections_context_and_delivery_failure()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("private-operation-reader").await?;
    let ranking = app.ranking(&cookie).await?.text().await?;
    let csrf = regex::Regex::new(r#"name="csrf" value="([^"]+)"#)?
        .captures(&ranking)
        .unwrap()[1]
        .to_owned();
    let package: serde_json::Value =
        serde_json::from_slice(&std::fs::read(app.package.path().join("integrity.json"))?)?;
    let release = package["binary"].as_str().unwrap();
    let cases = [
        (
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"start":0,"numFound":0,"docs":[]})),
            OperationOutcome::Confirmed,
            Stage::Selection,
            Reason::None,
            Severity::Info,
        ),
        (
            ResponseTemplate::new(429),
            OperationOutcome::Rejected,
            Stage::Provider,
            Reason::ProviderLimited,
            Severity::Info,
        ),
        (
            ResponseTemplate::new(200).set_body_string("private malformed payload"),
            OperationOutcome::DependencyFailure,
            Stage::Provider,
            Reason::Malformed,
            Severity::Warn,
        ),
        (
            ResponseTemplate::new(200).set_body_string("x".repeat(1_048_577)),
            OperationOutcome::DependencyFailure,
            Stage::Provider,
            Reason::Oversized,
            Severity::Warn,
        ),
        (
            ResponseTemplate::new(503),
            OperationOutcome::DependencyFailure,
            Stage::Provider,
            Reason::Unavailable,
            Severity::Warn,
        ),
        (
            ResponseTemplate::new(200).set_delay(Duration::from_secs(6)),
            OperationOutcome::DependencyFailure,
            Stage::Provider,
            Reason::Timeout,
            Severity::Warn,
        ),
    ];
    let mut events = Vec::new();
    for (index, (template, outcome, stage, reason, severity)) in cases.into_iter().enumerate() {
        app.provider.reset().await;
        sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at='-infinity'")
            .execute(&app.pool)
            .await?;
        Mock::given(method("GET"))
            .and(path("/search.json"))
            .respond_with(template)
            .mount(&app.provider)
            .await;
        assert_that!(
            submit(&app, &app.base, &cookie, &csrf, "private query")
                .await?
                .status()
                .as_u16(),
            eq(200)
        );
        let record = next_record(&app, index).await?;
        assert_that!(record.fact.event, eq(EventIdentity::OperationFinished));
        assert_that!(record.fact.operation, eq(OperationKind::CatalogSearch));
        assert_that!(record.fact.outcome, eq(outcome));
        assert_that!(record.fact.stage, eq(stage));
        assert_that!(record.fact.reason, eq(reason));
        assert_that!(record.severity, eq(severity));
        assert_that!(record.fact.release.as_str(), eq(release));
        assert_that!(record.fact.duration_ms < 15_000, eq(true));
        if reason == Reason::Timeout {
            assert_that!(record.fact.duration_ms >= 4_500, eq(true));
        }
        if let Some(first) = events.first() {
            let first: &crate::diagnostic_events::OperationFinished = first;
            assert_that!(record.fact.process_instance, eq(first.process_instance));
        }
        assert_that!(
            events
                .iter()
                .any(|event| event.operation_id == record.fact.operation_id),
            eq(false)
        );
        events.push(record.fact);
    }
    sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at=clock_timestamp()+interval '1 minute'").execute(&app.pool).await?;
    assert_that!(
        submit(&app, &app.base, &cookie, &csrf, "private query")
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    let limited = next_record(&app, 6).await?;
    assert_that!(limited.fact.outcome, eq(OperationOutcome::Rejected));
    assert_that!(limited.fact.reason, eq(Reason::AdmissionLimited));
    assert_that!(limited.fact.stage, eq(Stage::Admission));
    assert_that!(limited.severity, eq(Severity::Info));
    assert_that!(
        submit(&app, &app.base, &cookie, &csrf, "")
            .await?
            .status()
            .as_u16(),
        eq(400)
    );
    let rejected = next_record(&app, 7).await?;
    assert_that!(rejected.fact.reason, eq(Reason::InvalidInput));
    assert_that!(rejected.fact.stage, eq(Stage::Validation));
    assert_that!(rejected.fact.outcome, eq(OperationOutcome::Rejected));
    let (_second, base) = app.another_instance().await?;
    assert_that!(
        submit(&app, &base, &cookie, &csrf, "")
            .await?
            .status()
            .as_u16(),
        eq(400)
    );
    let other = next_record(&app, 8).await?;
    assert_that!(
        other.fact.process_instance == events[0].process_instance,
        eq(false)
    );
    assert_that!(other.fact.release.as_str(), eq(release));
    assert_that!(
        other.fact.operation_id == rejected.fact.operation_id,
        eq(false)
    );
    app.break_diagnostic_delivery().await?;
    app.provider.reset().await;
    sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at='-infinity'")
        .execute(&app.pool)
        .await?;
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"start":0,"numFound":0,"docs":[]})),
        )
        .mount(&app.provider)
        .await;
    assert_that!(
        submit(&app, &app.base, &cookie, &csrf, "private query")
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn catalog_schema_failure_keeps_its_stage_and_safe_reason() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("private-database-reader").await?;
    let ranking = app.ranking(&cookie).await?.text().await?;
    let csrf = regex::Regex::new(r#"name="csrf" value="([^"]+)"#)?
        .captures(&ranking)
        .unwrap()[1]
        .to_owned();
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"start":0,"numFound":0,"docs":[]})),
        )
        .mount(&app.provider)
        .await;
    assert_that!(
        submit(&app, &app.base, &cookie, &csrf, "private query")
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    assert_that!(
        next_record(&app, 0).await?.fact.outcome,
        eq(OperationOutcome::Confirmed)
    );
    sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at='-infinity'")
        .execute(&app.pool)
        .await?;
    sqlx::query("ALTER TABLE book_smartz_catalog.search_gate ADD CONSTRAINT diagnostic_fault CHECK (false) NOT VALID").execute(&app.pool).await?;
    assert_that!(
        submit(&app, &app.base, &cookie, &csrf, "private query")
            .await?
            .status()
            .as_u16(),
        eq(500)
    );
    let database = next_record(&app, 1).await?;
    assert_that!(database.fact.reason, eq(Reason::Database));
    assert_that!(
        database.fact.outcome,
        eq(OperationOutcome::DependencyFailure)
    );
    assert_that!(database.fact.stage, eq(Stage::Admission));
    assert_that!(database.severity, eq(Severity::Error));
    sqlx::query("ALTER TABLE book_smartz_catalog.search_gate DROP CONSTRAINT diagnostic_fault")
        .execute(&app.pool)
        .await?;
    assert_that!(
        submit(&app, &app.base, &cookie, &csrf, "private query")
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    assert_that!(
        next_record(&app, 2).await?.fact.outcome,
        eq(OperationOutcome::Confirmed)
    );
    sqlx::query("DROP TABLE book_smartz_catalog.search_gate")
        .execute(&app.pool)
        .await?;
    assert_that!(
        submit(&app, &app.base, &cookie, &csrf, "private query")
            .await?
            .status()
            .as_u16(),
        eq(500)
    );
    let record = next_record(&app, 3).await?;
    assert_that!(record.fact.stage, eq(Stage::Admission));
    assert_that!(record.fact.outcome, eq(OperationOutcome::DependencyFailure));
    assert_that!(record.fact.reason, eq(Reason::Schema));
    assert_that!(record.severity, eq(Severity::Error));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    Ok(())
}
