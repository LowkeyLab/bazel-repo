//! Profile recovery through normal HTTP, signed identity, and real PostgreSQL.
use crate::{
    profile_save_test::{encoded, page, post, values},
    test_support::Fixture,
};
use googletest::{
    assert_that,
    matchers::{contains_substring, eq, not},
};

fn cookie(response: &reqwest::Response) -> anyhow::Result<String> {
    Ok(response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("__Host-book-smartz-session="))
        .ok_or_else(|| anyhow::anyhow!("missing session"))?
        .split(';')
        .next()
        .unwrap()
        .to_owned())
}
fn sign_in_link(body: &str) -> anyhow::Result<String> {
    Ok(
        regex::Regex::new(r#"href="(/auth/google\?profile=[^"]+)""#)?
            .captures(body)
            .ok_or_else(|| anyhow::anyhow!("missing profile recovery sign-in link"))?[1]
            .to_owned(),
    )
}
#[googletest::test]
#[tokio::test]
async fn expired_profile_draft_returns_only_after_explicit_same_reader_sign_in()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let original = app.login("profile-expiry").await?;
    let before = page(&app, &original).await?;
    sqlx::query("UPDATE book_smartz_web.sessions SET expires_at=now()-interval '1 second'")
        .execute(&app.pool)
        .await?;
    let expired = post(
        &app,
        "",
        &values(
            &before,
            "Private unsaved name",
            "Private unsaved biography",
            "save",
        )?,
    )
    .await?;
    assert_that!(expired.status().as_u16(), eq(200));
    let body = expired.text().await?;
    let link = sign_in_link(&body)?;
    app.restart().await?;
    let login = app.begin_at(&link).await?;
    let response = app
        .finish(&login, Fixture::claims(&login, "profile-expiry"))
        .await?;
    assert_that!(response.status().as_u16(), eq(303));
    let session = cookie(&response)?;
    let destination = response.headers().get("location").unwrap().to_str()?;
    assert_that!(destination, contains_substring("/profile?recovery="));
    let recovered = app
        .client
        .get(format!("{}{destination}", app.base))
        .header("cookie", &session)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    assert_that!(
        recovered.as_str(),
        contains_substring(r#"value="Private unsaved name""#)
    );
    assert_that!(
        recovered.as_str(),
        contains_substring("Private unsaved biography")
    );
    assert_that!(
        recovered.as_str(),
        not(contains_substring(
            r#"class="profile-name">Private unsaved name"#
        ))
    );
    let ordinary = page(&app, &session).await?;
    assert_that!(
        ordinary.as_str(),
        not(contains_substring("Private unsaved name"))
    );
    let saved = post(
        &app,
        &session,
        &values(
            &recovered,
            "Private unsaved name",
            "Private unsaved biography",
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
    Ok(())
}

async fn capture(app: &Fixture, body: &str, name: &str) -> anyhow::Result<String> {
    sqlx::query("UPDATE book_smartz_web.sessions SET expires_at=now()-interval '1 second'")
        .execute(&app.pool)
        .await?;
    let response = post(
        app,
        "",
        &values(body, name, "Retained private biography", "save")?,
    )
    .await?;
    assert_that!(response.status().as_u16(), eq(200));
    sign_in_link(&response.text().await?)
}
async fn resume(app: &Fixture, link: &str, subject: &str) -> anyhow::Result<(String, String)> {
    let login = app.begin_at(link).await?;
    let response = app.finish(&login, Fixture::claims(&login, subject)).await?;
    let session = cookie(&response)?;
    Ok((
        session,
        response
            .headers()
            .get("location")
            .unwrap()
            .to_str()?
            .to_owned(),
    ))
}
async fn get(app: &Fixture, session: &str, path: &str) -> anyhow::Result<reqwest::Response> {
    Ok(app
        .client
        .get(format!("{}{path}", app.base))
        .header("cookie", session)
        .send()
        .await?)
}
#[googletest::test]
#[tokio::test]
async fn another_reader_cannot_receive_or_restore_the_original_draft() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let original = app.login("recovery-owner").await?;
    let before = page(&app, &original).await?;
    let link = capture(&app, &before, "Owner-only draft").await?;
    let id = link.strip_prefix("/auth/google?profile=").unwrap();
    let (other, destination) = resume(&app, &link, "different-reader").await?;
    assert_that!(destination, eq("/ranking"));
    assert_that!(
        page(&app, &other).await?,
        not(contains_substring("Owner-only draft"))
    );
    assert_that!(
        get(&app, &other, &format!("/profile?recovery={id}"))
            .await?
            .status()
            .as_u16(),
        eq(403)
    );
    let owner = app.login("recovery-owner").await?;
    assert_that!(
        get(&app, &owner, &format!("/profile?recovery={id}"))
            .await?
            .status()
            .as_u16(),
        eq(403)
    );
    assert_that!(
        page(&app, &owner).await?,
        not(contains_substring("Owner-only draft"))
    );
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn recovery_requires_original_form_csrf_origin_and_sign_out_invalidates_it()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let original = app.login("recovery-security").await?;
    let before = page(&app, &original).await?;
    let fields = values(&before, "Protected draft", "Private biography", "save")?;
    sqlx::query("UPDATE book_smartz_web.sessions SET expires_at=now()-interval '1 second'")
        .execute(&app.pool)
        .await?;
    for origin in [
        None,
        Some("null"),
        Some("https://foreign.example"),
        Some("https://reader.example/"),
    ] {
        let mut request = app
            .client
            .post(format!("{}/profile/edit", app.base))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encoded(&fields));
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        assert_that!(request.send().await?.status().as_u16(), eq(403));
    }
    let mut wrong = fields.clone();
    wrong.iter_mut().find(|(k, _)| k == "csrf").unwrap().1 = "wrong".into();
    assert_that!(post(&app, "", &wrong).await?.status().as_u16(), eq(403));
    let link = sign_in_link(&post(&app, "", &fields).await?.text().await?)?;
    let (session, destination) = resume(&app, &link, "recovery-security").await?;
    let restored = get(&app, &session, &destination).await?.text().await?;
    assert_that!(restored.as_str(), contains_substring("Protected draft"));
    let csrf = regex::Regex::new(r#"name="csrf" value="([^"]+)"#)?
        .captures(&restored)
        .unwrap()[1]
        .to_owned();
    assert_that!(
        app.client
            .post(format!("{}/logout", app.base))
            .header("cookie", &session)
            .header("origin", "https://reader.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encoded(&[("csrf".into(), csrf)]))
            .send()
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let session = app.login("recovery-security").await?;
    assert_that!(
        get(&app, &session, &destination).await?.status().as_u16(),
        eq(403)
    );
    assert_that!(post(&app, "", &fields).await?.status().as_u16(), eq(403));
    assert_that!(
        page(&app, &session).await?,
        not(contains_substring("Protected draft"))
    );
    let diagnostics = std::fs::read_to_string(app.diagnostics.path())?;
    assert_that!(diagnostics, not(contains_substring("Protected draft")));
    Ok(())
}
#[googletest::test]
#[tokio::test]
async fn uncertain_profile_submission_keeps_its_original_check_after_expiry_and_sign_out()
-> anyhow::Result<()> {
    use crate::action_test::commit_proxy::CommitProxy;
    use book_smartz_domain::ReaderId;
    let mut app = Fixture::new().await?;
    let original = app.login("recovery-uncertain").await?;
    let before = page(&app, &original).await?;
    let fields = values(
        &before,
        "Original uncertain name",
        "Retained private biography",
        "save",
    )?;
    let id: uuid::Uuid = sqlx::query_scalar(
        "SELECT reader_id FROM book_smartz_web.accounts WHERE subject='recovery-uncertain'",
    )
    .fetch_one(&app.pool)
    .await?;
    let direct = app.database.clone();
    let proxy = CommitProxy::profile_saved(&app.pool, ReaderId::new(id)).await?;
    app.database = proxy.database(&direct)?;
    app.restart().await?;
    let client = app.client.clone();
    let target = format!("{}/profile/edit", app.base);
    let auth = original.clone();
    let request = tokio::spawn(async move {
        client
            .post(target)
            .header("cookie", auth)
            .header("origin", "https://reader.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encoded(&fields))
            .send()
            .await
    });
    proxy.commit_reached().await?;
    proxy.disconnect();
    let uncertain = request.await??.text().await?;
    assert_that!(
        uncertain.as_str(),
        contains_substring("cannot confirm your profile save")
    );
    app.database = direct;
    app.restart().await?;
    let link = capture(&app, &uncertain, "Original uncertain name").await?;
    let (session, destination) = resume(&app, &link, "recovery-uncertain").await?;
    let restored = get(&app, &session, &destination).await?.text().await?;
    assert_that!(restored.as_str(), contains_substring("Check your save"));
    let blocked = post(
        &app,
        &session,
        &values(
            &restored,
            "Original uncertain name",
            "Retained private biography",
            "save",
        )?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        blocked,
        contains_substring("Check your save before another profile save.")
    );
    assert_that!(
        get(&app, &session, "/ranking").await?.status().as_u16(),
        eq(200)
    );
    let csrf = regex::Regex::new(r#"name="csrf" value="([^"]+)"#)?
        .captures(&restored)
        .unwrap()[1]
        .to_owned();
    app.client
        .post(format!("{}/logout", app.base))
        .header("cookie", &session)
        .header("origin", "https://reader.example")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(encoded(&[("csrf".into(), csrf)]))
        .send()
        .await?;
    let session = app.login("recovery-uncertain").await?;
    assert_that!(
        get(&app, &session, &destination).await?.status().as_u16(),
        eq(403)
    );
    let pending = page(&app, &session).await?;
    assert_that!(pending.as_str(), contains_substring("Check your save"));
    let checked = post(
        &app,
        &session,
        &values(
            &pending,
            "Original uncertain name",
            "Retained private biography",
            "check",
        )?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        checked,
        contains_substring("We confirmed your profile save.")
    );
    assert_that!(
        page(&app, &session).await?,
        contains_substring("Original uncertain name")
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn sign_out_preserves_another_browsers_private_draft() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let first = app.login("separate-browser-drafts").await?;
    let first_page = page(&app, &first).await?;
    let first_link = capture(&app, &first_page, "First browser draft").await?;
    let (first, first_destination) = resume(&app, &first_link, "separate-browser-drafts").await?;
    let first_page = get(&app, &first, &first_destination).await?.text().await?;
    let second = app.login("separate-browser-drafts").await?;
    let second_page = page(&app, &second).await?;
    let csrf_field = regex::Regex::new(r#"name="csrf" value="([^"]+)"#)?;
    let second_csrf = csrf_field.captures(&second_page).unwrap()[1].to_owned();
    sqlx::query(
        "UPDATE book_smartz_web.sessions SET expires_at=now()-interval '1 second' WHERE csrf=$1",
    )
    .bind(second_csrf)
    .execute(&app.pool)
    .await?;
    let second_link = sign_in_link(
        &post(
            &app,
            "",
            &values(
                &second_page,
                "Second browser draft",
                "Separate biography",
                "save",
            )?,
        )
        .await?
        .text()
        .await?,
    )?;
    let (second, second_destination) =
        resume(&app, &second_link, "separate-browser-drafts").await?;
    let csrf = csrf_field.captures(&first_page).unwrap()[1].to_owned();
    assert_that!(
        app.client
            .post(format!("{}/logout", app.base))
            .header("cookie", &first)
            .header("origin", "https://reader.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encoded(&[("csrf".into(), csrf)]))
            .send()
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let recovered = get(&app, &second, &second_destination).await?;
    assert_that!(recovered.status().as_u16(), eq(200));
    assert_that!(
        recovered.text().await?,
        contains_substring("Second browser draft")
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn ordinary_sign_in_then_sign_out_removes_an_abandoned_recovery_draft() -> anyhow::Result<()>
{
    let app = Fixture::new().await?;
    let original = app.login("abandoned-recovery").await?;
    let response = get(&app, &original, "/profile").await?;
    let browser_cookies = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap())
        .collect::<Vec<_>>()
        .join("; ");
    let before = response.text().await?;
    let link = capture(&app, &before, "Abandoned private draft").await?;
    let destination = format!(
        "/profile?recovery={}",
        link.strip_prefix("/auth/google?profile=").unwrap()
    );
    // The reader uses ordinary sign-in instead of the recovery link, then signs out.
    let renewed = app.login("abandoned-recovery").await?;
    let auth = format!("{renewed}; {browser_cookies}");
    let current = get(&app, &auth, "/profile").await?.text().await?;
    let csrf = regex::Regex::new(r#"name="csrf" value="([^"]+)"#)?
        .captures(&current)
        .unwrap()[1]
        .to_owned();
    assert_that!(
        app.client
            .post(format!("{}/logout", app.base))
            .header("cookie", &auth)
            .header("origin", "https://reader.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encoded(&[("csrf".into(), csrf)]))
            .send()
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    let session = app.login("abandoned-recovery").await?;
    assert_that!(
        get(&app, &session, &destination).await?.status().as_u16(),
        eq(403)
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn an_original_native_form_requires_check_when_its_saved_response_was_not_received()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let session = app.login("native-response-loss").await?;
    let before = page(&app, &session).await?;
    let fields = values(
        &before,
        "Name before response loss",
        "Retained private biography",
        "save",
    )?;
    let response = post(&app, &session, &fields).await?;
    assert_that!(response.status().as_u16(), eq(200));
    // The client retains the original form instead of consuming the response page.
    drop(response);
    let link = capture(&app, &before, "Name before response loss").await?;
    let (session, destination) = resume(&app, &link, "native-response-loss").await?;
    let restored = get(&app, &session, &destination).await?.text().await?;
    assert_that!(restored.as_str(), contains_substring(r#"value="check""#));
    let blocked = post(
        &app,
        &session,
        &values(
            &restored,
            "Name before response loss",
            "Retained private biography",
            "save",
        )?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        blocked,
        contains_substring("Check your save before another profile save.")
    );
    let checked = post(
        &app,
        &session,
        &values(
            &restored,
            "Name before response loss",
            "Retained private biography",
            "check",
        )?,
    )
    .await?
    .text()
    .await?;
    assert_that!(
        checked,
        contains_substring("We confirmed your profile save.")
    );
    Ok(())
}
