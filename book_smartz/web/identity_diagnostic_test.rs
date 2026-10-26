//! Identity and saved-read facts through the real packaged HTTP composition.
use std::time::Duration;

use googletest::{assert_that, matchers::eq};

use crate::{
    diagnostic_events::{
        OperationKind, OperationOutcome, OperationRecord, Reason, Severity, Stage,
    },
    test_support::Fixture,
};

async fn record(
    app: &Fixture,
    kind: OperationKind,
    index: usize,
) -> anyhow::Result<OperationRecord> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let contents = std::fs::read_to_string(app.diagnostics.path())?;
            if let Some(record) = contents
                .lines()
                .filter_map(|line| serde_json::from_str::<OperationRecord>(line).ok())
                .filter(|record| record.fact.operation == kind)
                .nth(index)
            {
                return Ok::<_, anyhow::Error>(record);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Typed identity or ranking operation record is absent."))?
}

#[googletest::test]
#[tokio::test]
async fn saved_ranking_records_its_completed_read_through_packaged_http() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("private-ranking-diagnostic-reader").await?;
    let response = app.ranking(&cookie).await?;
    assert_that!(response.status().as_u16(), eq(200));
    let record = record(&app, OperationKind::RankingRead, 0).await?;
    assert_that!(record.fact.outcome, eq(OperationOutcome::Confirmed));
    assert_that!(record.fact.reason, eq(Reason::None));
    assert_that!(record.severity, eq(Severity::Info));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn sign_in_reports_acknowledged_session_creation_with_safe_correlation() -> anyhow::Result<()>
{
    let app = Fixture::new().await?;
    let cookie = app.login("private-sign-in-diagnostic-reader").await?;
    let signed_in = record(&app, OperationKind::SignIn, 0).await?;
    assert_that!(signed_in.fact.outcome, eq(OperationOutcome::Confirmed));
    assert_that!(signed_in.fact.reason, eq(Reason::None));
    assert_that!(signed_in.severity, eq(Severity::Info));
    let response = app.ranking(&cookie).await?;
    assert_that!(response.status().as_u16(), eq(200));
    let read = record(&app, OperationKind::RankingRead, 0).await?;
    assert_that!(
        read.fact.process_instance,
        eq(signed_in.fact.process_instance)
    );
    assert_that!(&read.fact.release, eq(&signed_in.fact.release));
    assert_that!(
        read.fact.operation_id == signed_in.fact.operation_id,
        eq(false)
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn sign_in_distinguishes_provider_failure_from_rejected_identity() -> anyhow::Result<()> {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let app = Fixture::new().await?;
    let cases = [
        (
            ResponseTemplate::new(503).set_body_string("private provider error"),
            OperationOutcome::DependencyFailure,
            Reason::Unavailable,
            Severity::Warn,
        ),
        (
            ResponseTemplate::new(429),
            OperationOutcome::Rejected,
            Reason::ProviderLimited,
            Severity::Info,
        ),
        (
            ResponseTemplate::new(200).set_body_string("private malformed provider body"),
            OperationOutcome::DependencyFailure,
            Reason::Malformed,
            Severity::Warn,
        ),
        (
            ResponseTemplate::new(200).set_body_string("x".repeat(1_048_577)),
            OperationOutcome::DependencyFailure,
            Reason::Oversized,
            Severity::Warn,
        ),
        (
            ResponseTemplate::new(200).set_delay(Duration::from_secs(6)),
            OperationOutcome::DependencyFailure,
            Reason::Timeout,
            Severity::Warn,
        ),
    ];
    for (index, (template, outcome, reason, severity)) in cases.into_iter().enumerate() {
        let login = app.begin().await?;
        app.provider.reset().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(template)
            .mount(&app.provider)
            .await;
        let response = app
            .callback(&login, "private-code", &login.state, &login.cookie)
            .await?;
        assert_that!(response.status().as_u16(), eq(401));
        let failure = record(&app, OperationKind::SignIn, index).await?;
        assert_that!(failure.fact.outcome, eq(outcome));
        assert_that!(failure.fact.reason, eq(reason));
        assert_that!(failure.fact.stage, eq(Stage::Provider));
        assert_that!(failure.severity, eq(severity));
        if reason == Reason::Timeout {
            assert_that!(failure.fact.duration_ms >= 4_500, eq(true));
        }
    }
    for (offset, template) in [
        ResponseTemplate::new(503),
        ResponseTemplate::new(400).set_body_json(serde_json::json!({"error":"invalid_grant"})),
    ]
    .into_iter()
    .enumerate()
    {
        let login = app.begin().await?;
        app.provider.reset().await;
        Mock::given(method("GET"))
            .and(path("/keys"))
            .respond_with(template)
            .mount(&app.provider)
            .await;
        assert_that!(
            app.finish(&login, Fixture::claims(&login, "private-key-reader"))
                .await?
                .status()
                .as_u16(),
            eq(401)
        );
        let key_failure = record(&app, OperationKind::SignIn, 5 + offset).await?;
        assert_that!(
            key_failure.fact.outcome,
            eq(OperationOutcome::DependencyFailure)
        );
        assert_that!(key_failure.fact.reason, eq(Reason::Unavailable));
        assert_that!(key_failure.fact.stage, eq(Stage::Provider));
    }
    Ok(())
}

async fn csrf(app: &Fixture, cookie: &str) -> anyhow::Result<String> {
    let body = app.ranking(cookie).await?.text().await?;
    Ok(regex::Regex::new(r#"name="csrf" value="([^"]+)"#)?
        .captures(&body)
        .unwrap()[1]
        .to_owned())
}
async fn logout(app: &Fixture, cookie: &str, csrf: &str) -> anyhow::Result<reqwest::Response> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([("csrf", csrf)])
        .finish();
    Ok(app
        .client
        .post(format!("{}/logout", app.base))
        .header("cookie", cookie)
        .header("origin", "https://reader.example")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?)
}
#[googletest::test]
#[tokio::test]
async fn sign_out_records_confirmed_revocation_without_changing_session_behavior()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("private-sign-out-reader").await?;
    let token = csrf(&app, &cookie).await?;
    assert_that!(
        logout(&app, &cookie, &token).await?.status().as_u16(),
        eq(303)
    );
    let signed_out = record(&app, OperationKind::SignOut, 0).await?;
    assert_that!(signed_out.fact.outcome, eq(OperationOutcome::Confirmed));
    assert_that!(signed_out.fact.reason, eq(Reason::None));
    assert_that!(signed_out.severity, eq(Severity::Info));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(401));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn sign_in_start_reports_challenge_persistence_before_redirect() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let _ = app.begin().await?;
    let started = record(&app, OperationKind::SignInStart, 0).await?;
    assert_that!(started.fact.outcome, eq(OperationOutcome::Confirmed));
    sqlx::query("DROP TABLE book_smartz_web.login_challenges")
        .execute(&app.pool)
        .await?;
    let response = app
        .client
        .get(format!("{}/auth/google", app.base))
        .send()
        .await?;
    assert_that!(response.status().as_u16(), eq(500));
    let failed = record(&app, OperationKind::SignInStart, 1).await?;
    assert_that!(failed.fact.outcome, eq(OperationOutcome::DependencyFailure));
    assert_that!(failed.fact.reason, eq(Reason::Schema));
    assert_that!(failed.severity, eq(Severity::Error));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn identity_rejections_and_persistence_faults_keep_distinct_outcomes() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let rejected = app.begin().await?;
    let mut claims = Fixture::claims(&rejected, "private-rejected-reader");
    claims["aud"] = serde_json::json!("wrong-client");
    assert_that!(
        app.finish(&rejected, claims).await?.status().as_u16(),
        eq(401)
    );
    let rejected = record(&app, OperationKind::SignIn, 0).await?;
    assert_that!(rejected.fact.outcome, eq(OperationOutcome::Rejected));
    assert_that!(rejected.fact.reason, eq(Reason::IdentityRejected));
    assert_that!(rejected.fact.stage, eq(Stage::Authentication));
    assert_that!(rejected.severity, eq(Severity::Info));
    let cookie = app.login("private-existing-reader").await?;
    assert_that!(
        record(&app, OperationKind::SignIn, 1).await?.fact.outcome,
        eq(OperationOutcome::Confirmed)
    );
    sqlx::query("ALTER TABLE book_smartz_web.accounts ADD CONSTRAINT diagnostic_signup_fault CHECK (false) NOT VALID").execute(&app.pool).await?;
    let login = app.begin().await?;
    assert_that!(
        app.finish(&login, Fixture::claims(&login, "private-failed-reader"))
            .await?
            .status()
            .as_u16(),
        eq(500)
    );
    let failed = record(&app, OperationKind::SignIn, 2).await?;
    assert_that!(failed.fact.outcome, eq(OperationOutcome::DependencyFailure));
    assert_that!(failed.fact.reason, eq(Reason::Database));
    assert_that!(failed.fact.stage, eq(Stage::Persistence));
    assert_that!(failed.severity, eq(Severity::Error));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    sqlx::query("ALTER TABLE book_smartz_web.accounts DROP CONSTRAINT diagnostic_signup_fault")
        .execute(&app.pool)
        .await?;
    let _ = app.login("private-restored-reader").await?;
    assert_that!(
        record(&app, OperationKind::SignIn, 3).await?.fact.outcome,
        eq(OperationOutcome::Confirmed)
    );
    let token = csrf(&app, &cookie).await?;
    assert_that!(
        logout(&app, &cookie, "wrong-csrf").await?.status().as_u16(),
        eq(403)
    );
    let invalid = record(&app, OperationKind::SignOut, 0).await?;
    assert_that!(invalid.fact.outcome, eq(OperationOutcome::Rejected));
    assert_that!(invalid.fact.reason, eq(Reason::InvalidInput));
    assert_that!(invalid.fact.stage, eq(Stage::Validation));
    sqlx::query("CREATE FUNCTION book_smartz_web.diagnostic_revocation_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'private database fault' USING ERRCODE='23514'; END; $$").execute(&app.pool).await?;
    sqlx::query("CREATE TRIGGER diagnostic_revocation_fault BEFORE DELETE ON book_smartz_web.sessions FOR EACH ROW EXECUTE FUNCTION book_smartz_web.diagnostic_revocation_fault()").execute(&app.pool).await?;
    assert_that!(
        logout(&app, &cookie, &token).await?.status().as_u16(),
        eq(500)
    );
    let failed = record(&app, OperationKind::SignOut, 1).await?;
    assert_that!(failed.fact.outcome, eq(OperationOutcome::DependencyFailure));
    assert_that!(failed.fact.reason, eq(Reason::Database));
    assert_that!(failed.fact.stage, eq(Stage::SessionRevocation));
    assert_that!(failed.severity, eq(Severity::Error));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    sqlx::query("DROP TRIGGER diagnostic_revocation_fault ON book_smartz_web.sessions")
        .execute(&app.pool)
        .await?;
    assert_that!(
        logout(&app, &cookie, &token).await?.status().as_u16(),
        eq(303)
    );
    assert_that!(
        record(&app, OperationKind::SignOut, 2).await?.fact.outcome,
        eq(OperationOutcome::Confirmed)
    );
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(401));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn ranking_failure_is_observed_after_component_loading_and_survives_broken_output()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let denied = app
        .client
        .get(format!("{}/ranking", app.base))
        .send()
        .await?;
    assert_that!(denied.status().as_u16(), eq(401));
    let rejected = record(&app, OperationKind::RankingRead, 0).await?;
    assert_that!(rejected.fact.outcome, eq(OperationOutcome::Rejected));
    assert_that!(rejected.fact.stage, eq(Stage::Authentication));
    let cookie = app.login("private-failed-read-reader").await?;
    let normal = app.ranking(&cookie).await?;
    assert_that!(normal.status().as_u16(), eq(200));
    let _ = normal.bytes().await?;
    assert_that!(
        record(&app, OperationKind::RankingRead, 1)
            .await?
            .fact
            .outcome,
        eq(OperationOutcome::Confirmed)
    );
    sqlx::query("ALTER TABLE book_smartz_actions.display RENAME TO diagnostic_display_saved")
        .execute(&app.pool)
        .await?;
    let response = app.ranking(&cookie).await?;
    let expected_status = response.status().as_u16();
    let expected_body_available = response.bytes().await.is_ok();
    let failed = record(&app, OperationKind::RankingRead, 2).await?;
    assert_that!(failed.fact.outcome, eq(OperationOutcome::DependencyFailure));
    assert_that!(failed.fact.reason, eq(Reason::Schema));
    assert_that!(failed.fact.stage, eq(Stage::Persistence));
    assert_that!(failed.severity, eq(Severity::Error));
    sqlx::query("ALTER TABLE book_smartz_actions.diagnostic_display_saved RENAME TO display")
        .execute(&app.pool)
        .await?;
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    assert_that!(
        record(&app, OperationKind::RankingRead, 3)
            .await?
            .fact
            .outcome,
        eq(OperationOutcome::Confirmed)
    );
    app.break_diagnostic_delivery().await?;
    sqlx::query("ALTER TABLE book_smartz_actions.display RENAME TO diagnostic_display_saved")
        .execute(&app.pool)
        .await?;
    let response = app.ranking(&cookie).await?;
    assert_that!(response.status().as_u16(), eq(expected_status));
    assert_that!(response.bytes().await.is_ok(), eq(expected_body_available));
    sqlx::query("ALTER TABLE book_smartz_actions.diagnostic_display_saved RENAME TO display")
        .execute(&app.pool)
        .await?;
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    let token = csrf(&app, &cookie).await?;
    assert_that!(
        logout(&app, &cookie, &token).await?.status().as_u16(),
        eq(303)
    );
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(401));
    let fresh = app.login("private-broken-output-reader").await?;
    assert_that!(app.ranking(&fresh).await?.status().as_u16(), eq(200));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn rejected_oauth_grants_are_expected_rejections_not_provider_outages() -> anyhow::Result<()>
{
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let app = Fixture::new().await?;
    let cases = [
        (ResponseTemplate::new(400).set_body_json(serde_json::json!({"error":"invalid_grant","error_description":"private grant details"})),OperationOutcome::Rejected,Reason::IdentityRejected,Stage::Authentication,Severity::Info),
        (ResponseTemplate::new(400).set_body_json(serde_json::json!({"error":"invalid_client","error_description":"invalid_grant"})),OperationOutcome::DependencyFailure,Reason::Unavailable,Stage::Provider,Severity::Warn),
        (ResponseTemplate::new(400).set_body_string("private malformed error"),OperationOutcome::DependencyFailure,Reason::Malformed,Stage::Provider,Severity::Warn),
        (ResponseTemplate::new(400).set_body_string("x".repeat(1_048_577)),OperationOutcome::DependencyFailure,Reason::Oversized,Stage::Provider,Severity::Warn),
    ];
    for (index, (template, outcome, reason, stage, severity)) in cases.into_iter().enumerate() {
        let login = app.begin().await?;
        app.provider.reset().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(template)
            .mount(&app.provider)
            .await;
        assert_that!(
            app.callback(&login, "private-code", &login.state, &login.cookie)
                .await?
                .status()
                .as_u16(),
            eq(401)
        );
        let observed = record(&app, OperationKind::SignIn, index).await?;
        assert_that!(observed.fact.outcome, eq(outcome));
        assert_that!(observed.fact.reason, eq(reason));
        assert_that!(observed.fact.stage, eq(stage));
        assert_that!(observed.severity, eq(severity));
    }
    Ok(())
}
