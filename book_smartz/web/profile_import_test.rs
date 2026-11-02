//! One-time import through the delivered callback and real PostgreSQL.
use std::time::Duration;

use googletest::{
    assert_that,
    matchers::{contains_substring, eq, not},
};
use serde_json::{Value, json};
use uuid::Uuid;

use book_smartz_domain::ReaderId;

use crate::{
    action_test::commit_proxy::CommitProxy,
    profile_save_test::{page as profile, post, values},
    test_support::Fixture,
};

async fn sign_in(app: &Fixture, subject: &str, name: Option<Value>) -> anyhow::Result<String> {
    let login = app.begin().await?;
    let mut claims = Fixture::claims(&login, subject);
    if let Some(name) = name {
        claims["name"] = name;
    }
    let response = app.finish(&login, claims).await?;
    anyhow::ensure!(
        response.status().as_u16() == 303,
        "sign-in did not complete"
    );
    Ok(response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|header| header.to_str().ok())
        .find(|value| value.starts_with("__Host-book-smartz-session="))
        .ok_or_else(|| anyhow::anyhow!("missing session cookie"))?
        .split(';')
        .next()
        .unwrap()
        .to_owned())
}
#[googletest::test]
#[tokio::test]
async fn google_name_is_normalized_once_and_survives_restart_and_later_sign_in()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = sign_in(
        &app,
        "one-time-google-name",
        Some(json!("  Google Reader  ")),
    )
    .await?;
    assert_that!(
        profile(&app, &cookie).await?,
        contains_substring("value=\"Google Reader\"")
    );
    app.restart().await?;
    let cookie = sign_in(
        &app,
        "one-time-google-name",
        Some(json!("Later Google name")),
    )
    .await?;
    assert_that!(
        profile(&app, &cookie).await?,
        contains_substring("value=\"Google Reader\"")
    );
    Ok(())
}

fn name(body: &str) -> anyhow::Result<String> {
    Ok(
        regex::Regex::new(r#"id="profile-name"[^>]*value="([^"]*)"#)?
            .captures(body)
            .ok_or_else(|| anyhow::anyhow!("missing profile name"))?[1]
            .to_owned(),
    )
}
async fn legacy_reader(app: &Fixture, subject: &str) -> anyhow::Result<(String, ReaderId)> {
    let cookie = app.login(subject).await?;
    let id: Uuid =
        sqlx::query_scalar("SELECT reader_id FROM book_smartz_web.accounts WHERE subject=$1")
            .bind(subject)
            .fetch_one(&app.pool)
            .await?;
    // Arrange the state of a reader whose first import is still due after upgrade.
    sqlx::query("UPDATE book_smartz_profiles.profiles SET display_name='',biography='Existing biography',revision=0,import_attempted=false WHERE reader_id=$1")
        .bind(id).execute(&app.pool).await?;
    Ok((cookie, ReaderId::new(id)))
}

#[googletest::test]
#[tokio::test]
async fn missing_unusable_and_boundary_names_complete_the_single_attempt() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cases = [
        (None, String::new()),
        (Some(Value::Null), String::new()),
        (Some(json!({"unusable": "object"})), String::new()),
        (Some(json!(false)), String::new()),
        (Some(json!(42)), String::new()),
        (Some(json!(" \t\n\u{2003}")), String::new()),
        (Some(json!("Two\nlines")), String::new()),
        (Some(json!("e\u{301}".repeat(101))), String::new()),
        (
            Some(json!(format!(" {} ", "e\u{301}".repeat(100)))),
            "e\u{301}".repeat(100),
        ),
        (Some(json!("👩‍👩‍👧‍👦".repeat(100))), "👩‍👩‍👧‍👦".repeat(100)),
        (Some(json!("<b>Plain text</b>")), "<b>Plain text</b>".into()),
    ];
    for (index, (claim, expected)) in cases.into_iter().enumerate() {
        let subject = format!("import-claim-{index}");
        let cookie = sign_in(&app, &subject, claim).await?;
        let body = profile(&app, &cookie).await?;
        assert_that!(name(&body)?, eq(&expected));
        if expected == "<b>Plain text</b>" {
            assert_that!(
                body,
                contains_substring("<h2 class=\"profile-name\">&lt;b&gt;Plain text&lt;/b&gt;</h2>")
            );
        }
        let cookie = sign_in(&app, &subject, Some(json!("Later usable name"))).await?;
        assert_that!(name(&profile(&app, &cookie).await?)?, eq(&expected));
    }
    let diagnostics = std::fs::read_to_string(app.diagnostics.path())?;
    assert_that!(
        diagnostics.as_str(),
        not(contains_substring("Later usable name"))
    );
    assert_that!(diagnostics, not(contains_substring("before@example.test")));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn existing_reader_import_preserves_biography_identity_and_ranking_history()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let (cookie, id) = legacy_reader(&app, "upgraded-reader").await?;
    let offer = crate::action_test::offer(
        &app,
        &cookie,
        id,
        crate::action_test::work("OL913W", "Saved before import"),
        chrono::Utc::now().timestamp() + 900,
    )
    .await?;
    assert_that!(
        crate::action_test::post(&app, &cookie, "/add", &offer)
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let before: Vec<Value> = sqlx::query_scalar(
        "SELECT envelope FROM book_smartz.ranking_events WHERE reader_id=$1 ORDER BY sequence",
    )
    .bind(id.as_uuid())
    .fetch_all(&app.pool)
    .await?;
    let cookie = sign_in(&app, "upgraded-reader", Some(json!("Imported reader"))).await?;
    assert_that!(
        profile(&app, &cookie).await?,
        contains_substring("Existing biography")
    );
    assert_that!(name(&profile(&app, &cookie).await?)?, eq("Imported reader"));
    assert_that!(
        app.ranking(&cookie).await?.text().await?,
        contains_substring("Saved before import")
    );
    app.restart().await?;
    let cookie = sign_in(
        &app,
        "upgraded-reader",
        Some(json!("Changed provider name")),
    )
    .await?;
    assert_that!(name(&profile(&app, &cookie).await?)?, eq("Imported reader"));
    let after: Vec<Value> = sqlx::query_scalar(
        "SELECT envelope FROM book_smartz.ranking_events WHERE reader_id=$1 ORDER BY sequence",
    )
    .bind(id.as_uuid())
    .fetch_all(&app.pool)
    .await?;
    assert_that!(after, eq(&before));
    let current: Uuid = sqlx::query_scalar(
        "SELECT reader_id FROM book_smartz_web.accounts WHERE subject='upgraded-reader'",
    )
    .fetch_one(&app.pool)
    .await?;
    assert_that!(current, eq(*id.as_uuid()));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn explicit_manual_choices_including_an_empty_name_prevent_later_import() -> anyhow::Result<()>
{
    let app = Fixture::new().await?;
    for (index, chosen) in ["Reader choice", ""].into_iter().enumerate() {
        let subject = format!("manual-before-import-{index}");
        let (cookie, _) = legacy_reader(&app, &subject).await?;
        let page = profile(&app, &cookie).await?;
        assert_that!(
            post(
                &app,
                &cookie,
                &values(&page, chosen, "Manual biography", "save")?
            )
            .await?
            .text()
            .await?,
            contains_substring("Your profile changes were saved.")
        );
        let cookie = sign_in(&app, &subject, Some(json!("Google must not replace this"))).await?;
        let page = profile(&app, &cookie).await?;
        assert_that!(name(&page)?, eq(chosen));
        assert_that!(page, contains_substring("Manual biography"));
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn concurrent_first_sign_ins_converge_on_one_import_and_reader() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let (first, second) = tokio::join!(
        sign_in(&app, "simultaneous-import", Some(json!("First candidate"))),
        sign_in(&app, "simultaneous-import", Some(json!("Second candidate"))),
    );
    let first = name(&profile(&app, &first?).await?)?;
    let second = name(&profile(&app, &second?).await?)?;
    assert_that!(
        first.as_str(),
        googletest::matchers::any!(eq("First candidate"), eq("Second candidate"))
    );
    assert_that!(second, eq(&first));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM book_smartz_web.accounts WHERE subject='simultaneous-import'",
    )
    .fetch_one(&app.pool)
    .await?;
    assert_that!(count, eq(1));
    Ok(())
}

async fn writing_profile(app: &Fixture) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event='PgSleep' AND query LIKE 'INSERT INTO book_smartz_profiles.profiles%')").fetch_one(&app.pool).await?;
            if waiting { return Ok::<_, sqlx::Error>(()); }
            tokio::task::yield_now().await;
        }
    }).await??;
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn manual_saves_win_both_serialized_import_race_orders() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    sqlx::raw_sql("CREATE FUNCTION delay_initial_profile() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.display_name='Delayed first write' THEN PERFORM pg_sleep(2); END IF; RETURN NEW; END $$; CREATE TRIGGER delay_initial_profile BEFORE INSERT ON book_smartz_profiles.profiles FOR EACH ROW EXECUTE FUNCTION delay_initial_profile()")
        .execute(&app.pool).await?;
    for import_first in [true, false] {
        let subject = format!("import-save-race-{import_first}");
        let (cookie, _) = legacy_reader(&app, &subject).await?;
        let page = profile(&app, &cookie).await?;
        let chosen = if import_first {
            "Manual winner"
        } else {
            "Delayed first write"
        };
        let imported = if import_first {
            "Delayed first write"
        } else {
            "Google initial"
        };
        let save = async {
            if import_first {
                writing_profile(&app).await?;
            }
            let body = post(
                &app,
                &cookie,
                &values(&page, chosen, "Manual biography", "save")?,
            )
            .await?
            .text()
            .await?;
            assert_that!(body, contains_substring("Your profile changes were saved."));
            Ok::<_, anyhow::Error>(())
        };
        let import = async {
            if !import_first {
                writing_profile(&app).await?;
            }
            sign_in(&app, &subject, Some(json!(imported))).await
        };
        let (saved, imported) = tokio::join!(save, import);
        saved?;
        let page = profile(&app, &imported?).await?;
        assert_that!(name(&page)?, eq(chosen));
        assert_that!(page, contains_substring("Manual biography"));
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn rejected_identity_and_failed_import_cannot_complete_the_attempt() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let (cookie, _) = legacy_reader(&app, "failed-import").await?;
    let login = app.begin().await?;
    let mut claims = Fixture::claims(&login, "failed-import");
    claims["nonce"] = json!("wrong nonce");
    claims["name"] = json!("Untrusted name");
    assert_that!(app.finish(&login, claims).await?.status().as_u16(), eq(401));
    assert_that!(name(&profile(&app, &cookie).await?)?, eq(""));
    sqlx::raw_sql("CREATE FUNCTION reject_initial_profile() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled import rejection'; END $$; CREATE TRIGGER reject_initial_profile BEFORE INSERT ON book_smartz_profiles.profiles FOR EACH ROW EXECUTE FUNCTION reject_initial_profile()")
        .execute(&app.pool).await?;
    let login = app.begin().await?;
    let mut claims = Fixture::claims(&login, "failed-import");
    claims["name"] = json!("Rejected write");
    assert_that!(app.finish(&login, claims).await?.status().as_u16(), eq(500));
    assert_that!(name(&profile(&app, &cookie).await?)?, eq(""));
    sqlx::raw_sql("DROP TRIGGER reject_initial_profile ON book_smartz_profiles.profiles; DROP FUNCTION reject_initial_profile()")
        .execute(&app.pool).await?;
    let cookie = sign_in(&app, "failed-import", Some(json!("Recovered import"))).await?;
    assert_that!(
        name(&profile(&app, &cookie).await?)?,
        eq("Recovered import")
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn a_lost_import_commit_acknowledgement_never_reports_success_or_reimports()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let (cookie, id) = legacy_reader(&app, "uncertain-import").await?;
    let direct = app.database.clone();
    let proxy = CommitProxy::profile_imported(&app.pool, id).await?;
    app.database = proxy.database(&direct)?;
    app.restart().await?;
    let signing_in = sign_in(
        &app,
        "uncertain-import",
        Some(json!("Committed initial name")),
    );
    let interrupt = async {
        proxy.commit_reached().await?;
        proxy.disconnect();
        Ok::<_, anyhow::Error>(())
    };
    let (result, interrupted) = tokio::join!(signing_in, interrupt);
    interrupted?;
    assert_that!(result.is_err(), eq(true));
    app.database = direct;
    app.restart().await?;
    assert_that!(
        name(&profile(&app, &cookie).await?)?,
        eq("Committed initial name")
    );
    let cookie = sign_in(
        &app,
        "uncertain-import",
        Some(json!("Must not replace committed name")),
    )
    .await?;
    assert_that!(
        name(&profile(&app, &cookie).await?)?,
        eq("Committed initial name")
    );
    Ok(())
}
