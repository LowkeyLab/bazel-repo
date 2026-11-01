//! Profile behavior through the delivered application and real PostgreSQL.
use std::time::Duration;

use googletest::{
    assert_that,
    matchers::{contains_substring, eq, not},
};
use reqwest::Response;
use uuid::Uuid;

use crate::{action_test::commit_proxy::CommitProxy, test_support::Fixture};
use book_smartz_domain::ReaderId;

type Values = Vec<(String, String)>;
fn field(body: &str, name: &str) -> anyhow::Result<String> {
    Ok(
        regex::Regex::new(&format!(r#"name="{name}" value="([^"]+)"#))?
            .captures(body)
            .ok_or_else(|| anyhow::anyhow!("missing profile field {name}"))?[1]
            .to_owned(),
    )
}
fn values(body: &str, name: &str, biography: &str, command: &str) -> anyhow::Result<Values> {
    let mut fields = vec![
        ("csrf".into(), field(body, "csrf")?),
        ("offer".into(), field(body, "offer")?),
        ("name".into(), name.into()),
        ("biography".into(), biography.into()),
        ("command".into(), command.into()),
    ];
    if let Ok(checking) = field(body, "checking") {
        fields.push(("checking".into(), checking));
    }
    Ok(fields)
}
fn encoded(values: &Values) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(values)
        .finish()
}
async fn page(app: &Fixture, cookie: &str) -> anyhow::Result<String> {
    Ok(app
        .client
        .get(format!("{}/profile", app.base))
        .header("cookie", cookie)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?)
}
async fn post(app: &Fixture, cookie: &str, values: &Values) -> anyhow::Result<Response> {
    Ok(app
        .client
        .post(format!("{}/profile/edit", app.base))
        .header("cookie", cookie)
        .header("origin", "https://reader.example")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(encoded(values))
        .send()
        .await?)
}
async fn reader(app: &Fixture, subject: &str) -> anyhow::Result<ReaderId> {
    let id: Uuid =
        sqlx::query_scalar("SELECT reader_id FROM book_smartz_web.accounts WHERE subject=$1")
            .bind(subject)
            .fetch_one(&app.pool)
            .await?;
    Ok(ReaderId::new(id))
}
#[googletest::test]
#[tokio::test]
async fn validation_discard_and_conflicts_preserve_confirmed_profile_history() -> anyhow::Result<()>
{
    let mut app = Fixture::new().await?;
    let cookie = app.login("profile-edit-cases").await?;
    let first = page(&app, &cookie).await?;
    let invalid = post(
        &app,
        &cookie,
        &values(&first, &"👩‍👩‍👧‍👦".repeat(101), "Unsaved biography", "save")?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        invalid.as_str(),
        contains_substring("Use 100 visible characters or fewer")
    );
    assert_that!(
        invalid.as_str(),
        contains_substring("aria-invalid=\"true\"")
    );
    assert_that!(
        invalid.as_str(),
        not(contains_substring(
            "class=\"profile-biography\">Unsaved biography"
        ))
    );
    let name = "e\u{301}".repeat(100);
    let biography = "👍🏽".repeat(500);
    let saved = post(&app, &cookie, &values(&invalid, &name, &biography, "save")?)
        .await?
        .text()
        .await?;
    assert_that!(
        saved.as_str(),
        contains_substring("Your profile changes were saved.")
    );
    let a = page(&app, &cookie).await?;
    let b = page(&app, &cookie).await?;
    let newer = post(
        &app,
        &cookie,
        &values(&a, "Newer name", "Paragraph one.\n\nParagraph two.", "save")?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        newer,
        contains_substring("Your profile changes were saved.")
    );
    let conflict = post(
        &app,
        &cookie,
        &values(&b, "Stale name", "Stale biography", "save")?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        conflict.as_str(),
        contains_substring("discarded your local edits")
    );
    assert_that!(
        conflict.as_str(),
        contains_substring("value=\"Newer name\"")
    );
    assert_that!(
        conflict.as_str(),
        not(contains_substring("value=\"Stale name\""))
    );
    let discarded = post(
        &app,
        &cookie,
        &values(&conflict, "Discard me", "Also discard", "discard")?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        discarded.as_str(),
        contains_substring("value=\"Newer name\"")
    );
    let id = reader(&app, "profile-edit-cases").await?;
    let state: (i64, bool) = sqlx::query_as(
        "SELECT revision,import_attempted FROM book_smartz_profiles.profiles WHERE reader_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_one(&app.pool)
    .await?;
    assert_that!(state, eq((2, true)));
    let store =
        book_smartz_storage::Store::new(app.pool.clone(), book_smartz_storage::tracing_observer());
    assert_that!(
        store
            .load_ranking(id, book_smartz_storage::OperationContext::default())
            .await?
            .history()
            .len(),
        eq(0)
    );
    app.restart().await?;
    assert_that!(page(&app, &cookie).await?, contains_substring("Newer name"));
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn each_profile_post_requires_session_origin_csrf_and_reader_bound_offer()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("profile-security").await?;
    let body = page(&app, &cookie).await?;
    let fields = values(&body, "Private name", "Private biography", "save")?;
    for origin in [
        None,
        Some("null"),
        Some("https://foreign.example"),
        Some("https://reader.example/"),
    ] {
        let mut request = app
            .client
            .post(format!("{}/profile/edit", app.base))
            .header("cookie", &cookie)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encoded(&fields));
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        assert_that!(request.send().await?.status().as_u16(), eq(403));
    }
    let mut wrong = fields.clone();
    wrong.iter_mut().find(|(key, _)| key == "csrf").unwrap().1 = "wrong".into();
    assert_that!(
        post(&app, &cookie, &wrong).await?.status().as_u16(),
        eq(403)
    );
    let other = app.login("profile-security-other").await?;
    let other_page = page(&app, &other).await?;
    for command in ["save", "check", "discard"] {
        let mut forged = values(&other_page, "Other text", "Other biography", command)?;
        forged.iter_mut().find(|(key, _)| key == "offer").unwrap().1 = field(&body, "offer")?;
        assert_that!(
            post(&app, &other, &forged).await?.status().as_u16(),
            eq(403)
        );
    }
    assert_that!(post(&app, "", &fields).await?.status().as_u16(), eq(401));
    let id = reader(&app, "profile-security").await?;
    sqlx::query("UPDATE book_smartz_web.sessions SET expires_at=now()-interval '1 second' WHERE reader_id=$1").bind(id.as_uuid()).execute(&app.pool).await?;
    assert_that!(
        post(&app, &cookie, &fields).await?.status().as_u16(),
        eq(401)
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM book_smartz_profile_saves.submissions")
            .fetch_one(&app.pool)
            .await?;
    assert_that!(count, eq(0));
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn definite_database_rejection_retains_the_draft_and_allows_an_explicit_new_save()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("profile-definite-failure").await?;
    let before = page(&app, &cookie).await?;
    sqlx::raw_sql("CREATE FUNCTION profile_reject() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled profile rejection'; END $$; CREATE TRIGGER profile_reject BEFORE INSERT OR UPDATE ON book_smartz_profiles.profiles FOR EACH ROW EXECUTE FUNCTION profile_reject()")
        .execute(&app.pool).await?;
    let failed = post(
        &app,
        &cookie,
        &values(
            &before,
            "Retain this draft",
            "Retain these paragraphs.\n\nSecond paragraph.",
            "save",
        )?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        failed.as_str(),
        contains_substring("Your profile changes were not saved.")
    );
    assert_that!(
        failed.as_str(),
        contains_substring("value=\"Retain this draft\"")
    );
    assert_that!(
        failed.as_str(),
        not(contains_substring("name=\"checking\""))
    );
    sqlx::raw_sql("DROP TRIGGER profile_reject ON book_smartz_profiles.profiles; DROP FUNCTION profile_reject()").execute(&app.pool).await?;
    let saved = post(
        &app,
        &cookie,
        &values(
            &failed,
            "Retain this draft",
            "Retain these paragraphs.\n\nSecond paragraph.",
            "save",
        )?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        saved,
        contains_substring("Your profile changes were saved.")
    );
    let diagnostics = std::fs::read_to_string(app.diagnostics.path())?;
    assert_that!(diagnostics, not(contains_substring("Retain this draft")));
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn real_profile_commit_ack_loss_and_process_interruption_reconcile_one_submission()
-> anyhow::Result<()> {
    for cancel in [false, true] {
        let mut app = Fixture::new().await?;
        let cookie = app.login("profile-commit-uncertainty").await?;
        let other_tab = page(&app, &cookie).await?;
        let body = page(&app, &cookie).await?;
        let fields = values(&body, "Committed name", "Committed biography", "save")?;
        let id = reader(&app, "profile-commit-uncertainty").await?;
        let direct = app.database.clone();
        let proxy = CommitProxy::profile_saved(&app.pool, id).await?;
        app.database = proxy.database(&direct)?;
        app.restart().await?;
        let client = app.client.clone();
        let target = format!("{}/profile/edit", app.base);
        let auth = cookie.clone();
        let encoded_body = encoded(&fields);
        let request = tokio::spawn(async move {
            client
                .post(target)
                .header("cookie", auth)
                .header("origin", "https://reader.example")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(encoded_body)
                .send()
                .await
        });
        proxy.commit_reached().await?;
        if cancel {
            request.abort();
            app.process.0.kill()?;
            app.process.0.wait()?;
            proxy.disconnect();
        } else {
            proxy.disconnect();
            let response = request.await??.text().await?;
            assert_that!(
                response,
                contains_substring("We cannot confirm your profile save yet.")
            );
        }
        app.database = direct;
        app.restart().await?;
        let unresolved = page(&app, &cookie).await?;
        assert_that!(unresolved.as_str(), contains_substring("name=\"checking\""));
        let ranking_offer = crate::action_test::offer(
            &app,
            &cookie,
            id,
            crate::action_test::work("OL771W", "Ranking while profile pending"),
            chrono::Utc::now().timestamp() + 900,
        )
        .await?;
        assert_that!(
            crate::action_test::post(&app, &cookie, "/add", &ranking_offer)
                .await?
                .status()
                .as_u16(),
            eq(303)
        );
        let ranking_before: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT envelope FROM book_smartz.ranking_events WHERE reader_id=$1 ORDER BY sequence",
        )
        .bind(id.as_uuid())
        .fetch_all(&app.pool)
        .await?;
        assert_that!(ranking_before.len(), eq(1));
        let discarded = post(
            &app,
            &cookie,
            &values(
                &unresolved,
                "Committed name",
                "Committed biography",
                "discard",
            )?,
        )
        .await?
        .text()
        .await?;
        assert_that!(
            discarded.as_str(),
            contains_substring("Check your save before discarding")
        );
        let mut changed = fields.clone();
        changed.iter_mut().find(|(key, _)| key == "name").unwrap().1 = "Changed duplicate".into();
        assert_that!(
            post(&app, &cookie, &changed).await?.status().as_u16(),
            eq(403)
        );
        let blocked = post(
            &app,
            &cookie,
            &values(&other_tab, "New tab draft", "Must not overwrite", "save")?,
        )
        .await?
        .text()
        .await?;
        assert_that!(
            blocked.as_str(),
            contains_substring("An earlier profile save needs checking.")
        );
        assert_that!(
            blocked.as_str(),
            contains_substring("value=\"New tab draft\"")
        );
        let conflict = post(
            &app,
            &cookie,
            &values(&blocked, "New tab draft", "Must not overwrite", "check")?,
        )
        .await?
        .text()
        .await?;
        assert_that!(
            conflict.as_str(),
            contains_substring("discarded your local edits")
        );
        let checked = post(
            &app,
            &cookie,
            &values(
                &unresolved,
                "Committed name",
                "Committed biography",
                "check",
            )?,
        )
        .await?
        .text()
        .await?;
        assert_that!(
            checked.as_str(),
            contains_substring("We confirmed your profile save.")
        );
        let repeated = post(
            &app,
            &cookie,
            &values(
                &unresolved,
                "Committed name",
                "Committed biography",
                "check",
            )?,
        )
        .await?
        .text()
        .await?;
        assert_that!(
            repeated,
            contains_substring("We confirmed your profile save.")
        );
        let state:(i64,bool)=sqlx::query_as("SELECT revision,import_attempted FROM book_smartz_profiles.profiles WHERE reader_id=$1").bind(id.as_uuid()).fetch_one(&app.pool).await?;
        assert_that!(state, eq((1, true)));
        let ranking_after: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT envelope FROM book_smartz.ranking_events WHERE reader_id=$1 ORDER BY sequence",
        )
        .bind(id.as_uuid())
        .fetch_all(&app.pool)
        .await?;
        assert_that!(ranking_after, eq(&ranking_before));
    }
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn preparation_uncertainty_and_inflight_checks_cannot_admit_a_late_duplicate()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("profile-prepare-uncertainty").await?;
    let body = page(&app, &cookie).await?;
    let fields = values(&body, "Prepared name", "Prepared biography", "save")?;
    let id = reader(&app, "profile-prepare-uncertainty").await?;
    let direct = app.database.clone();
    let proxy = CommitProxy::profile_prepared(&app.pool, id).await?;
    app.database = proxy.database(&direct)?;
    app.restart().await?;
    let client = app.client.clone();
    let target = format!("{}/profile/edit", app.base);
    let auth = cookie.clone();
    let encoded_body = encoded(&fields);
    let request = tokio::spawn(async move {
        client
            .post(target)
            .header("cookie", auth)
            .header("origin", "https://reader.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encoded_body)
            .send()
            .await
    });
    proxy.commit_reached().await?;
    proxy.disconnect();
    assert_that!(
        request.await??.text().await?,
        contains_substring("cannot confirm your profile save")
    );
    app.database = direct;
    app.restart().await?;
    let pending = page(&app, &cookie).await?;
    let checked = post(
        &app,
        &cookie,
        &values(&pending, "Prepared name", "Prepared biography", "check")?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        checked,
        contains_substring("Your profile changes were not saved.")
    );
    let late = post(&app, &cookie, &fields).await?.text().await?;
    assert_that!(
        late,
        contains_substring("Your profile changes were not saved.")
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM book_smartz_profiles.profiles WHERE reader_id=$1")
            .bind(id.as_uuid())
            .fetch_one(&app.pool)
            .await?;
    assert_that!(count, eq(0));
    let never_sent = page(&app, &cookie).await?;
    let never_fields = values(
        &never_sent,
        "Late original request",
        "Must not be applied",
        "save",
    )?;
    let mut never_check = never_fields.clone();
    never_check
        .iter_mut()
        .find(|(key, _)| key == "command")
        .unwrap()
        .1 = "check".into();
    assert_that!(
        post(&app, &cookie, &never_check).await?.text().await?,
        contains_substring("Your profile changes were not saved.")
    );
    assert_that!(
        post(&app, &cookie, &never_fields).await?.text().await?,
        contains_substring("Your profile changes were not saved.")
    );
    // A check times out conservatively while an application writer holds the account lock.
    let fresh = page(&app, &cookie).await?;
    let values = values(&fresh, "In-flight name", "In-flight biography", "save")?;
    sqlx::raw_sql("CREATE FUNCTION slow_profile() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(8); RETURN NEW; END $$; CREATE TRIGGER slow_profile BEFORE INSERT ON book_smartz_profiles.profiles FOR EACH ROW EXECUTE FUNCTION slow_profile()").execute(&app.pool).await?;
    let client = app.client.clone();
    let target = format!("{}/profile/edit", app.base);
    let auth = cookie.clone();
    let encoded_body = encoded(&values);
    let request = tokio::spawn(async move {
        client
            .post(target)
            .header("cookie", auth)
            .header("origin", "https://reader.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encoded_body)
            .send()
            .await
    });
    tokio::time::timeout(Duration::from_secs(10),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event='PgSleep' AND query LIKE 'INSERT INTO book_smartz_profiles.profiles%')").fetch_one(&app.pool).await?;
            if waiting{return Ok::<_,sqlx::Error>(());}tokio::task::yield_now().await;
        }
    }).await??;
    let mut check = values.clone();
    check
        .iter_mut()
        .find(|(key, _)| key == "command")
        .unwrap()
        .1 = "check".into();
    // The check must remain unresolved while the real writer retains the account lock.
    let client = app.client.clone();
    let target = format!("{}/profile/edit", app.base);
    let auth = cookie.clone();
    let encoded_body = encoded(&check);
    let checking = tokio::spawn(async move {
        client
            .post(target)
            .header("cookie", auth)
            .header("origin", "https://reader.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encoded_body)
            .send()
            .await
    });
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    assert_that!(
        checking.await??.text().await?,
        contains_substring("cannot confirm your profile save")
    );
    assert_that!(
        request.await??.text().await?,
        contains_substring("Your profile changes were saved.")
    );
    assert_that!(
        post(&app, &cookie, &check).await?.text().await?,
        contains_substring("We confirmed your profile save.")
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn ranking_uncertainty_does_not_block_an_independent_profile_save() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("profile-with-ranking-uncertainty").await?;
    let id = reader(&app, "profile-with-ranking-uncertainty").await?;
    let ranking = crate::action_test::offer(
        &app,
        &cookie,
        id,
        crate::action_test::work("OL772W", "Unconfirmed ranking"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    crate::action_test::reject_ranking(&app).await?;
    let failed = crate::action_test::post(&app, &cookie, "/add", &ranking)
        .await?
        .text()
        .await?;
    assert_that!(failed, contains_substring("cannot confirm your save"));
    let before: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT envelope FROM book_smartz.ranking_events WHERE reader_id=$1 ORDER BY sequence",
    )
    .bind(id.as_uuid())
    .fetch_all(&app.pool)
    .await?;
    let profile = page(&app, &cookie).await?;
    let saved = post(
        &app,
        &cookie,
        &values(
            &profile,
            "Independent profile",
            "Independent biography",
            "save",
        )?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        saved,
        contains_substring("Your profile changes were saved.")
    );
    let after: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT envelope FROM book_smartz.ranking_events WHERE reader_id=$1 ORDER BY sequence",
    )
    .bind(id.as_uuid())
    .fetch_all(&app.pool)
    .await?;
    assert_that!(after, eq(&before));
    assert_that!(
        app.ranking(&cookie).await?.text().await?,
        contains_substring("Check your save")
    );
    Ok(())
}
