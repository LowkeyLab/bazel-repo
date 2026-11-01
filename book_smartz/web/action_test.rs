//! Durable action and security acceptance through the packaged application.
use googletest::{
    assert_that,
    matchers::{contains_substring, eq, not},
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::test_support::Fixture;
use book_smartz_domain::{BookId, Command, CommandContext, EventId, ReaderId};
use book_smartz_storage::{OperationContext, Store, tracing_observer};

async fn reader(app: &Fixture, subject: &str) -> anyhow::Result<ReaderId> {
    Ok(ReaderId::new(
        sqlx::query_scalar("SELECT reader_id FROM book_smartz_web.accounts WHERE subject=$1")
            .bind(subject)
            .fetch_one(&app.pool)
            .await?,
    ))
}
fn store(app: &Fixture) -> Store {
    Store::new(app.pool.clone(), tracing_observer())
}
fn field(html: &str, name: &str) -> anyhow::Result<String> {
    Ok(
        regex::Regex::new(&format!(r#"name="{name}" value="([^"]+)"#))?
            .captures(html)
            .ok_or_else(|| anyhow::anyhow!("missing {name}"))?[1]
            .to_owned(),
    )
}
pub(crate) async fn post(
    app: &Fixture,
    cookie: &str,
    route: &str,
    values: &[(String, String)],
) -> anyhow::Result<reqwest::Response> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(values.iter().map(|(k, v)| (k, v)))
        .finish();
    Ok(app
        .client
        .post(format!("{}{route}", app.base))
        .header("cookie", cookie)
        .header("origin", "https://reader.example")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?)
}
fn display_cookie(response: &reqwest::Response) -> anyhow::Result<String> {
    Ok(response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with("__Host-book-smartz-feedback="))
        .ok_or_else(|| anyhow::anyhow!("Missing confirmed display cookie"))?
        .to_owned())
}
pub(crate) async fn offer(
    app: &Fixture,
    cookie: &str,
    reader: ReaderId,
    work: Value,
    expires: i64,
) -> anyhow::Result<Vec<(String, String)>> {
    let csrf = field(&app.ranking(cookie).await?.text().await?, "csrf")?;
    let selection = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &json!({"iss":"book-smartz-selection","aud":"book-smartz-add","reader":reader.as_uuid(),"iat":expires-900,"exp":expires,"work":work}),
        &jsonwebtoken::EncodingKey::from_secret(b"public-fixture-key-at-least-thirty-two-bytes"),
    )?;
    let response = post(
        app,
        cookie,
        "/select",
        &[
            ("csrf".into(), csrf.clone()),
            ("selection".into(), selection.clone()),
        ],
    )
    .await?;
    anyhow::ensure!(response.status().as_u16() == 200, "selection rejected");
    let html = response.text().await?;
    Ok(vec![
        ("csrf".into(), csrf),
        ("selection".into(), selection),
        ("action".into(), field(&html, "action")?),
        ("revision".into(), field(&html, "revision")?),
        ("offer".into(), field(&html, "offer")?),
    ])
}
pub(crate) fn work(id: &str, title: &str) -> Value {
    json!({"work_id":id,"title":title,"authors":["Saved Author"],"cover":123,"year":null})
}
fn value<'a>(values: &'a [(String, String)], key: &str) -> &'a str {
    &values.iter().find(|(k, _)| k == key).unwrap().1
}

async fn operation_record(
    app: &Fixture,
    kind: crate::diagnostic_events::OperationKind,
    index: usize,
) -> anyhow::Result<crate::diagnostic_events::OperationRecord> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(record) = std::fs::read_to_string(app.diagnostics.path())?
                .lines()
                .filter_map(|line| {
                    serde_json::from_str::<crate::diagnostic_events::OperationRecord>(line).ok()
                })
                .filter(|record| record.fact.operation == kind)
                .nth(index)
            {
                return Ok::<_, anyhow::Error>(record);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("Typed operation record is absent."))?
}

async fn assert_lost_acknowledgement_fact(
    app: &Fixture,
    kind: crate::diagnostic_events::OperationKind,
    values: &[(String, String)],
) -> anyhow::Result<()> {
    use crate::diagnostic_events::{OperationOutcome, OriginalActionId, Reason, Severity, Stage};
    let record = operation_record(app, kind, 0).await?;
    assert_that!(
        record.fact.outcome,
        eq(OperationOutcome::UncertainPersistence)
    );
    assert_that!(record.fact.reason, eq(Reason::CommitUncertain));
    assert_that!(record.fact.stage, eq(Stage::Persistence));
    assert_that!(record.severity, eq(Severity::Warn));
    assert_that!(
        record.fact.original_action_id,
        eq(Some(OriginalActionId::from(Uuid::parse_str(value(
            values, "action"
        ))?)))
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn add_facts_distinguish_acknowledged_save_from_duplicate_attempt() -> anyhow::Result<()> {
    use crate::diagnostic_events::{
        OperationKind, OperationOutcome, OriginalActionId, Reason, Severity, Stage,
    };
    let app = Fixture::new().await?;
    let cookie = app.login("private-mutation-diagnostic-reader").await?;
    let id = reader(&app, "private-mutation-diagnostic-reader").await?;
    let values = offer(
        &app,
        &cookie,
        id,
        work("OL998W", "Private diagnostic title"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let saved = operation_record(&app, OperationKind::Add, 0).await?;
    assert_that!(saved.fact.outcome, eq(OperationOutcome::Confirmed));
    assert_that!(saved.fact.stage, eq(Stage::Persistence));
    assert_that!(saved.fact.reason, eq(Reason::None));
    assert_that!(saved.severity, eq(Severity::Info));
    assert_that!(
        saved.fact.original_action_id,
        eq(Some(OriginalActionId::from(Uuid::parse_str(value(
            &values, "action"
        ))?)))
    );
    let history = store(&app).load_application_history(id).await?;
    let ranking = store(&app).load_ranking_documents(id).await?;
    assert_that!(ranking.len(), eq(1));
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let duplicate = operation_record(&app, OperationKind::Add, 1).await?;
    assert_that!(duplicate.fact.outcome, eq(OperationOutcome::Confirmed));
    assert_that!(duplicate.fact.reason, eq(Reason::ExistingSaved));
    assert_that!(
        duplicate.fact.original_action_id,
        eq(saved.fact.original_action_id)
    );
    assert_that!(
        duplicate.fact.operation_id == saved.fact.operation_id,
        eq(false)
    );
    assert_that!(
        store(&app).load_application_history(id).await?,
        eq(&history)
    );
    assert_that!(store(&app).load_ranking_documents(id).await?, eq(&ranking));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn placement_facts_distinguish_pause_resume_preference_and_rejected_input()
-> anyhow::Result<()> {
    use crate::diagnostic_events::{
        OperationKind, OperationOutcome, OriginalActionId, Reason, Severity, Stage,
    };
    let app = Fixture::new().await?;
    let cookie = app.login("placement-diagnostic-reader").await?;
    let id = reader(&app, "placement-diagnostic-reader").await?;
    add_work(&app, &cookie, id, "OL991W", "One").await?;
    add_work(&app, &cookie, id, "OL992W", "Candidate").await?;
    for (route, kind) in [
        ("/pause", OperationKind::Pause),
        ("/resume", OperationKind::Resume),
        ("/answer", OperationKind::Preference),
    ] {
        let values = placement_form(&app.ranking(&cookie).await?.text().await?, route)?;
        let before = store(&app).load_ranking_documents(id).await?;
        let mut invalid = values.clone();
        invalid
            .iter_mut()
            .find(|(name, _)| name == "csrf")
            .unwrap()
            .1 = "wrong".into();
        assert_that!(
            post(&app, &cookie, route, &invalid)
                .await?
                .status()
                .as_u16(),
            eq(403)
        );
        let rejected = operation_record(&app, kind, 0).await?;
        assert_that!(rejected.fact.outcome, eq(OperationOutcome::Rejected));
        assert_that!(rejected.fact.reason, eq(Reason::InvalidInput));
        assert_that!(rejected.fact.original_action_id, eq(None));
        assert_that!(store(&app).load_ranking_documents(id).await?, eq(&before));
        assert_that!(
            post(&app, &cookie, route, &values).await?.status().as_u16(),
            eq(303)
        );
        let saved = operation_record(&app, kind, 1).await?;
        assert_that!(saved.fact.outcome, eq(OperationOutcome::Confirmed));
        assert_that!(saved.fact.reason, eq(Reason::None));
        assert_that!(saved.fact.stage, eq(Stage::Persistence));
        assert_that!(saved.severity, eq(Severity::Info));
        assert_that!(
            saved.fact.original_action_id,
            eq(Some(OriginalActionId::from(Uuid::parse_str(value(
                &values, "action"
            ))?)))
        );
        assert_that!(
            saved.fact.operation_id == rejected.fact.operation_id,
            eq(false)
        );
        assert_that!(
            store(&app).load_ranking_documents(id).await?.len(),
            eq(before.len() + 1)
        );
        let history = store(&app).load_application_history(id).await?;
        let ranking = store(&app).load_ranking_documents(id).await?;
        for (endpoint, message) in [
            (route, "We already saved this."),
            ("/recover", "We confirmed your save."),
        ] {
            let response = post(&app, &cookie, endpoint, &values).await?;
            assert_that!(response.status().as_u16(), eq(303));
            let receipt = display_cookie(&response)?;
            let html = app
                .ranking(&format!("{cookie}; {}", receipt.split(';').next().unwrap()))
                .await?
                .text()
                .await?;
            assert_that!(html.as_str(), contains_substring(message));
            assert_that!(
                html.as_str(),
                not(contains_substring("We saved this book at position"))
            );
            assert_that!(
                store(&app).load_application_history(id).await?,
                eq(&history)
            );
            assert_that!(store(&app).load_ranking_documents(id).await?, eq(&ranking));
        }
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn mutation_facts_preserve_known_failures_and_unresolved_original_saves() -> anyhow::Result<()>
{
    use crate::diagnostic_events::{
        OperationKind, OperationOutcome, OriginalActionId, Reason, Severity, Stage,
    };
    let app = Fixture::new().await?;
    let cookie = app.login("mutation-failure-reader").await?;
    let id = reader(&app, "mutation-failure-reader").await?;
    let values = offer(
        &app,
        &cookie,
        id,
        work("OL993W", "Private failure title"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    sqlx::raw_sql("CREATE FUNCTION reject_diagnostic_intent() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'private dependency cause'; END $$; CREATE TRIGGER reject_diagnostic_intent BEFORE INSERT ON book_smartz.application_events FOR EACH ROW EXECUTE FUNCTION reject_diagnostic_intent()")
        .execute(&app.pool).await?;
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(500)
    );
    let failed = operation_record(&app, OperationKind::Add, 0).await?;
    assert_that!(failed.fact.outcome, eq(OperationOutcome::DependencyFailure));
    assert_that!(failed.fact.reason, eq(Reason::Database));
    assert_that!(failed.fact.stage, eq(Stage::Persistence));
    assert_that!(failed.severity, eq(Severity::Error));
    assert_that!(store(&app).load_application_history(id).await?.len(), eq(0));
    assert_that!(store(&app).load_ranking_documents(id).await?.len(), eq(0));
    sqlx::query("DROP TRIGGER reject_diagnostic_intent ON book_smartz.application_events")
        .execute(&app.pool)
        .await?;
    reject_ranking(&app).await?;
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    let unresolved = operation_record(&app, OperationKind::Add, 1).await?;
    assert_that!(
        unresolved.fact.outcome,
        eq(OperationOutcome::UncertainPersistence)
    );
    assert_that!(unresolved.fact.reason, eq(Reason::Database));
    assert_that!(unresolved.severity, eq(Severity::Error));
    let action = Some(OriginalActionId::from(Uuid::parse_str(value(
        &values, "action",
    ))?));
    assert_that!(failed.fact.original_action_id, eq(action));
    assert_that!(unresolved.fact.original_action_id, eq(action));
    let frozen = store(&app).load_application_history(id).await?;
    assert_that!(frozen.len(), eq(3));
    assert_that!(store(&app).load_ranking_documents(id).await?.len(), eq(0));
    sqlx::query("DROP TRIGGER reject_web_ranking ON book_smartz.ranking_events")
        .execute(&app.pool)
        .await?;
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let saved = operation_record(&app, OperationKind::Add, 2).await?;
    assert_that!(saved.fact.outcome, eq(OperationOutcome::Confirmed));
    assert_that!(saved.fact.reason, eq(Reason::None));
    assert_that!(saved.fact.original_action_id, eq(action));
    assert_that!(store(&app).load_application_history(id).await?, eq(&frozen));
    let ranked = store(&app).load_ranking_documents(id).await?;
    assert_that!(ranked.len(), eq(1));
    let duplicate_book = offer(
        &app,
        &cookie,
        id,
        work("OL993W", "Private failure title"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        post(&app, &cookie, "/add", &duplicate_book)
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    let rejected = operation_record(&app, OperationKind::Add, 3).await?;
    assert_that!(rejected.fact.outcome, eq(OperationOutcome::Rejected));
    assert_that!(rejected.fact.reason, eq(Reason::ActionRejected));
    assert_that!(rejected.severity, eq(Severity::Info));
    assert_that!(store(&app).load_ranking_documents(id).await?, eq(&ranked));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn mutation_schema_failure_preserves_history_and_allows_a_restored_control()
-> anyhow::Result<()> {
    use crate::diagnostic_events::{
        OperationKind, OperationOutcome, OriginalActionId, Reason, Severity, Stage,
    };
    let app = Fixture::new().await?;
    let cookie = app.login("mutation-schema-reader").await?;
    let id = reader(&app, "mutation-schema-reader").await?;
    for (route, kind) in [
        ("/add", OperationKind::Add),
        ("/answer", OperationKind::Preference),
    ] {
        let values = if route == "/add" {
            offer(
                &app,
                &cookie,
                id,
                work("OL994W", "One"),
                chrono::Utc::now().timestamp() + 900,
            )
            .await?
        } else {
            add_work(&app, &cookie, id, "OL995W", "Candidate").await?;
            preference(&app, &cookie, "candidate").await?
        };
        let history = store(&app).load_application_history(id).await?;
        let ranking = store(&app).load_ranking_documents(id).await?;
        sqlx::query(
            "ALTER TABLE book_smartz.application_events RENAME TO unavailable_diagnostic_events",
        )
        .execute(&app.pool)
        .await?;
        let response = post(&app, &cookie, route, &values).await?;
        assert_that!(response.status().as_u16(), eq(200));
        let html = response.text().await?;
        assert_that!(
            html.as_str(),
            contains_substring(
                "We cannot confirm your save yet. Return to your books to check your save."
            )
        );
        assert_that!(html.as_str(), contains_substring("Check your save"));
        assert_that!(
            html.as_str(),
            not(contains_substring("action=\"/recover\""))
        );
        assert_that!(html.as_str(), contains_substring("Return to your books"));
        let failure = operation_record(&app, kind, 0).await?;
        assert_that!(
            failure.fact.outcome,
            eq(OperationOutcome::DependencyFailure)
        );
        assert_that!(failure.fact.reason, eq(Reason::Schema));
        assert_that!(failure.fact.stage, eq(Stage::Persistence));
        assert_that!(failure.severity, eq(Severity::Error));
        assert_that!(
            failure.fact.original_action_id,
            eq(Some(OriginalActionId::from(Uuid::parse_str(value(
                &values, "action"
            ))?)))
        );
        sqlx::query(
            "ALTER TABLE book_smartz.unavailable_diagnostic_events RENAME TO application_events",
        )
        .execute(&app.pool)
        .await?;
        assert_that!(
            store(&app).load_application_history(id).await?,
            eq(&history)
        );
        assert_that!(store(&app).load_ranking_documents(id).await?, eq(&ranking));
        assert_that!(
            post(&app, &cookie, route, &values).await?.status().as_u16(),
            eq(303)
        );
        let restored = operation_record(&app, kind, 1).await?;
        assert_that!(restored.fact.outcome, eq(OperationOutcome::Confirmed));
        assert_that!(restored.fact.reason, eq(Reason::None));
        assert_that!(
            restored.fact.original_action_id,
            eq(failure.fact.original_action_id)
        );
        assert_that!(
            restored.fact.operation_id == failure.fact.operation_id,
            eq(false)
        );
        assert_that!(
            store(&app).load_ranking_documents(id).await?.len(),
            eq(ranking.len() + 1)
        );
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn original_save_check_reports_unresolved_fresh_and_existing_results() -> anyhow::Result<()> {
    use crate::diagnostic_events::{
        OperationKind, OperationOutcome, OriginalActionId, Reason, Severity, Stage,
    };
    let app = Fixture::new().await?;
    let cookie = app.login("original-check-reader").await?;
    let id = reader(&app, "original-check-reader").await?;
    let values = offer(
        &app,
        &cookie,
        id,
        work("OL996W", "Private original title"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    reject_ranking(&app).await?;
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    let frozen = store(&app).load_application_history(id).await?;
    assert_that!(frozen.len(), eq(3));
    assert_that!(store(&app).load_ranking_documents(id).await?.len(), eq(0));
    let action = Some(OriginalActionId::from(Uuid::parse_str(value(
        &values, "action",
    ))?));
    let recovery = vec![
        ("csrf".into(), value(&values, "csrf").into()),
        ("action".into(), value(&values, "action").into()),
    ];
    assert_that!(
        post(&app, &cookie, "/recover", &recovery)
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    let unresolved = operation_record(&app, OperationKind::OriginalSaveCheck, 0).await?;
    assert_that!(
        unresolved.fact.outcome,
        eq(OperationOutcome::UncertainPersistence)
    );
    assert_that!(unresolved.fact.reason, eq(Reason::Database));
    assert_that!(unresolved.fact.stage, eq(Stage::Persistence));
    assert_that!(unresolved.fact.original_action_id, eq(action));
    assert_that!(unresolved.severity, eq(Severity::Error));
    assert_that!(store(&app).load_application_history(id).await?, eq(&frozen));
    assert_that!(store(&app).load_ranking_documents(id).await?.len(), eq(0));
    let blocked = offer(
        &app,
        &cookie,
        id,
        work("OL997W", "Blocked private title"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        post(&app, &cookie, "/add", &blocked)
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    assert_that!(store(&app).load_application_history(id).await?, eq(&frozen));
    sqlx::query("DROP TRIGGER reject_web_ranking ON book_smartz.ranking_events")
        .execute(&app.pool)
        .await?;
    let response = post(&app, &cookie, "/recover", &recovery).await?;
    assert_that!(response.status().as_u16(), eq(303));
    let receipt = display_cookie(&response)?;
    let html = app
        .ranking(&format!("{cookie}; {}", receipt.split(';').next().unwrap()))
        .await?
        .text()
        .await?;
    assert_that!(html.as_str(), contains_substring("We confirmed your save."));
    assert_that!(
        html.as_str(),
        not(contains_substring("We saved this book at position"))
    );
    let fresh = operation_record(&app, OperationKind::OriginalSaveCheck, 1).await?;
    assert_that!(fresh.fact.outcome, eq(OperationOutcome::Confirmed));
    assert_that!(fresh.fact.reason, eq(Reason::None));
    assert_that!(fresh.fact.original_action_id, eq(action));
    assert_that!(fresh.severity, eq(Severity::Info));
    let saved = store(&app).load_ranking_documents(id).await?;
    assert_that!(saved.len(), eq(1));
    assert_that!(store(&app).load_application_history(id).await?, eq(&frozen));
    let expected = &frozen.last().unwrap().envelope()["data"]["document"];
    assert_that!(
        serde_json::from_str::<Value>(&book_smartz_domain::encode_document(&saved[0])?)?,
        eq(expected)
    );
    assert_that!(
        post(&app, &cookie, "/recover", &recovery)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let existing = operation_record(&app, OperationKind::OriginalSaveCheck, 2).await?;
    assert_that!(existing.fact.outcome, eq(OperationOutcome::Confirmed));
    assert_that!(existing.fact.reason, eq(Reason::ExistingSaved));
    assert_that!(existing.fact.original_action_id, eq(action));
    assert_that!(
        existing.fact.operation_id == fresh.fact.operation_id,
        eq(false)
    );
    assert_that!(
        fresh.fact.operation_id == unresolved.fact.operation_id,
        eq(false)
    );
    assert_that!(store(&app).load_application_history(id).await?, eq(&frozen));
    assert_that!(store(&app).load_ranking_documents(id).await?, eq(&saved));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    assert_that!(store(&app).load_application_history(id).await?, eq(&frozen));
    assert_that!(store(&app).load_ranking_documents(id).await?, eq(&saved));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn original_save_check_reports_existing_rejection_and_missing_action() -> anyhow::Result<()> {
    use crate::diagnostic_events::{
        OperationKind, OperationOutcome, OriginalActionId, Reason, Severity,
    };
    let app = Fixture::new().await?;
    let cookie = app.login("rejected-original-reader").await?;
    let id = reader(&app, "rejected-original-reader").await?;
    add_work(&app, &cookie, id, "OL990W", "Already ranked").await?;
    let values = offer(
        &app,
        &cookie,
        id,
        work("OL990W", "Already ranked"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    let history = store(&app).load_application_history(id).await?;
    let ranking = store(&app).load_ranking_documents(id).await?;
    let recovery = vec![
        ("csrf".into(), value(&values, "csrf").into()),
        ("action".into(), value(&values, "action").into()),
    ];
    let mut previous = None;
    for index in 0..2 {
        assert_that!(
            post(&app, &cookie, "/recover", &recovery)
                .await?
                .status()
                .as_u16(),
            eq(200)
        );
        let rejected = operation_record(&app, OperationKind::OriginalSaveCheck, index).await?;
        assert_that!(rejected.fact.outcome, eq(OperationOutcome::Rejected));
        assert_that!(rejected.fact.reason, eq(Reason::ActionRejected));
        assert_that!(rejected.severity, eq(Severity::Info));
        assert_that!(
            rejected.fact.original_action_id,
            eq(Some(OriginalActionId::from(Uuid::parse_str(value(
                &values, "action"
            ))?)))
        );
        assert_that!(Some(rejected.fact.operation_id) == previous, eq(false));
        previous = Some(rejected.fact.operation_id);
    }
    let missing = vec![
        ("csrf".into(), value(&values, "csrf").into()),
        ("action".into(), Uuid::new_v4().to_string()),
    ];
    assert_that!(
        post(&app, &cookie, "/recover", &missing)
            .await?
            .status()
            .as_u16(),
        eq(403)
    );
    let absent = operation_record(&app, OperationKind::OriginalSaveCheck, 2).await?;
    assert_that!(absent.fact.outcome, eq(OperationOutcome::Rejected));
    assert_that!(absent.fact.reason, eq(Reason::InvalidInput));
    assert_that!(
        store(&app).load_application_history(id).await?,
        eq(&history)
    );
    assert_that!(store(&app).load_ranking_documents(id).await?, eq(&ranking));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn original_save_check_lost_acknowledgement_and_cancellation_preserve_one_original()
-> anyhow::Result<()> {
    use crate::diagnostic_events::{OperationKind, OperationOutcome, OriginalActionId, Reason};
    for cancel in [false, true] {
        let mut app = Fixture::new().await?;
        let cookie = app.login("original-check-ack-reader").await?;
        let id = reader(&app, "original-check-ack-reader").await?;
        let values = offer(
            &app,
            &cookie,
            id,
            work("OL989W", "Frozen original"),
            chrono::Utc::now().timestamp() + 900,
        )
        .await?;
        reject_ranking(&app).await?;
        assert_that!(
            post(&app, &cookie, "/add", &values)
                .await?
                .status()
                .as_u16(),
            eq(200)
        );
        let frozen = store(&app).load_application_history(id).await?;
        sqlx::query("DROP TRIGGER reject_web_ranking ON book_smartz.ranking_events")
            .execute(&app.pool)
            .await?;
        let recovery = vec![
            ("csrf".into(), value(&values, "csrf").into()),
            ("action".into(), value(&values, "action").into()),
        ];
        let direct = app.database.clone();
        let proxy = commit_proxy::CommitProxy::new(&app.pool, id).await?;
        app.database = proxy.database(&direct)?;
        app.restart().await?;
        let client = app.client.clone();
        let target = format!("{}/recover", app.base);
        let auth = cookie.clone();
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(recovery.iter().map(|(k, v)| (k, v)))
            .finish();
        let request = tokio::spawn(async move {
            client
                .post(target)
                .header("cookie", auth)
                .header("origin", "https://reader.example")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .send()
                .await
        });
        proxy.commit_reached().await?;
        let saved = store(&app).load_ranking_documents(id).await?;
        assert_that!(saved.len(), eq(1));
        assert_that!(
            serde_json::from_str::<Value>(&book_smartz_domain::encode_document(&saved[0])?)?,
            eq(&frozen.last().unwrap().envelope()["data"]["document"])
        );
        if cancel {
            request.abort();
            app.process.0.kill()?;
            app.process.0.wait()?;
            proxy.disconnect();
        } else {
            proxy.disconnect();
            assert_that!(request.await??.status().as_u16(), eq(200));
            assert_lost_acknowledgement_fact(&app, OperationKind::OriginalSaveCheck, &recovery)
                .await?;
        }
        app.database = direct;
        app.restart().await?;
        assert_that!(
            post(&app, &cookie, "/recover", &recovery)
                .await?
                .status()
                .as_u16(),
            eq(303)
        );
        let existing =
            operation_record(&app, OperationKind::OriginalSaveCheck, usize::from(!cancel)).await?;
        assert_that!(existing.fact.outcome, eq(OperationOutcome::Confirmed));
        assert_that!(existing.fact.reason, eq(Reason::ExistingSaved));
        assert_that!(
            existing.fact.original_action_id,
            eq(Some(OriginalActionId::from(Uuid::parse_str(value(
                &values, "action"
            ))?)))
        );
        assert_that!(store(&app).load_application_history(id).await?, eq(&frozen));
        assert_that!(store(&app).load_ranking_documents(id).await?, eq(&saved));
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn protected_actions_and_read_only_views_preserve_empty_history() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("security-reader").await?;
    let id = reader(&app, "security-reader").await?;
    let values = offer(
        &app,
        &cookie,
        id,
        work("OL1W", "Security Book"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    // Reading a fresh ranking and offering an action must not create business state.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM book_smartz.reader_streams WHERE reader_id=$1)",
    )
    .bind(id.as_uuid())
    .fetch_one(&app.pool)
    .await?;
    assert_that!(exists, eq(false));
    for route in ["/add", "/recover"] {
        assert_that!(
            app.client
                .get(format!("{}{route}", app.base))
                .send()
                .await?
                .status()
                .as_u16(),
            eq(405)
        );
        assert_that!(
            post(&app, "", route, &values).await?.status().as_u16(),
            eq(401)
        );
        let mut invalid = values.clone();
        invalid[0].1 = "wrong".into();
        assert_that!(
            post(&app, &cookie, route, &invalid)
                .await?
                .status()
                .as_u16(),
            eq(403)
        );
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(values.iter().map(|(k, v)| (k, v)))
            .finish();
        let response = app
            .client
            .post(format!("{}{route}", app.base))
            .header("cookie", &cookie)
            .header("origin", "https://attacker.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await?;
        assert_that!(response.status().as_u16(), eq(403));
    }
    let other = app.login("other-security-reader").await?;
    let other_csrf = field(&app.ranking(&other).await?.text().await?, "csrf")?;
    let mut invalid = values.clone();
    invalid[0].1 = other_csrf;
    assert_that!(
        post(&app, &other, "/add", &invalid)
            .await?
            .status()
            .as_u16(),
        eq(403)
    );
    for key in ["selection", "action", "revision", "offer"] {
        let mut invalid = values.clone();
        invalid.iter_mut().find(|(k, _)| k == key).unwrap().1 = if key == "action" {
            Uuid::new_v4().to_string()
        } else if key == "revision" {
            "1".into()
        } else {
            "tampered".into()
        };
        assert_that!(
            post(&app, &cookie, "/add", &invalid)
                .await?
                .status()
                .as_u16(),
            eq(403)
        );
    }
    assert_that!(store(&app).load_application_history(id).await?.len(), eq(0));
    assert_that!(store(&app).load_ranking_documents(id).await?.len(), eq(0));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn concurrent_registration_converges_and_rejected_start_retains_metadata()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let a = app.login("registration-a").await?;
    let b = app.login("registration-b").await?;
    let ar = reader(&app, "registration-a").await?;
    let br = reader(&app, "registration-b").await?;
    let expiry = chrono::Utc::now().timestamp() + 900;
    let av = offer(&app, &a, ar, work("OL2W", "Original title"), expiry).await?;
    let bv = offer(&app, &b, br, work("OL2W", "Original title"), expiry).await?;
    let stale = offer(
        &app,
        &a,
        ar,
        work("OL3W", "Registered but not ranked"),
        expiry,
    )
    .await?;
    let (one, two) = tokio::join!(post(&app, &a, "/add", &av), post(&app, &b, "/add", &bv));
    assert_that!(one?.status().as_u16(), eq(303));
    assert_that!(two?.status().as_u16(), eq(303));
    let sa = store(&app).load_ranking_documents(ar).await?;
    let sb = store(&app).load_ranking_documents(br).await?;
    assert_that!(sa[0].event().candidate(), eq(sb[0].event().candidate()));
    let rejected = post(&app, &a, "/add", &stale).await?.text().await?;
    assert_that!(
        rejected.as_str(),
        contains_substring("Your books changed. Review them before you try again.")
    );
    assert_that!(
        rejected.as_str(),
        contains_substring("<title>Review your books · Book Smartz</title>")
    );
    assert_that!(
        rejected.as_str(),
        contains_substring(">Review your books</h1>")
    );
    assert_that!(
        store(&app)
            .book_by_work_id(
                &book_smartz_domain::OpenLibraryWorkId::try_from("OL3W")?,
                OperationContext::default()
            )
            .await?
            .is_some(),
        eq(true)
    );
    let refresh = offer(
        &app,
        &a,
        ar,
        json!({"work_id":"OL2W","title":"Refreshed title","authors":[],"cover":null,"year":null}),
        expiry,
    )
    .await?;
    assert_that!(
        post(&app, &a, "/add", &refresh)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("You already added this book.")
    );
    let body = app.ranking(&a).await?.text().await?;
    assert_that!(body.as_str(), contains_substring("Refreshed title"));
    assert_that!(body.as_str(), contains_substring("Saved Author"));
    assert_that!(body.as_str(), contains_substring("/123-M.jpg"));
    assert_that!(
        body.as_str(),
        not(contains_substring("Registered but not ranked"))
    );
    assert_that!(store(&app).load_ranking_documents(ar).await?.len(), eq(1));
    let history = store(&app).load_application_history(ar).await?;
    assert_that!(
        history
            .iter()
            .filter(|e| e.envelope()["type"] == "booksmartz.add.rejected.v1")
            .count(),
        eq(2)
    );
    let reasons = history
        .iter()
        .filter(|event| event.envelope()["type"] == "booksmartz.add.rejected.v1")
        .map(|event| {
            event.envelope()["data"]["reason"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_that!(
        reasons,
        eq(&vec![
            "Your books changed. Review them before you try again.".to_owned(),
            "You already added this book.".to_owned()
        ])
    );
    Ok(())
}

pub(crate) async fn reject_ranking(app: &Fixture) -> anyhow::Result<()> {
    sqlx::raw_sql("CREATE FUNCTION reject_web_ranking() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture failure'; END $$; CREATE TRIGGER reject_web_ranking BEFORE INSERT ON book_smartz.ranking_events FOR EACH ROW EXECUTE FUNCTION reject_web_ranking();").execute(&app.pool).await?;
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn failed_save_blocks_new_actions_and_recovers_frozen_expired_intent_after_restart()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("checking-reader").await?;
    let id = reader(&app, "checking-reader").await?;
    let expiry = chrono::Utc::now().timestamp() + 4;
    let values = offer(&app, &cookie, id, work("OL4W", "Recovered title"), expiry).await?;
    reject_ranking(&app).await?;
    let response = post(&app, &cookie, "/add", &values).await?;
    assert_that!(
        response.text().await?.as_str(),
        contains_substring("We cannot confirm your save yet. Check it before you try again.")
    );
    let diagnostic = app.diagnostic("checking", "unconfirmed").await?;
    assert_that!(
        diagnostic["action"].as_str(),
        eq(Some(value(&values, "action")))
    );
    app.diagnostic("persistence", "unconfirmed").await?;
    let records = std::fs::read_to_string(app.diagnostics.path())?;
    for record in records.lines() {
        let fact: Value = serde_json::from_str(record)?;
        assert_that!(
            fact["category"] == "persistence" && fact["outcome"] == "confirmed",
            eq(false)
        );
    }
    let before = store(&app).load_application_history(id).await?;
    assert_that!(before.len(), eq(3));
    assert_that!(store(&app).load_ranking_documents(id).await?.len(), eq(0));
    let other = offer(
        &app,
        &cookie,
        id,
        work("OL5W", "Blocked title"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        post(&app, &cookie, "/add", &other)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("Check your save before you add another book.")
    );
    assert_that!(store(&app).load_application_history(id).await?, eq(&before));
    app.restart().await?;
    assert_that!(
        app.ranking(&cookie).await?.text().await?.as_str(),
        contains_substring("Check your save")
    );
    sqlx::query("DROP TRIGGER reject_web_ranking ON book_smartz.ranking_events")
        .execute(&app.pool)
        .await?;
    // Exercise real selection expiry; this is a deadline, not race coordination.
    let remaining = (expiry - chrono::Utc::now().timestamp() + 1).max(0);
    tokio::time::sleep(std::time::Duration::from_secs(u64::try_from(remaining)?)).await;
    assert_that!(
        post(&app, &cookie, "/select", &values)
            .await?
            .status()
            .as_u16(),
        eq(403)
    );
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    assert_that!(store(&app).load_application_history(id).await?, eq(&before));
    let actual = store(&app).load_ranking_documents(id).await?;
    let expected = &before[2].envelope()["data"]["document"];
    assert_that!(
        serde_json::from_str::<Value>(&book_smartz_domain::encode_document(&actual[0])?)?,
        eq(expected)
    );
    assert_that!(
        actual[0].event().metadata().id.as_uuid().to_string(),
        eq(before[0].envelope()["data"]["event"].as_str().unwrap())
    );
    assert_that!(
        app.ranking(&cookie).await?.text().await?.as_str(),
        contains_substring("Recovered title")
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn matching_event_identity_with_different_contents_stays_checking() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("conflicting-evidence-reader").await?;
    let id = reader(&app, "conflicting-evidence-reader").await?;
    let values = offer(
        &app,
        &cookie,
        id,
        work("OL6W", "Conflicting evidence"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    reject_ranking(&app).await?;
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("Check it before")
    );
    let history = store(&app).load_application_history(id).await?;
    let original = &history[0].envelope()["data"];
    let intended = &history[2].envelope()["data"]["document"];
    sqlx::query("DROP TRIGGER reject_web_ranking ON book_smartz.ranking_events")
        .execute(&app.pool)
        .await?;
    let candidate = BookId::new(Uuid::parse_str(
        intended["data"]["candidateBookId"].as_str().unwrap(),
    )?);
    store(&app)
        .execute(
            id,
            CommandContext {
                expected_revision: 0,
                event_id: EventId::new(Uuid::parse_str(original["event"].as_str().unwrap())?),
                time: chrono::DateTime::parse_from_rfc3339(original["time"].as_str().unwrap())?
                    .to_utc()
                    + chrono::Duration::seconds(1),
            },
            Command::Start { book_id: candidate },
            OperationContext::default(),
        )
        .await?;
    let recovery = vec![
        ("csrf".into(), value(&values, "csrf").into()),
        ("action".into(), value(&values, "action").into()),
    ];
    assert_that!(
        post(&app, &cookie, "/recover", &recovery)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("Check it before")
    );
    assert_that!(
        store(&app).load_application_history(id).await?,
        eq(&history)
    );
    assert_that!(
        app.ranking(&cookie).await?.text().await?.as_str(),
        contains_substring(
            "We cannot confirm your save yet. Check it before you change your books."
        )
    );
    Ok(())
}

#[path = "commit_proxy.rs"]
pub(crate) mod commit_proxy;

#[googletest::test]
#[tokio::test]
async fn real_lost_commit_acknowledgement_and_cancelled_process_recover_one_original_event()
-> anyhow::Result<()> {
    for cancel in [false, true] {
        let mut app = Fixture::new().await?;
        let cookie = app.login("lost-ack-reader").await?;
        let id = reader(&app, "lost-ack-reader").await?;
        let values = offer(
            &app,
            &cookie,
            id,
            work("OL7W", "Lost acknowledgement"),
            chrono::Utc::now().timestamp() + 900,
        )
        .await?;
        let direct = app.database.clone();
        let proxy = commit_proxy::CommitProxy::new(&app.pool, id).await?;
        app.database = proxy.database(&direct)?;
        app.restart().await?;
        let client = app.client.clone();
        let target = format!("{}/add", app.base);
        let cookie_copy = cookie.clone();
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(values.iter().map(|(k, v)| (k, v)))
            .finish();
        let request = tokio::spawn(async move {
            client
                .post(target)
                .header("cookie", cookie_copy)
                .header("origin", "https://reader.example")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .send()
                .await
        });
        proxy.commit_reached().await?;
        let before = store(&app).load_application_history(id).await?;
        let committed = store(&app).load_ranking_documents(id).await?;
        assert_that!(committed.len(), eq(1));
        if cancel {
            request.abort();
            app.process.0.kill()?;
            app.process.0.wait()?;
            proxy.disconnect();
        } else {
            proxy.disconnect();
            assert_that!(
                request.await??.text().await?.as_str(),
                contains_substring("Check it before")
            );
            assert_lost_acknowledgement_fact(
                &app,
                crate::diagnostic_events::OperationKind::Add,
                &values,
            )
            .await?;
        }
        app.database = direct;
        app.restart().await?;
        assert_that!(
            post(&app, &cookie, "/add", &values)
                .await?
                .status()
                .as_u16(),
            eq(303)
        );
        assert_that!(store(&app).load_application_history(id).await?, eq(&before));
        assert_that!(
            store(&app).load_ranking_documents(id).await?,
            eq(&committed)
        );
        assert_that!(
            app.ranking(&cookie).await?.text().await?.as_str(),
            contains_substring("Lost acknowledgement")
        );
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn in_flight_absent_ranking_stays_checking_until_original_action_is_recovered()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("interrupted-reader").await?;
    let id = reader(&app, "interrupted-reader").await?;
    let values = offer(
        &app,
        &cookie,
        id,
        work("OL8W", "Interrupted save"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    sqlx::raw_sql("CREATE FUNCTION hold_web_ranking() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_advisory_xact_lock(724,42); RETURN NEW; END $$; CREATE TRIGGER hold_web_ranking BEFORE INSERT ON book_smartz.ranking_events FOR EACH ROW EXECUTE FUNCTION hold_web_ranking();").execute(&app.pool).await?;
    let mut barrier = app.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(724,42)")
        .execute(&mut *barrier)
        .await?;
    let client = app.client.clone();
    let target = format!("{}/add", app.base);
    let cookie_copy = cookie.clone();
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(values.iter().map(|(k, v)| (k, v)))
        .finish();
    let request = tokio::spawn(async move {
        client
            .post(target)
            .header("cookie", cookie_copy)
            .header("origin", "https://reader.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10),async {
        loop {
            let waiting: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND classid=724 AND objid=42 AND NOT granted)").fetch_one(&app.pool).await?;
            if waiting {return Ok::<_,anyhow::Error>(())}tokio::task::yield_now().await;
        }
    }).await??;
    assert_that!(store(&app).load_ranking_documents(id).await?.len(), eq(0));
    // A concurrent read must visibly retain uncertainty without needing the write lock.
    let response = app
        .client
        .get(format!("{}/ranking", app.base))
        .header("cookie", &cookie)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await?;
    assert_that!(
        response.text().await?.as_str(),
        contains_substring(
            "We cannot confirm your save yet. Check it before you change your books."
        )
    );
    app.process.0.kill()?;
    app.process.0.wait()?;
    request.abort();
    barrier.rollback().await?;
    app.restart().await?;
    let before = store(&app).load_application_history(id).await?;
    let other = offer(
        &app,
        &cookie,
        id,
        work("OL9W", "Must stay blocked"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        post(&app, &cookie, "/add", &other)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("Check your save before")
    );
    assert_that!(store(&app).load_application_history(id).await?, eq(&before));
    let recovery = vec![
        ("csrf".into(), value(&values, "csrf").into()),
        ("action".into(), value(&values, "action").into()),
    ];
    assert_that!(
        post(&app, &cookie, "/recover", &recovery)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let actual = store(&app).load_ranking_documents(id).await?;
    assert_that!(
        serde_json::from_str::<Value>(&book_smartz_domain::encode_document(&actual[0])?)?,
        eq(&before[2].envelope()["data"]["document"])
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn five_concurrent_readers_save_with_the_production_connection_limit() -> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let mut inputs = Vec::new();
    for number in 10..15 {
        let subject = format!("pool-reader-{number}");
        let cookie = app.login(&subject).await?;
        let id = reader(&app, &subject).await?;
        let values = offer(
            &app,
            &cookie,
            id,
            work(&format!("OL{number}W"), "Concurrent save"),
            chrono::Utc::now().timestamp() + 900,
        )
        .await?;
        inputs.push((cookie, values));
    }
    let mut database = url::Url::parse(&app.database)?;
    database
        .query_pairs_mut()
        .append_pair("application_name", "book-smartz-pressure");
    app.database = database.to_string();
    app.restart().await?;
    sqlx::raw_sql("CREATE FUNCTION hold_prepared_readers() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF EXISTS(SELECT 1 FROM book_smartz.application_events WHERE reader_id=NEW.reader_id AND envelope->>'type'='booksmartz.add.intent.v1') THEN PERFORM pg_advisory_xact_lock_shared(724,50); END IF; RETURN NEW; END $$; CREATE TRIGGER hold_prepared_readers BEFORE INSERT ON book_smartz.reader_streams FOR EACH ROW EXECUTE FUNCTION hold_prepared_readers();").execute(&app.pool).await?;
    let mut barrier = app.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(724,50)")
        .execute(&mut *barrier)
        .await?;
    let mut requests = tokio::task::JoinSet::new();
    for (cookie, values) in inputs {
        let client = app.client.clone();
        let target = format!("{}/add", app.base);
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(values)
            .finish();
        requests.spawn(async move {
            client
                .post(target)
                .header("cookie", cookie)
                .header("origin", "https://reader.example")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .send()
                .await
        });
    }
    tokio::time::timeout(std::time::Duration::from_secs(4),async {
        loop {
            let active: i64=sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND classid=724 AND objid=50 AND NOT granted").fetch_one(&app.pool).await?;
            if active==5 {return Ok::<_,anyhow::Error>(())}tokio::task::yield_now().await;
        }
    }).await??;
    barrier.rollback().await?;
    while let Some(result) = requests.join_next().await {
        assert_that!(result??.status().as_u16(), eq(303));
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn later_add_offers_either_book_and_saves_one_preference() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("preference-reader").await?;
    let id = reader(&app, "preference-reader").await?;
    for (work_id, title) in [
        ("OL101W", "First saved book"),
        ("OL102W", "Pending candidate"),
    ] {
        let values = offer(
            &app,
            &cookie,
            id,
            work(work_id, title),
            chrono::Utc::now().timestamp() + 900,
        )
        .await?;
        assert_that!(
            post(&app, &cookie, "/add", &values)
                .await?
                .status()
                .as_u16(),
            eq(303)
        );
    }
    let body = app.ranking(&cookie).await?.text().await?;
    assert_that!(
        body.as_str(),
        contains_substring("Which book do you prefer?")
    );
    let values = [
        "csrf",
        "action",
        "candidate",
        "opponent",
        "revision",
        "offer",
    ]
    .into_iter()
    .map(|name| Ok((name.to_owned(), field(&body, name)?)))
    .collect::<anyhow::Result<Vec<_>>>()?;
    let mut values = values;
    values.push(("choice".into(), "candidate".into()));
    let (first, retry) = tokio::join!(
        post(&app, &cookie, "/answer", &values),
        post(&app, &cookie, "/answer", &values)
    );
    assert_that!(first?.status().as_u16(), eq(303));
    assert_that!(retry?.status().as_u16(), eq(303));
    let body = app.ranking(&cookie).await?.text().await?;
    let saved = body
        .split("<ol>")
        .nth(1)
        .unwrap()
        .split("</ol>")
        .next()
        .unwrap();
    assert_that!(
        saved.find("Pending candidate").unwrap() < saved.find("First saved book").unwrap(),
        eq(true)
    );
    assert_that!(
        body.as_str(),
        not(contains_substring("Which book do you prefer?"))
    );
    assert_that!(store(&app).load_ranking_documents(id).await?.len(), eq(3));
    Ok(())
}

async fn preference(
    app: &Fixture,
    cookie: &str,
    choice: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let html = app.ranking(cookie).await?.text().await?;
    let mut values = [
        "csrf",
        "action",
        "candidate",
        "opponent",
        "revision",
        "offer",
    ]
    .into_iter()
    .map(|name| Ok((name.to_owned(), field(&html, name)?)))
    .collect::<anyhow::Result<Vec<_>>>()?;
    values.push(("choice".into(), choice.into()));
    Ok(values)
}
async fn add_work(
    app: &Fixture,
    cookie: &str,
    id: ReaderId,
    work_id: &str,
    title: &str,
) -> anyhow::Result<()> {
    let values = offer(
        app,
        cookie,
        id,
        work(work_id, title),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        post(app, cookie, "/add", &values).await?.status().as_u16(),
        eq(303)
    );
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn top_middle_bottom_insertions_preserve_order_and_pending_add_gates() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    for (position, choices, expected) in [
        (
            "top",
            ["candidate", "candidate"],
            ["Candidate", "One", "Two", "Three"],
        ),
        (
            "middle",
            ["opponent", "candidate"],
            ["One", "Two", "Candidate", "Three"],
        ),
        (
            "bottom",
            ["opponent", "opponent"],
            ["One", "Two", "Three", "Candidate"],
        ),
    ] {
        let cookie = app.login(position).await?;
        let id = reader(&app, position).await?;
        add_work(&app, &cookie, id, "OL201W", "One").await?;
        add_work(&app, &cookie, id, "OL202W", "Two").await?;
        let answer = preference(&app, &cookie, "opponent").await?;
        assert_that!(
            post(&app, &cookie, "/answer", &answer)
                .await?
                .status()
                .as_u16(),
            eq(303)
        );
        add_work(&app, &cookie, id, "OL203W", "Three").await?;
        while store(&app)
            .load_ranking(id, OperationContext::default())
            .await?
            .projection()
            .pending()
            .is_some()
        {
            let answer = preference(&app, &cookie, "opponent").await?;
            assert_that!(
                post(&app, &cookie, "/answer", &answer)
                    .await?
                    .status()
                    .as_u16(),
                eq(303)
            );
        }
        add_work(&app, &cookie, id, "OL204W", "Candidate").await?;
        let before = app.ranking(&cookie).await?.text().await?;
        let saved = before
            .split("<ol>")
            .nth(1)
            .unwrap()
            .split("</ol>")
            .next()
            .unwrap();
        assert_that!(saved, not(contains_substring("Candidate")));
        let blocked = offer(
            &app,
            &cookie,
            id,
            work("OL205W", "Blocked candidate"),
            chrono::Utc::now().timestamp() + 900,
        )
        .await?;
        assert_that!(
            post(&app, &cookie, "/add", &blocked)
                .await?
                .text()
                .await?
                .as_str(),
            contains_substring(
                "Finish the comparisons for your current book before you add another book."
            )
        );
        for choice in choices {
            let values = preference(&app, &cookie, choice).await?;
            assert_that!(
                post(&app, &cookie, "/answer", &values)
                    .await?
                    .status()
                    .as_u16(),
                eq(303)
            );
        }
        let body = app.ranking(&cookie).await?.text().await?;
        let saved = body
            .split("<ol>")
            .nth(1)
            .unwrap()
            .split("</ol>")
            .next()
            .unwrap();
        let offsets = expected.map(|name| saved.find(&format!(">{name}</a>")).unwrap());
        assert_that!(offsets.windows(2).all(|pair| pair[0] < pair[1]), eq(true));
        assert_that!(
            body.as_str(),
            not(contains_substring("Which book do you prefer?"))
        );
    }
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn preference_security_and_stale_offers_require_fresh_confirmation() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("stale-preference").await?;
    let id = reader(&app, "stale-preference").await?;
    for (work_id, title) in [("OL301W", "One"), ("OL302W", "Two")] {
        add_work(&app, &cookie, id, work_id, title).await?;
        if title == "Two" {
            let values = preference(&app, &cookie, "opponent").await?;
            assert_that!(
                post(&app, &cookie, "/answer", &values)
                    .await?
                    .status()
                    .as_u16(),
                eq(303)
            );
        }
    }
    add_work(&app, &cookie, id, "OL303W", "Candidate").await?;
    let original = preference(&app, &cookie, "opponent").await?;
    let stale = preference(&app, &cookie, "candidate").await?;
    let before = store(&app).load_application_history(id).await?;
    assert_that!(
        post(&app, "", "/answer", &original)
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    for key in [
        "csrf",
        "offer",
        "action",
        "candidate",
        "opponent",
        "revision",
        "choice",
    ] {
        let mut invalid = original.clone();
        invalid.iter_mut().find(|(k, _)| k == key).unwrap().1 = match key {
            "action" | "candidate" | "opponent" => Uuid::new_v4().to_string(),
            "revision" => "999".into(),
            "choice" => "skip".into(),
            _ => "wrong".into(),
        };
        assert_that!(
            post(&app, &cookie, "/answer", &invalid)
                .await?
                .status()
                .is_client_error(),
            eq(true)
        );
    }
    let other = app.login("isolated-preference").await?;
    let mut wrong_reader = original.clone();
    wrong_reader[0].1 = field(&app.ranking(&other).await?.text().await?, "csrf")?;
    assert_that!(
        post(&app, &other, "/answer", &wrong_reader)
            .await?
            .status()
            .as_u16(),
        eq(403)
    );
    assert_that!(store(&app).load_application_history(id).await?, eq(&before));
    assert_that!(
        post(&app, &cookie, "/answer", &original)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let after = store(&app).load_ranking_documents(id).await?;
    assert_that!(
        post(&app, &cookie, "/answer", &stale)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("comparison changed")
    );
    assert_that!(store(&app).load_ranking_documents(id).await?, eq(&after));
    assert_that!(
        post(&app, &cookie, "/answer", &original)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let mut changed = original.clone();
    changed.iter_mut().find(|(k, _)| k == "choice").unwrap().1 = "candidate".into();
    assert_that!(
        post(&app, &cookie, "/answer", &changed)
            .await?
            .status()
            .as_u16(),
        eq(403)
    );
    // Expiry rejects an old browser; login restores the next comparison without submitting it.
    sqlx::query("UPDATE book_smartz_web.sessions SET expires_at=now()-interval '1 second' WHERE reader_id=$1").bind(id.as_uuid()).execute(&app.pool).await?;
    assert_that!(
        post(&app, &cookie, "/answer", &original)
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    let reauthenticated = app.login("stale-preference").await?;
    assert_that!(
        app.ranking(&reauthenticated).await?.text().await?.as_str(),
        contains_substring("Which book do you prefer?")
    );
    assert_that!(store(&app).load_ranking_documents(id).await?, eq(&after));
    let fresh = preference(&app, &reauthenticated, "candidate").await?;
    assert_that!(
        post(&app, &reauthenticated, "/answer", &fresh)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn uncertain_preference_blocks_answers_and_restores_original_after_restart()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("uncertain-preference").await?;
    let id = reader(&app, "uncertain-preference").await?;
    add_work(&app, &cookie, id, "OL401W", "One").await?;
    add_work(&app, &cookie, id, "OL402W", "Candidate").await?;
    let values = preference(&app, &cookie, "candidate").await?;
    let other = preference(&app, &cookie, "opponent").await?;
    reject_ranking(&app).await?;
    assert_that!(
        post(&app, &cookie, "/answer", &values)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("We cannot confirm your save yet. Check it before you try again.")
    );
    let history = store(&app).load_application_history(id).await?;
    assert_that!(
        post(&app, &cookie, "/answer", &other)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("Check your save before you choose another book.")
    );
    assert_that!(
        store(&app).load_application_history(id).await?,
        eq(&history)
    );
    app.provider.reset().await;
    app.restart().await?;
    let body = app.ranking(&cookie).await?.text().await?;
    assert_that!(body.as_str(), contains_substring("Check your save"));
    assert_that!(body.as_str(), contains_substring("disabled"));
    sqlx::query("DROP TRIGGER reject_web_ranking ON book_smartz.ranking_events")
        .execute(&app.pool)
        .await?;
    assert_that!(
        post(&app, &cookie, "/recover", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    assert_that!(
        store(&app).load_application_history(id).await?,
        eq(&history)
    );
    let docs = store(&app).load_ranking_documents(id).await?;
    assert_that!(docs.len(), eq(3));
    assert_that!(
        serde_json::from_str::<Value>(&book_smartz_domain::encode_document(&docs[2])?)?,
        eq(&history.last().unwrap().envelope()["data"]["document"])
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn real_ranking_commit_ack_loss_and_cancellation_preserve_frozen_commands()
-> anyhow::Result<()> {
    for (route, choice, cancel) in [
        ("/answer", "opponent", false),
        ("/answer", "opponent", true),
        ("/pause", "pause", false),
        ("/pause", "pause", true),
        ("/resume", "resume", false),
        ("/resume", "resume", true),
    ] {
        let mut app = Fixture::new().await?;
        let cookie = app.login("preference-ack").await?;
        let id = reader(&app, "preference-ack").await?;
        add_work(&app, &cookie, id, "OL601W", "One").await?;
        add_work(&app, &cookie, id, "OL602W", "Candidate").await?;
        if route == "/resume" {
            let pause = placement_form(&app.ranking(&cookie).await?.text().await?, "/pause")?;
            assert_that!(
                post(&app, &cookie, "/pause", &pause)
                    .await?
                    .status()
                    .as_u16(),
                eq(303)
            );
        }
        let values = if route == "/answer" {
            preference(&app, &cookie, choice).await?
        } else {
            placement_form(&app.ranking(&cookie).await?.text().await?, route)?
        };
        let direct = app.database.clone();
        let proxy = commit_proxy::CommitProxy::new(&app.pool, id).await?;
        app.database = proxy.database(&direct)?;
        app.restart().await?;
        let client = app.client.clone();
        let target = format!("{}{route}", app.base);
        let auth = cookie.clone();
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(values.iter().map(|(k, v)| (k, v)))
            .finish();
        let request = tokio::spawn(async move {
            client
                .post(target)
                .header("cookie", auth)
                .header("origin", "https://reader.example")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .send()
                .await
        });
        proxy.commit_reached().await?;
        let history = store(&app).load_application_history(id).await?;
        let committed = store(&app).load_ranking_documents(id).await?;
        assert_that!(committed.len(), eq(if route == "/resume" { 4 } else { 3 }));
        if cancel {
            request.abort();
            app.process.0.kill()?;
            app.process.0.wait()?;
            proxy.disconnect();
        } else {
            proxy.disconnect();
            assert_that!(
                request.await??.text().await?.as_str(),
                contains_substring("Check it before")
            );
            assert_lost_acknowledgement_fact(
                &app,
                match route {
                    "/answer" => crate::diagnostic_events::OperationKind::Preference,
                    "/pause" => crate::diagnostic_events::OperationKind::Pause,
                    _ => crate::diagnostic_events::OperationKind::Resume,
                },
                &values,
            )
            .await?;
        }
        app.database = direct;
        app.restart().await?;
        assert_that!(
            post(&app, &cookie, route, &values).await?.status().as_u16(),
            eq(303)
        );
        assert_that!(
            store(&app).load_application_history(id).await?,
            eq(&history)
        );
        assert_that!(
            store(&app).load_ranking_documents(id).await?,
            eq(&committed)
        );
    }
    Ok(())
}

fn placement_form(html: &str, route: &str) -> anyhow::Result<Vec<(String, String)>> {
    let form = html
        .split(&format!("action=\"{route}\""))
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("missing {route} form"))?
        .split("</form>")
        .next()
        .unwrap();
    [
        "csrf",
        "action",
        "candidate",
        "opponent",
        "choice",
        "revision",
        "offer",
    ]
    .into_iter()
    .map(|name| Ok((name.to_owned(), field(form, name)?)))
    .collect()
}
#[googletest::test]
#[tokio::test]
async fn pause_resume_retains_the_unanswered_pair_after_preference_and_restart()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("pause-reader").await?;
    let id = reader(&app, "pause-reader").await?;
    for (work_id, title) in [
        ("OL701W", "One"),
        ("OL702W", "Two"),
        ("OL703W", "Three"),
        ("OL704W", "Four"),
        ("OL705W", "Five"),
        ("OL706W", "Six"),
        ("OL707W", "Seven"),
    ] {
        add_work(&app, &cookie, id, work_id, title).await?;
        while app
            .ranking(&cookie)
            .await?
            .text()
            .await?
            .contains("Which book do you prefer?")
        {
            let answer = preference(&app, &cookie, "opponent").await?;
            assert_that!(
                post(&app, &cookie, "/answer", &answer)
                    .await?
                    .status()
                    .as_u16(),
                eq(303)
            );
        }
    }
    add_work(&app, &cookie, id, "OL708W", "Candidate").await?;
    assert_that!(
        app.ranking(&cookie).await?.text().await?.as_str(),
        not(contains_substring(
            "You saved no choices for this book yet."
        ))
    );
    let answer = preference(&app, &cookie, "opponent").await?;
    assert_that!(
        post(&app, &cookie, "/answer", &answer)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let before = preference(&app, &cookie, "candidate").await?;
    let html = app.ranking(&cookie).await?.text().await?;
    let pause = placement_form(&html, "/pause")?;
    for _ in 0..2 {
        assert_that!(
            post(&app, &cookie, "/pause", &pause)
                .await?
                .status()
                .as_u16(),
            eq(303)
        );
    }
    let html = app.ranking(&cookie).await?.text().await?;
    assert_that!(
        html.as_str(),
        contains_substring("You paused the comparisons for this book. Resume when you are ready.")
    );
    assert_that!(
        html.as_str(),
        not(contains_substring("Which book do you prefer?"))
    );
    assert_that!(
        html.as_str(),
        not(contains_substring("You saved 1 choice for this book."))
    );
    app.restart().await?;
    let other = app.login("pause-reader").await?;
    let resume = placement_form(&app.ranking(&other).await?.text().await?, "/resume")?;
    for _ in 0..2 {
        assert_that!(
            post(&app, &other, "/resume", &resume)
                .await?
                .status()
                .as_u16(),
            eq(303)
        );
    }
    let restored = preference(&app, &other, "candidate").await?;
    assert_that!(
        value(&restored, "candidate"),
        eq(value(&before, "candidate"))
    );
    assert_that!(value(&restored, "opponent"), eq(value(&before, "opponent")));
    let documents = store(&app).load_ranking_documents(id).await?;
    let response = post(&app, &cookie, "/answer", &before)
        .await?
        .text()
        .await?;
    assert_that!(response.as_str(), contains_substring("comparison changed"));
    assert_that!(
        store(&app).load_ranking_documents(id).await?,
        eq(&documents)
    );
    assert_that!(
        app.ranking(&other).await?.text().await?.as_str(),
        not(contains_substring("You saved 1 choice for this book."))
    );
    let second = preference(&app, &other, "opponent").await?;
    assert_that!(
        post(&app, &other, "/answer", &second)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let html = app.ranking(&other).await?.text().await?;
    assert_that!(
        html.as_str(),
        not(contains_substring("You saved 2 choices for this book."))
    );
    assert_that!(
        html.as_str(),
        contains_substring("Which book do you prefer?")
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn placement_actions_authorize_each_post_and_reconcile_before_expired_session_resume()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("placement-security").await?;
    let id = reader(&app, "placement-security").await?;
    add_work(&app, &cookie, id, "OL801W", "One").await?;
    add_work(&app, &cookie, id, "OL802W", "Pending").await?;
    let other = app.login("other-placement").await?;
    for route in ["/pause", "/resume"] {
        let values = placement_form(&app.ranking(&cookie).await?.text().await?, route)?;
        let before = store(&app).load_ranking_documents(id).await?;
        assert_that!(
            app.client
                .get(format!("{}{route}", app.base))
                .send()
                .await?
                .status()
                .as_u16(),
            eq(405)
        );
        assert_that!(
            post(&app, "", route, &values).await?.status().as_u16(),
            eq(401)
        );
        let mut invalid = values.clone();
        invalid[0].1 = "wrong".into();
        assert_that!(
            post(&app, &cookie, route, &invalid)
                .await?
                .status()
                .as_u16(),
            eq(403)
        );
        invalid[0].1 = field(&app.ranking(&other).await?.text().await?, "csrf")?;
        assert_that!(
            post(&app, &other, route, &invalid).await?.status().as_u16(),
            eq(403)
        );
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(values.iter().map(|(k, v)| (k, v)))
            .finish();
        assert_that!(
            app.client
                .post(format!("{}{route}", app.base))
                .header("cookie", &cookie)
                .header("origin", "https://attacker.example")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .send()
                .await?
                .status()
                .as_u16(),
            eq(403)
        );
        let mut skip = values.clone();
        skip.iter_mut().find(|(k, _)| k == "choice").unwrap().1 = "skip".into();
        assert_that!(
            post(&app, &cookie, route, &skip)
                .await?
                .status()
                .is_client_error(),
            eq(true)
        );
        assert_that!(store(&app).load_ranking_documents(id).await?, eq(&before));
        reject_ranking(&app).await?;
        assert_that!(
            post(&app, &cookie, route, &values)
                .await?
                .text()
                .await?
                .as_str(),
            contains_substring("Check it before")
        );
        let history = store(&app).load_application_history(id).await?;
        let html = app.ranking(&cookie).await?.text().await?;
        assert_that!(html.as_str(), contains_substring("disabled"));
        let blocked = placement_form(&html, route)?;
        assert_that!(
            post(&app, &cookie, route, &blocked)
                .await?
                .text()
                .await?
                .as_str(),
            contains_substring("Check your save before")
        );
        assert_that!(
            store(&app).load_application_history(id).await?,
            eq(&history)
        );
        sqlx::raw_sql("DROP TRIGGER reject_web_ranking ON book_smartz.ranking_events; DROP FUNCTION reject_web_ranking()")
            .execute(&app.pool).await?;
        app.restart().await?;
        assert_that!(
            post(&app, &cookie, "/recover", &values)
                .await?
                .status()
                .as_u16(),
            eq(303)
        );
        assert_that!(
            store(&app).load_application_history(id).await?,
            eq(&history)
        );
    }
    let pause = placement_form(&app.ranking(&cookie).await?.text().await?, "/pause")?;
    assert_that!(
        post(&app, &cookie, "/pause", &pause)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let resume = placement_form(&app.ranking(&cookie).await?.text().await?, "/resume")?;
    let before = store(&app).load_ranking_documents(id).await?;
    sqlx::query("UPDATE book_smartz_web.sessions SET expires_at=now()-interval '1 second' WHERE reader_id=$1").bind(id.as_uuid()).execute(&app.pool).await?;
    assert_that!(
        post(&app, &cookie, "/resume", &resume)
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    let fresh = app.login("placement-security").await?;
    assert_that!(
        app.ranking(&fresh).await?.text().await?.as_str(),
        contains_substring("You paused the comparisons for this book. Resume when you are ready.")
    );
    assert_that!(store(&app).load_ranking_documents(id).await?, eq(&before));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn private_diagnostics_confirm_new_commits_without_replaying_activity() -> anyhow::Result<()>
{
    let app = Fixture::new().await?;
    let rejected = app.begin().await?;
    let mut claims = Fixture::claims(&rejected, "never-log-this-subject");
    claims["aud"] = json!("wrong-client");
    assert_that!(
        app.finish(&rejected, claims).await?.status().as_u16(),
        eq(401)
    );
    let cookie = app.login("never-log-this-subject").await?;
    let reader = reader(&app, "never-log-this-subject").await?;
    let values = offer(
        &app,
        &cookie,
        reader,
        work("OL771W", "Never log this title"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    // Business records retain their replay count. Auth and read attempts are separate facts.
    let read = || -> anyhow::Result<Vec<Value>> {
        Ok(std::fs::read_to_string(app.diagnostics.path())?
            .lines()
            .map(serde_json::from_str::<Value>)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|record| record.get("category").is_some())
            .collect())
    };
    app.diagnostic("persistence", "confirmed").await?;
    let facts = read()?;
    let typed: Vec<crate::diagnostic_events::OperationRecord> =
        std::fs::read_to_string(app.diagnostics.path())?
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
    for outcome in [
        crate::diagnostic_events::OperationOutcome::Confirmed,
        crate::diagnostic_events::OperationOutcome::Rejected,
    ] {
        assert_that!(
            typed.iter().any(|record| record.fact.operation
                == crate::diagnostic_events::OperationKind::SignIn
                && record.fact.outcome == outcome),
            eq(true)
        );
    }
    let text = std::fs::read_to_string(app.diagnostics.path())?;
    for forbidden in [
        "never-log-this-subject",
        "Never log this title",
        "OL771W",
        &reader.as_uuid().to_string(),
        &cookie,
        "before@example.test",
        "Saved Author",
    ] {
        assert_that!(text.as_str(), not(contains_substring(forbidden)));
    }
    for fact in &facts {
        assert_that!(fact["release"].as_str().unwrap().len(), eq(64));
        assert_that!(fact["duration_ms"].is_number(), eq(true));
        let fields = fact.as_object().unwrap();
        assert_that!(
            fields.keys().all(
                |k| ["category", "outcome", "duration_ms", "release", "action"]
                    .contains(&k.as_str())
            ),
            eq(true)
        );
    }
    let before = read()?.len();
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    assert_that!(
        post(&app, &cookie, "/recover", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let checked = operation_record(
        &app,
        crate::diagnostic_events::OperationKind::OriginalSaveCheck,
        0,
    )
    .await?;
    assert_that!(
        checked.fact.reason,
        eq(crate::diagnostic_events::Reason::ExistingSaved)
    );
    assert_that!(read()?.len(), eq(before));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn broken_diagnostic_delivery_preserves_confirmed_outcomes_and_durable_history()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    app.break_diagnostic_delivery().await?;
    let cookie = app.login("broken-diagnostics-reader").await?;
    let reader = reader(&app, "broken-diagnostics-reader").await?;
    let values = offer(
        &app,
        &cookie,
        reader,
        work("OL772W", "Saved despite broken diagnostics"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    assert_that!(
        post(&app, &cookie, "/recover", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let ranking = store(&app)
        .load_ranking(reader, OperationContext::default())
        .await?;
    assert_that!(ranking.projection().revision(), eq(1));
    app.restart().await?;
    assert_that!(
        app.ranking(&cookie).await?.text().await?.as_str(),
        contains_substring("Saved despite broken diagnostics")
    );
    assert_that!(
        post(&app, &cookie, "/add", &values)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    assert_that!(
        store(&app)
            .load_ranking(reader, OperationContext::default())
            .await?
            .projection()
            .revision(),
        eq(1)
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn initial_intent_ack_loss_reports_checking_before_original_action_recovery()
-> anyhow::Result<()> {
    for route in ["/add", "/answer"] {
        let mut app = Fixture::new().await?;
        let cookie = app.login("initial-intent-reader").await?;
        let id = reader(&app, "initial-intent-reader").await?;
        let values = if route == "/add" {
            offer(
                &app,
                &cookie,
                id,
                work("OL881W", "Private initial intent"),
                chrono::Utc::now().timestamp() + 900,
            )
            .await?
        } else {
            add_work(&app, &cookie, id, "OL882W", "First book").await?;
            add_work(&app, &cookie, id, "OL883W", "Candidate").await?;
            preference(&app, &cookie, "candidate").await?
        };
        let prior = store(&app).load_ranking_documents(id).await?.len();
        let direct = app.database.clone();
        let proxy = commit_proxy::CommitProxy::initial_intent(&app.pool, id).await?;
        app.database = proxy.database(&direct)?;
        app.restart().await?;
        let client = app.client.clone();
        let cookie_copy = cookie.clone();
        let target = format!("{}{route}", app.base);
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(values.iter().map(|(k, v)| (k, v)))
            .finish();
        let request = tokio::spawn(async move {
            client
                .post(target)
                .header("cookie", cookie_copy)
                .header("origin", "https://reader.example")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .send()
                .await
        });
        proxy.commit_reached().await?;
        let frozen = store(&app).load_application_history(id).await?;
        assert_that!(
            frozen.last().unwrap().envelope()["type"]
                .as_str()
                .unwrap()
                .ends_with(".intent.v1"),
            eq(true)
        );
        assert_that!(
            store(&app).load_ranking_documents(id).await?.len(),
            eq(prior)
        );
        proxy.disconnect();
        assert_that!(
            request.await??.text().await?.as_str(),
            contains_substring("Check it before")
        );
        assert_lost_acknowledgement_fact(
            &app,
            if route == "/add" {
                crate::diagnostic_events::OperationKind::Add
            } else {
                crate::diagnostic_events::OperationKind::Preference
            },
            &values,
        )
        .await?;
        let diagnostic = app.diagnostic("checking", "unconfirmed").await?;
        assert_that!(
            diagnostic["action"].as_str(),
            eq(Some(value(&values, "action")))
        );
        app.diagnostic("persistence", "unconfirmed").await?;
        app.database = direct;
        app.restart().await?;
        assert_that!(
            post(&app, &cookie, route, &values).await?.status().as_u16(),
            eq(303)
        );
        assert_that!(
            store(&app).load_ranking_documents(id).await?.len(),
            eq(prior + 1)
        );
        assert_that!(
            store(&app)
                .load_application_history(id)
                .await?
                .starts_with(&frozen),
            eq(true)
        );
    }
    Ok(())
}

// Controlled app-owned payloads, persisted through the concrete composition API.
// The store validates envelopes; the packaged application interprets their data.
async fn evidence_fact(
    app: &Fixture,
    reader: ReaderId,
    action: Uuid,
    kind: &str,
    data: Value,
) -> anyhow::Result<()> {
    let mut tx = store(app).begin_command(reader).await?;
    let sequence = tx.application_history().len() + 1;
    let event = book_smartz_storage::ApplicationEvent::decode(
        reader,
        &json!({
            "specversion":"1.0", "id":Uuid::new_v4(),
            "source":format!("urn:book-smartz:application:{}", reader.as_uuid()),
            "type":format!("booksmartz.{kind}.v1"), "time":"2026-01-01T12:00:00Z",
            "datacontenttype":"application/json", "sequence":format!("{sequence:020}"),
            "correlationid":action, "data":data,
        })
        .to_string(),
    )?;
    tx.append_application(&event).await?;
    tx.commit().await?;
    Ok(())
}
fn evidence_intent(action: Uuid) -> Value {
    json!({"action":action, "selection":"retained-selection", "work":null,
        "revision":0, "event":Uuid::new_v4(), "time":"2026-01-01T12:00:00Z"})
}
async fn recover_evidence(
    app: &Fixture,
    cookie: &str,
    csrf: &str,
    action: Uuid,
) -> anyhow::Result<reqwest::Response> {
    post(
        app,
        cookie,
        "/recover",
        &[
            ("csrf".into(), csrf.into()),
            ("action".into(), action.to_string()),
        ],
    )
    .await
}
fn recovery_actions(html: &str) -> anyhow::Result<Vec<String>> {
    regex::Regex::new(r#"(?s)<form method="post" action="/recover"[^>]*>(.*?)</form>"#)?
        .captures_iter(html)
        .map(|capture| field(&capture[1], "action"))
        .collect()
}

#[googletest::test]
#[tokio::test]
async fn evidence_controls_preserve_intent_order_duplicates_and_payload_identity()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("evidence-order").await?;
    let id = reader(&app, "evidence-order").await?;
    let csrf = field(&app.ranking(&cookie).await?.text().await?, "csrf")?;
    let first = Uuid::from_u128(101);
    let second = Uuid::from_u128(102);
    let correlation = Uuid::from_u128(103);
    for (action, family, payload) in [
        (correlation, "add.intent", evidence_intent(second)),
        (first, "preference.intent", evidence_intent(first)),
        (first, "add.intent", evidence_intent(first)),
        (first, "add.unknown", json!({"malformed":"ignored"})),
    ] {
        evidence_fact(&app, id, action, family, payload).await?;
    }
    let before = store(&app).load_application_history(id).await?;
    let html = app.ranking(&cookie).await?.text().await?;
    assert_that!(
        recovery_actions(&html)?,
        eq(&vec![
            second.to_string(),
            first.to_string(),
            first.to_string()
        ])
    );
    assert_that!(
        html.as_str(),
        contains_substring(
            "We cannot confirm your save yet. Check it before you change your books."
        )
    );
    // Enumeration uses the payload identity; recovery requires a correlated intent.
    assert_that!(
        recover_evidence(&app, &cookie, &csrf, second)
            .await?
            .status()
            .as_u16(),
        eq(403)
    );
    let other = offer(
        &app,
        &cookie,
        id,
        work("OL901W", "Blocked evidence"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        post(&app, &cookie, "/add", &other)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("Check your save before")
    );
    assert_that!(store(&app).load_application_history(id).await?, eq(&before));
    assert_that!(store(&app).load_ranking_documents(id).await?.len(), eq(0));
    Ok(())
}

async fn evidence_saved_document(app: &Fixture, id: ReaderId) -> anyhow::Result<Value> {
    let book = book_smartz_domain::Book::new(
        BookId::new(Uuid::from_u128(900)),
        book_smartz_domain::OpenLibraryWorkId::try_from("OL902W")?,
    );
    store(app)
        .register_book(&book, OperationContext::default())
        .await?;
    store(app)
        .execute(
            id,
            CommandContext {
                expected_revision: 0,
                event_id: EventId::new(Uuid::new_v4()),
                time: chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")?.to_utc(),
            },
            Command::Start { book_id: book.id() },
            OperationContext::default(),
        )
        .await?;
    let documents = store(app).load_ranking_documents(id).await?;
    Ok(serde_json::from_str(&book_smartz_domain::encode_document(
        &documents[0],
    )?)?)
}

#[googletest::test]
#[tokio::test]
async fn evidence_recovery_preserves_validation_scope_first_facts_and_terminal_precedence()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    // Each case has an independent reader, a real saved ranking envelope, and
    // controlled app facts. Expected HTTP outcomes are specified independently.
    for name in [
        "saved_before_rejection",
        "rejected_before_prepared",
        "later_preparation",
        "unrelated_intent",
        "first_intent",
        "later_intent",
        "first_rejection",
        "later_rejection",
        "missing_document_id",
        "invalid_document_id",
        "unrelated_preparation",
        "unrelated_rejection",
        "saved_before_bad_rejection",
        "missing_correlated_intent",
        "different_payload_identity",
    ] {
        let cookie = app.login(name).await?;
        let id = reader(&app, name).await?;
        let csrf = field(&app.ranking(&cookie).await?.text().await?, "csrf")?;
        let document = evidence_saved_document(&app, id).await?;
        let action = Uuid::from_u128(201);
        let other = Uuid::from_u128(202);
        let intent = evidence_intent(action);
        let prepared = json!({"document":document});
        let missing = json!({"document":{}});
        let invalid = json!({"document":{"id":"invalid-uuid"}});
        let rejected = json!({"reason":"Your books changed. Reload and confirm a fresh action."});
        let malformed = json!({"reason":false});
        let mut facts = vec![(action, "add.intent", intent.clone())];
        let (ranking_status, recovery_status, message, controls) = match name {
            "saved_before_rejection" => {
                facts.extend([
                    (action, "preference.rankingprepared", prepared.clone()),
                    (action, "add.rejected", rejected.clone()),
                ]);
                (200, 303, None, 0)
            }
            "rejected_before_prepared" => {
                facts.extend([
                    (action, "add.rankingprepared", invalid.clone()),
                    (action, "preference.rejected", rejected.clone()),
                ]);
                (
                    200,
                    200,
                    Some("Your books changed. Reload and confirm a fresh action."),
                    0,
                )
            }
            "later_preparation" => {
                facts.extend([
                    (action, "preference.rankingprepared", prepared.clone()),
                    (action, "add.rankingprepared", invalid.clone()),
                ]);
                (200, 303, None, 0)
            }
            "unrelated_intent" => {
                facts.extend([
                    (action, "add.rankingprepared", prepared.clone()),
                    (other, "preference.intent", json!({})),
                ]);
                (500, 303, None, 0)
            }
            "first_intent" => {
                facts[0].2 = json!({});
                facts.extend([
                    (action, "preference.intent", intent.clone()),
                    (action, "add.rankingprepared", prepared.clone()),
                ]);
                (
                    500,
                    200,
                    Some("We cannot confirm your save yet. Check it before you try again."),
                    0,
                )
            }
            "later_intent" => {
                facts.extend([
                    (action, "preference.intent", json!({})),
                    (action, "add.rankingprepared", prepared.clone()),
                ]);
                (500, 303, None, 0)
            }
            "first_rejection" => {
                facts.extend([
                    (action, "add.rankingprepared", missing.clone()),
                    (action, "add.rejected", malformed.clone()),
                    (action, "preference.rejected", rejected.clone()),
                ]);
                (
                    500,
                    200,
                    Some("We cannot confirm your save yet. Check it before you try again."),
                    0,
                )
            }
            "later_rejection" => {
                facts.extend([
                    (action, "add.rankingprepared", missing.clone()),
                    (action, "preference.rejected", rejected.clone()),
                    (action, "add.rejected", malformed.clone()),
                ]);
                (
                    500,
                    200,
                    Some("Your books changed. Reload and confirm a fresh action."),
                    0,
                )
            }
            "missing_document_id" => {
                facts.push((action, "add.rankingprepared", missing.clone()));
                (
                    500,
                    200,
                    Some("We cannot confirm your save yet. Check it before you try again."),
                    0,
                )
            }
            "invalid_document_id" => {
                facts.push((action, "preference.rankingprepared", invalid.clone()));
                (
                    500,
                    200,
                    Some("We cannot confirm your save yet. Check it before you try again."),
                    0,
                )
            }
            "unrelated_preparation" => {
                facts.extend([
                    (action, "add.rankingprepared", prepared.clone()),
                    (other, "preference.rankingprepared", invalid.clone()),
                ]);
                (200, 303, None, 0)
            }
            "unrelated_rejection" => {
                facts.extend([
                    (action, "add.rankingprepared", prepared.clone()),
                    (other, "preference.rejected", malformed.clone()),
                ]);
                (500, 303, None, 0)
            }
            "saved_before_bad_rejection" => {
                facts.extend([
                    (action, "add.rankingprepared", prepared.clone()),
                    (action, "preference.rejected", malformed.clone()),
                ]);
                (500, 303, None, 0)
            }
            "missing_correlated_intent" => {
                facts[0].0 = other;
                facts.push((action, "preference.rankingprepared", prepared.clone()));
                (200, 403, None, 0)
            }
            "different_payload_identity" => {
                facts[0].2 = evidence_intent(other);
                facts.push((action, "preference.rankingprepared", prepared.clone()));
                (200, 303, None, 1)
            }
            _ => unreachable!(),
        };
        for (correlation, kind, data) in facts {
            evidence_fact(&app, id, correlation, kind, data).await?;
        }
        let history = store(&app).load_application_history(id).await?;
        let ranking = store(&app).load_ranking_documents(id).await?;
        let response = app.ranking(&cookie).await?;
        assert_that!(
            response.status().as_u16(),
            eq(ranking_status),
            "{name}: ranking response"
        );
        if ranking_status == 200 {
            let html = response.text().await?;
            assert_that!(
                recovery_actions(&html)?.len(),
                eq(controls),
                "{name}: recovery controls"
            );
            if controls == 1 {
                assert_that!(recovery_actions(&html)?, eq(&vec![other.to_string()]));
            }
        }
        let response = recover_evidence(&app, &cookie, &csrf, action).await?;
        assert_that!(
            response.status().as_u16(),
            eq(recovery_status),
            "{name}: original recovery"
        );
        if let Some(message) = message {
            assert_that!(
                response.text().await?.as_str(),
                contains_substring(message),
                "{name}"
            );
        }
        assert_that!(
            store(&app).load_application_history(id).await?,
            eq(&history),
            "{name}: no new app facts"
        );
        assert_that!(
            store(&app).load_ranking_documents(id).await?,
            eq(&ranking),
            "{name}: no new ranking effect"
        );
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn evidence_first_preparation_requires_the_complete_envelope_including_optional_context()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    for (name, attribute, matching_first) in [
        ("subject_conflict", "subject", false),
        ("extension_conflict", "correlationid", false),
        ("later_subject_conflict", "subject", true),
        ("later_extension_conflict", "correlationid", true),
    ] {
        let cookie = app.login(name).await?;
        let id = reader(&app, name).await?;
        let csrf = field(&app.ranking(&cookie).await?.text().await?, "csrf")?;
        let document = evidence_saved_document(&app, id).await?;
        let mut conflict = document.clone();
        conflict[attribute] = json!(Uuid::from_u128(303).to_string());
        let action = Uuid::from_u128(301);
        evidence_fact(
            &app,
            id,
            action,
            "preference.intent",
            evidence_intent(action),
        )
        .await?;
        let documents = if matching_first {
            [document, conflict]
        } else {
            [conflict, document]
        };
        for (kind, document) in ["add.rankingprepared", "preference.rankingprepared"]
            .into_iter()
            .zip(documents)
        {
            evidence_fact(&app, id, action, kind, json!({"document":document})).await?;
        }
        let history = store(&app).load_application_history(id).await?;
        let ranking = store(&app).load_ranking_documents(id).await?;
        let html = app.ranking(&cookie).await?.text().await?;
        let expected_controls = if matching_first {
            vec![]
        } else {
            vec![action.to_string()]
        };
        assert_that!(recovery_actions(&html)?, eq(&expected_controls), "{name}");
        let response = recover_evidence(&app, &cookie, &csrf, action).await?;
        if matching_first {
            assert_that!(response.status().as_u16(), eq(303));
        } else {
            assert_that!(response.status().as_u16(), eq(200));
            assert_that!(
                response.text().await?.as_str(),
                contains_substring(
                    "We cannot confirm your save yet. Check it before you try again."
                )
            );
        }
        assert_that!(
            store(&app).load_application_history(id).await?,
            eq(&history)
        );
        assert_that!(store(&app).load_ranking_documents(id).await?, eq(&ranking));
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn display_feedback_is_reader_bound_and_distinguishes_fresh_saves_from_replay()
-> anyhow::Result<()> {
    use base64::Engine;
    let app = Fixture::new().await?;
    let owner = app.login("display-owner").await?;
    let other = app.login("display-other").await?;
    let id = reader(&app, "display-owner").await?;
    let values = offer(
        &app,
        &owner,
        id,
        work("OL899W", "Confirmed display book"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    let response = post(&app, &owner, "/add", &values).await?;
    assert_that!(response.status().as_u16(), eq(303));

    let raw = display_cookie(&response)?;
    for attribute in ["Secure", "HttpOnly", "SameSite=Lax", "Path=/", "Max-Age=60"] {
        assert_that!(raw.as_str(), contains_substring(attribute));
    }
    let cookie = raw.split(';').next().unwrap();
    let token = cookie.strip_prefix("__Host-book-smartz-feedback=").unwrap();
    let response = app.ranking(&format!("{owner}; {cookie}")).await?;
    assert_that!(response.status().as_u16(), eq(200));
    assert_that!(
        display_cookie(&response)?.as_str(),
        contains_substring("Max-Age=0")
    );
    assert_that!(
        response.text().await?.as_str(),
        contains_substring("We saved this book at position 1.")
    );
    assert_that!(
        app.ranking(&owner).await?.text().await?.as_str(),
        not(contains_substring("We saved this book at position"))
    );
    let foreign = app.ranking(&format!("{other}; {cookie}")).await?;
    assert_that!(foreign.status().as_u16(), eq(200));
    let foreign = foreign.text().await?;
    assert_that!(
        foreign.as_str(),
        not(contains_substring("We saved this book at position"))
    );
    assert_that!(
        foreign.as_str(),
        contains_substring("You have no books yet.")
    );
    let mut tampered = token.as_bytes().to_vec();
    let signature = token.rfind('.').unwrap() + 1;
    tampered[signature] = if tampered[signature] == b'A' {
        b'B'
    } else {
        b'A'
    };
    let tampered = String::from_utf8(tampered)?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(token.split('.').nth(1).unwrap())?;
    let claims: Value = serde_json::from_slice(&payload)?;
    let resign = |claims: &Value| {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            claims,
            &jsonwebtoken::EncodingKey::from_secret(
                b"public-fixture-key-at-least-thirty-two-bytes",
            ),
        )
    };
    let mut expired = claims.clone();
    expired["exp"] = json!(chrono::Utc::now().timestamp() - 1);
    let mut wrong_audience = claims;
    wrong_audience["aud"] = json!("book-smartz-add");
    for invalid in [tampered, resign(&expired)?, resign(&wrong_audience)?] {
        let response = app
            .ranking(&format!("{owner}; __Host-book-smartz-feedback={invalid}"))
            .await?;
        assert_that!(response.status().as_u16(), eq(200));
        let html = response.text().await?;
        assert_that!(
            html.as_str(),
            not(contains_substring("We saved this book at position"))
        );
        assert_that!(html.as_str(), contains_substring("Confirmed display book"));
    }
    let replay = post(&app, &owner, "/add", &values).await?;
    assert_that!(replay.status().as_u16(), eq(303));
    let replay_cookie = display_cookie(&replay)?;
    let replay_cookie = replay_cookie.split(';').next().unwrap();
    let html = app
        .ranking(&format!("{owner}; {replay_cookie}"))
        .await?
        .text()
        .await?;
    assert_that!(html.as_str(), contains_substring("We already saved this."));
    assert_that!(
        html.as_str(),
        not(contains_substring("We saved this book at position"))
    );
    let later = offer(
        &app,
        &owner,
        id,
        work("OL900W", "Pending display book"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    let response = post(&app, &owner, "/add", &later).await?;
    assert_that!(response.status().as_u16(), eq(303));
    let pending_cookie = display_cookie(&response)?;
    let html = app
        .ranking(&format!(
            "{owner}; {}",
            pending_cookie.split(';').next().unwrap()
        ))
        .await?
        .text()
        .await?;
    assert_that!(html.as_str(), contains_substring("Book added."));
    assert_that!(
        html.as_str(),
        not(contains_substring("We saved this book at position"))
    );
    let saved = html
        .split("<ol>")
        .nth(1)
        .unwrap()
        .split("</ol>")
        .next()
        .unwrap();
    assert_that!(saved, contains_substring("OL899W"));
    assert_that!(saved, not(contains_substring("OL900W")));
    let selected = post(
        &app,
        &owner,
        "/select",
        &[
            ("csrf".into(), value(&later, "csrf").into()),
            ("selection".into(), value(&later, "selection").into()),
        ],
    )
    .await?
    .text()
    .await?;
    assert_that!(
        selected.as_str(),
        contains_substring(
            "Finish your comparisons or check your earlier save before you add another book."
        )
    );
    assert_that!(
        regex::Regex::new(r"<button[^>]*disabled[^>]*>Add to your books</button>")?
            .is_match(&selected),
        eq(true)
    );
    Ok(())
}
