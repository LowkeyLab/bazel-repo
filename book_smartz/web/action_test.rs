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
async fn post(
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
async fn offer(
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
fn work(id: &str, title: &str) -> Value {
    json!({"work_id":id,"title":title,"authors":["Saved Author"],"cover":123,"year":null})
}
fn value<'a>(values: &'a [(String, String)], key: &str) -> &'a str {
    &values.iter().find(|(k, _)| k == key).unwrap().1
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
    assert_that!(
        post(&app, &a, "/add", &stale).await?.text().await?.as_str(),
        contains_substring("ranking changed")
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
        contains_substring("Already ranked")
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
    Ok(())
}

async fn reject_ranking(app: &Fixture) -> anyhow::Result<()> {
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
        contains_substring("Checking")
    );
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
        contains_substring("Checking")
    );
    assert_that!(store(&app).load_application_history(id).await?, eq(&before));
    app.restart().await?;
    assert_that!(
        app.ranking(&cookie).await?.text().await?.as_str(),
        contains_substring("Check original save")
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
        contains_substring("Checking")
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
        contains_substring("Checking")
    );
    assert_that!(
        store(&app).load_application_history(id).await?,
        eq(&history)
    );
    assert_that!(
        app.ranking(&cookie).await?.text().await?.as_str(),
        contains_substring("Further ranking changes are blocked")
    );
    Ok(())
}

#[path = "commit_proxy.rs"]
mod commit_proxy;

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
                contains_substring("Checking")
            );
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
        contains_substring("Checking an earlier save")
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
        contains_substring("Checking")
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
