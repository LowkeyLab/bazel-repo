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
    assert_that!(
        std::process::Command::new("kill")
            .args(["-TERM", &app.process.0.id().to_string()])
            .status()?
            .success(),
        eq(true)
    );
    assert_that!(
        wait_for_shutdown(&mut app.process).await?.success(),
        eq(true)
    );
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

#[googletest::test]
#[tokio::test]
async fn stalled_output_preserves_requests_and_reports_accumulated_loss_after_resuming()
-> anyhow::Result<()> {
    use crate::diagnostic_events::{DiagnosticLossRecord, EventIdentity, WorkerStatus};
    const REQUESTS: usize = 1_500;
    let mut app = Fixture::new().await?;
    let cookie = app.login("stalled-output-reader").await?;
    let mut reader = app.stall_diagnostic_delivery().await?;
    tokio::time::timeout(Duration::from_secs(45), async {
        for _ in 0..REQUESTS {
            let response = app.ranking(&cookie).await?;
            assert_that!(response.status().as_u16(), eq(200));
            let _ = response.text().await?;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    let response = tokio::time::timeout(Duration::from_secs(5), app.ranking(&cookie)).await??;
    assert_that!(response.status().as_u16(), eq(200));
    let _ = response.text().await?;
    let captured = tempfile::NamedTempFile::new()?;
    let mut output = captured.as_file().try_clone()?;
    let resumed = std::thread::spawn(move || std::io::copy(&mut reader, &mut output));
    let (records, loss) = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let contents = std::fs::read_to_string(captured.path())?;
            let records: Vec<OperationRecord> = contents
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();
            let loss: Vec<DiagnosticLossRecord> = contents
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();
            let dropped: u64 = loss.iter().map(|record| record.fact.queue_drops).sum();
            if !loss.is_empty()
                && u64::try_from(records.len())? + dropped == u64::try_from(REQUESTS + 1)?
            {
                break Ok::<_, anyhow::Error>((records, loss));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Accumulated diagnostic loss record is absent."))??;
    assert_that!(
        std::process::Command::new("kill")
            .arg("-TERM")
            .arg(app.process.0.id().to_string())
            .status()?
            .success(),
        eq(true)
    );
    app.process.0.wait()?;
    resumed
        .join()
        .map_err(|_| anyhow::anyhow!("Diagnostic capture failed."))??;
    assert_that!(
        loss.iter()
            .map(|record| record.fact.queue_drops)
            .sum::<u64>()
            > 0,
        eq(true)
    );
    assert_that!(
        records
            .iter()
            .all(|record| record.fact.operation == OperationKind::RankingRead
                && record.fact.outcome == OperationOutcome::Confirmed),
        eq(true)
    );
    let ids: Vec<_> = records
        .iter()
        .map(|record| record.fact.operation_id)
        .collect();
    assert_that!(
        ids.iter()
            .enumerate()
            .all(|(index, id)| !ids[..index].contains(id)),
        eq(true)
    );
    for record in loss {
        assert_that!(record.fact.event, eq(EventIdentity::DiagnosticLossObserved));
        assert_that!(record.fact.destination_failures, eq(0));
        assert_that!(record.fact.worker_status, eq(WorkerStatus::Running));
        assert_that!(record.severity, eq(Severity::Warn));
        assert_that!(
            record.fact.process_instance,
            eq(records[0].fact.process_instance)
        );
        assert_that!(&record.fact.release, eq(&records[0].fact.release));
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn migration_configuration_failure_reports_a_safe_terminal_fact() -> anyhow::Result<()> {
    use crate::diagnostic_events::{
        EventIdentity, InitializationOutcome, InitializationReason, InitializationRecord,
        InitializationStage,
    };
    let app = Fixture::new().await?;
    let capture = tempfile::NamedTempFile::new()?;
    let status = std::process::Command::new(app.package.path().join("book-smartz"))
        .arg("migrate")
        .env("DATABASE_URL", "private invalid database configuration")
        .current_dir(app.cwd.path())
        .stdout(std::process::Stdio::null())
        .stderr(capture.as_file().try_clone()?)
        .status()?;
    assert_that!(status.success(), eq(false));
    let contents = std::fs::read_to_string(capture.path())?;
    let records: Vec<InitializationRecord> = contents
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_that!(records.len(), eq(1));
    let record = records
        .first()
        .ok_or_else(|| anyhow::anyhow!("The initialization fact is absent."))?;
    assert_that!(record.fact.event, eq(EventIdentity::MigrationFinished));
    assert_that!(record.fact.outcome, eq(InitializationOutcome::Failed));
    assert_that!(record.fact.stage, eq(InitializationStage::Configuration));
    assert_that!(record.fact.reason, eq(InitializationReason::Configuration));
    assert_that!(record.fact.release.is_none(), eq(true));
    assert_that!(record.severity, eq(Severity::Error));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn serving_reports_verified_startup_before_reader_operations() -> anyhow::Result<()> {
    use crate::diagnostic_events::{
        EventIdentity, InitializationOutcome, InitializationReason, InitializationRecord,
        InitializationStage,
    };
    let app = Fixture::new().await?;
    let startup = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let contents = std::fs::read_to_string(app.diagnostics.path())?;
            let records: Vec<InitializationRecord> = contents
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();
            if let Some(record) = records.first() {
                assert_that!(records.len(), eq(1));
                return Ok::<_, anyhow::Error>(record.clone());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("The serving startup fact is absent."))??;
    assert_that!(startup.fact.event, eq(EventIdentity::StartupFinished));
    assert_that!(startup.fact.outcome, eq(InitializationOutcome::Succeeded));
    assert_that!(startup.fact.stage, eq(InitializationStage::Complete));
    assert_that!(startup.fact.reason, eq(InitializationReason::None));
    assert_that!(startup.severity, eq(Severity::Info));
    let cookie = app.login("private-startup-reader").await?;
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    let operation = record(&app, OperationKind::RankingRead, 0).await?;
    assert_that!(
        startup.fact.release.as_ref(),
        eq(Some(&operation.fact.release))
    );
    assert_that!(
        startup.fact.process_instance,
        eq(operation.fact.process_instance)
    );
    Ok(())
}

fn initialization_command(app: &Fixture, command: &str) -> std::process::Command {
    let mut child = std::process::Command::new(app.package.path().join("book-smartz"));
    child
        .arg(command)
        .current_dir(app.cwd.path())
        .env("DATABASE_URL", &app.database)
        .env("HOST", "127.0.0.1")
        .env("PORT", "0")
        .env("BOOK_SMARTZ_ORIGIN", "https://reader.example")
        .env("BOOK_SMARTZ_MAINTAINER_CONTACT", "maintainer@example.test")
        .env(
            "BOOK_SMARTZ_SELECTION_KEY",
            "public-fixture-key-at-least-thirty-two-bytes",
        )
        .env("GOOGLE_CLIENT_ID", "fixture-client")
        .env("GOOGLE_CLIENT_SECRET", "fixture-secret")
        .env(
            "GOOGLE_TOKEN_ENDPOINT",
            format!("{}/token", app.provider.uri()),
        )
        .stdout(std::process::Stdio::null());
    child
}

fn rejected_initialization(
    command: &mut std::process::Command,
) -> anyhow::Result<crate::diagnostic_events::InitializationRecord> {
    let capture = tempfile::NamedTempFile::new()?;
    command.stderr(capture.as_file().try_clone()?);
    assert_that!(command.status()?.success(), eq(false));
    let contents = std::fs::read_to_string(capture.path())?;
    let records: Vec<crate::diagnostic_events::InitializationRecord> = contents
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_that!(records.len(), eq(1));
    records
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("The initialization fact is absent."))
}

#[googletest::test]
#[tokio::test]
async fn startup_failures_keep_typed_causes_and_only_verified_release_context() -> anyhow::Result<()>
{
    use crate::diagnostic_events::{
        EventIdentity, InitializationOutcome, InitializationReason, InitializationStage,
    };
    let mut app = Fixture::new().await?;
    app.process.0.kill()?;
    app.process.0.wait()?;
    let integrity: serde_json::Value =
        serde_json::from_slice(&std::fs::read(app.package.path().join("integrity.json"))?)?;
    let release = integrity["binary"].as_str().unwrap().to_owned();
    let occupied = std::net::TcpListener::bind("127.0.0.1:0")?;
    let unused = std::net::TcpListener::bind("127.0.0.1:0")?;
    let unused_port = unused.local_addr()?.port();
    drop(unused);
    let cases = [
        (
            "DATABASE_URL",
            "private invalid database configuration".to_owned(),
            InitializationStage::Configuration,
            InitializationReason::Configuration,
            false,
        ),
        (
            "DATABASE_URL",
            format!("postgres://private-user:private-password@127.0.0.1:{unused_port}/private-db"),
            InitializationStage::Database,
            InitializationReason::Database,
            false,
        ),
        (
            "BOOK_SMARTZ_ORIGIN",
            "http://private-reader.example".to_owned(),
            InitializationStage::Configuration,
            InitializationReason::Configuration,
            true,
        ),
        (
            "GOOGLE_TOKEN_ENDPOINT",
            "file:///private-provider-configuration".to_owned(),
            InitializationStage::Provider,
            InitializationReason::Provider,
            true,
        ),
        (
            "PORT",
            occupied.local_addr()?.port().to_string(),
            InitializationStage::Listener,
            InitializationReason::Unavailable,
            true,
        ),
    ];
    for (key, value, stage, reason, verified) in cases {
        let record =
            rejected_initialization(initialization_command(&app, "serve").env(key, value))?;
        assert_that!(record.fact.event, eq(EventIdentity::StartupFinished));
        assert_that!(record.fact.outcome, eq(InitializationOutcome::Failed));
        assert_that!(record.fact.stage, eq(stage));
        assert_that!(record.fact.reason, eq(reason));
        assert_that!(
            record.fact.release.as_deref(),
            eq(verified.then_some(release.as_str()))
        );
        assert_that!(record.severity, eq(Severity::Error));
    }
    let manifest = app.package.path().join("assets/manifest.toml");
    let original = std::fs::read(&manifest)?;
    std::fs::write(&manifest, b"private mismatched manifest")?;
    let package = rejected_initialization(&mut initialization_command(&app, "serve"))?;
    assert_that!(package.fact.stage, eq(InitializationStage::Package));
    assert_that!(package.fact.reason, eq(InitializationReason::Package));
    assert_that!(package.fact.release.is_none(), eq(true));
    std::fs::write(&manifest, original)?;
    sqlx::query("DROP TABLE book_smartz_web.sessions")
        .execute(&app.pool)
        .await?;
    let schema = rejected_initialization(&mut initialization_command(&app, "serve"))?;
    assert_that!(schema.fact.stage, eq(InitializationStage::Schema));
    assert_that!(schema.fact.reason, eq(InitializationReason::Schema));
    assert_that!(schema.fact.release.as_deref(), eq(Some(release.as_str())));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn migration_facts_preserve_sessions_and_distinguish_checksum_failure() -> anyhow::Result<()>
{
    use crate::diagnostic_events::{
        EventIdentity, InitializationOutcome, InitializationReason, InitializationRecord,
        InitializationStage,
    };
    let app = Fixture::new().await?;
    let cookie = app.login("private-migration-reader").await?;
    let mut instances = Vec::new();
    for _ in 0..2 {
        let capture = tempfile::NamedTempFile::new()?;
        let status = initialization_command(&app, "migrate")
            .stderr(capture.as_file().try_clone()?)
            .status()?;
        assert_that!(status.success(), eq(true));
        let contents = std::fs::read_to_string(capture.path())?;
        let records: Vec<InitializationRecord> = contents
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        assert_that!(records.len(), eq(1));
        let record = &records[0];
        assert_that!(record.fact.event, eq(EventIdentity::MigrationFinished));
        assert_that!(record.fact.outcome, eq(InitializationOutcome::Succeeded));
        assert_that!(record.fact.stage, eq(InitializationStage::Complete));
        assert_that!(record.fact.reason, eq(InitializationReason::None));
        assert_that!(record.fact.release.is_none(), eq(true));
        assert_that!(record.severity, eq(Severity::Info));
        instances.push(record.fact.process_instance);
    }
    assert_that!(instances[0] == instances[1], eq(false));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    sqlx::query("UPDATE book_smartz_web._sqlx_migrations SET checksum = decode('00', 'hex') WHERE version = 1")
        .execute(&app.pool).await?;
    let failure = rejected_initialization(&mut initialization_command(&app, "migrate"))?;
    assert_that!(failure.fact.event, eq(EventIdentity::MigrationFinished));
    assert_that!(failure.fact.outcome, eq(InitializationOutcome::Failed));
    assert_that!(failure.fact.stage, eq(InitializationStage::Schema));
    assert_that!(failure.fact.reason, eq(InitializationReason::Schema));
    assert_that!(failure.fact.release.is_none(), eq(true));
    assert_that!(failure.severity, eq(Severity::Error));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn stalled_early_stderr_preserves_failure_exit() -> anyhow::Result<()> {
    use std::io::Write;
    let app = Fixture::new().await?;
    let (mut destination, _reader) = std::os::unix::net::UnixStream::pair()?;
    destination.set_nonblocking(true)?;
    loop {
        match destination.write(&[0; 8192]) {
            Ok(0) => anyhow::bail!("The controlled destination closed."),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => return Err(error.into()),
        }
    }
    destination.set_nonblocking(false)?;
    let descriptor: std::os::fd::OwnedFd = destination.into();
    let mut process = crate::test_support::Process(
        initialization_command(&app, "migrate")
            .env("DATABASE_URL", "private invalid database configuration")
            .stderr(std::fs::File::from(descriptor))
            .spawn()?,
    );
    let status = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(status) = process.0.try_wait()? {
                return Ok::<_, std::io::Error>(status);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    assert_that!(status.success(), eq(false));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn readiness_records_first_result_and_changes_without_repeating_probes() -> anyhow::Result<()>
{
    use crate::diagnostic_events::{
        EventIdentity, ReadinessReason, ReadinessRecord, ReadinessState,
    };
    async fn records(app: &Fixture, expected: usize) -> anyhow::Result<Vec<ReadinessRecord>> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let data = std::fs::read_to_string(app.diagnostics.path())?;
                let records: Vec<ReadinessRecord> = data
                    .lines()
                    .filter_map(|line| serde_json::from_str(line).ok())
                    .collect();
                if records.len() >= expected {
                    return Ok::<_, anyhow::Error>(records);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("The readiness transition is absent."))?
    }
    async fn probe(app: &Fixture, expected: u16) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(6), async {
            loop {
                if app
                    .client
                    .get(format!("{}/health/ready", app.base))
                    .send()
                    .await?
                    .status()
                    .as_u16()
                    == expected
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        for _ in 0..8 {
            assert_that!(
                app.client
                    .get(format!("{}/health/ready", app.base))
                    .send()
                    .await?
                    .status()
                    .as_u16(),
                eq(expected)
            );
        }
        Ok(())
    }
    let app = Fixture::new().await?;
    let initial = records(&app, 1).await?;
    assert_that!(initial.len(), eq(1));
    assert_that!(initial[0].fact.state, eq(ReadinessState::Ready));
    probe(&app, 200).await?;
    let manifest = app.package.path().join("assets/manifest.toml");
    let original = std::fs::read(&manifest)?;
    std::fs::write(&manifest, b"mismatched readiness fixture")?;
    probe(&app, 503).await?;
    sqlx::query("ALTER TABLE book_smartz_web.sessions RENAME TO hidden_sessions")
        .execute(&app.pool)
        .await?;
    std::fs::write(&manifest, original)?;
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            let _ = app
                .client
                .get(format!("{}/health/ready", app.base))
                .send()
                .await?;
            let contents = std::fs::read_to_string(app.diagnostics.path())?;
            if contents
                .lines()
                .filter_map(|line| serde_json::from_str::<ReadinessRecord>(line).ok())
                .any(|record| record.fact.reason == ReadinessReason::Schema)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    probe(&app, 503).await?;
    sqlx::query("ALTER TABLE book_smartz_web.hidden_sessions RENAME TO sessions")
        .execute(&app.pool)
        .await?;
    probe(&app, 200).await?;
    let observed = records(&app, 4).await?;
    assert_that!(observed.len(), eq(4));
    for (record, (state, reason, severity)) in observed.iter().zip([
        (ReadinessState::Ready, ReadinessReason::None, Severity::Info),
        (
            ReadinessState::Unavailable,
            ReadinessReason::Package,
            Severity::Error,
        ),
        (
            ReadinessState::Unavailable,
            ReadinessReason::Schema,
            Severity::Error,
        ),
        (ReadinessState::Ready, ReadinessReason::None, Severity::Info),
    ]) {
        assert_that!(record.fact.event, eq(EventIdentity::ReadinessChanged));
        assert_that!(record.fact.state, eq(state));
        assert_that!(record.fact.reason, eq(reason));
        assert_that!(record.severity, eq(severity));
        assert_that!(&record.fact.release, eq(&initial[0].fact.release));
        assert_that!(
            record.fact.process_instance,
            eq(initial[0].fact.process_instance)
        );
    }
    Ok(())
}

async fn wait_for_shutdown(
    process: &mut crate::test_support::Process,
) -> anyhow::Result<std::process::ExitStatus> {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Some(status) = process.0.try_wait()? {
                return Ok::<_, std::io::Error>(status);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("The process exceeded its cleanup deadline."))?
    .map_err(Into::into)
}

#[googletest::test]
#[tokio::test]
async fn stalled_shutdown_output_cannot_extend_the_packaged_cleanup_limit() -> anyhow::Result<()> {
    use std::io::Write;
    let app = Fixture::new().await?;
    let socket = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    drop(socket);
    let (mut destination, _reader) = std::os::unix::net::UnixStream::pair()?;
    destination.set_nonblocking(true)?;
    loop {
        match destination.write(&[0; 8192]) {
            Ok(0) => anyhow::bail!("The controlled destination closed."),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => return Err(error.into()),
        }
    }
    destination.set_nonblocking(false)?;
    let descriptor: std::os::fd::OwnedFd = destination.into();
    let mut process = crate::test_support::Process(
        initialization_command(&app, "serve")
            .env("PORT", port.to_string())
            .stderr(std::fs::File::from(descriptor))
            .spawn()?,
    );
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(response) = app
                .client
                .get(format!("http://127.0.0.1:{port}/health/ready"))
                .send()
                .await
                && response.status().as_u16() == 200
            {
                break;
            }
            if process.0.try_wait()?.is_some() {
                anyhow::bail!("The packaged process stopped before readiness.");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    let started = std::time::Instant::now();
    assert_that!(
        std::process::Command::new("kill")
            .args(["-TERM", &process.0.id().to_string()])
            .status()?
            .success(),
        eq(true)
    );
    assert_that!(wait_for_shutdown(&mut process).await?.success(), eq(true));
    assert_that!(started.elapsed() >= Duration::from_secs(4), eq(true));
    Ok(())
}
