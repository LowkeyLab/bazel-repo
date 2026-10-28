//! Security and account continuity through the packaged HTTP application.
use anyhow::Context;
use googletest::{
    assert_that,
    matchers::{contains_substring, eq, not},
};
use serde_json::json;

use crate::test_support::Fixture;

#[googletest::test]
#[tokio::test]
async fn sign_in_restores_the_same_private_account_across_restart_and_email_change()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let home = app.client.get(format!("{}/", app.base)).send().await?;
    assert_that!(home.status().as_u16(), eq(200));
    let home = home.text().await?;
    for copy in [
        "Books, ranked, not rated.",
        "Put the books you read in order of preference. Choose between two books at a time to build your private ranking.",
        "Find your first book.",
        "<title>Book Smartz · Books, ranked, not rated</title>",
        "Rank the books you read. Choose between two books at a time to build your private ranking with Book Smartz.",
        "href=\"/auth/google\"",
        "Sign in with Google",
    ] {
        assert_that!(home.as_str(), contains_substring(copy));
    }
    let cookie = app.login("reader-one").await?;
    let response = app.ranking(&cookie).await?;
    assert_that!(response.status().as_u16(), eq(200));
    assert_that!(response.headers().contains_key("set-cookie"), eq(false));
    assert_that!(
        response
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        eq(Some("no-store"))
    );
    let ranking = response.text().await?;
    for copy in [
        "<title>Your books · Book Smartz</title>",
        "aria-label=\"Books\"",
        "aria-label=\"Books workspace\"",
        "Find a book",
        "Your books",
        "You have no books yet.",
        "Find a book you read.",
        "Sign out of this browser",
    ] {
        assert_that!(ranking.as_str(), contains_substring(copy));
    }
    assert_that!(
        ranking.as_str(),
        not(contains_substring(
            "Finish the comparisons for each new book"
        ))
    );
    assert_that!(
        ranking.as_str(),
        not(contains_substring("Books in your order of preference"))
    );
    app.restart().await?;
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    let login = app.begin().await?;
    let mut claims = Fixture::claims(&login, "reader-one");
    claims["email"] = json!("changed@example.test");
    let response = app.finish(&login, claims).await?;
    assert_that!(response.status().as_u16(), eq(303));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn audience_and_authorized_presenter_control_session_authority() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    for (audience, presenter, expected_status) in [
        (json!(["fixture-client", "other-client"]), None, 401),
        (
            json!(["fixture-client", "other-client"]),
            Some("other-client"),
            401,
        ),
        (
            json!(["fixture-client", "other-client"]),
            Some("fixture-client"),
            303,
        ),
        (
            json!(["other-client", "another-client"]),
            Some("fixture-client"),
            401,
        ),
        (json!("fixture-client"), None, 303),
    ] {
        let login = app.begin().await?;
        let mut claims = Fixture::claims(&login, "reader");
        claims["aud"] = audience;
        claims.as_object_mut().unwrap().remove("azp");
        if let Some(presenter) = presenter {
            claims["azp"] = json!(presenter);
        }
        let sessions_before: i64 =
            sqlx::query_scalar("SELECT count(*) FROM book_smartz_web.sessions")
                .fetch_one(&app.pool)
                .await?;
        let response = app.finish(&login, claims).await?;
        assert_that!(response.status().as_u16(), eq(expected_status));
        let session_cookie = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .find(|value| value.starts_with("__Host-book-smartz-session="));
        if expected_status == 401 {
            assert_that!(session_cookie, eq(None));
            let sessions_after: i64 =
                sqlx::query_scalar("SELECT count(*) FROM book_smartz_web.sessions")
                    .fetch_one(&app.pool)
                    .await?;
            assert_that!(sessions_after, eq(sessions_before));
        } else {
            let cookie = session_cookie.context("Missing session cookie")?;
            let cookie = cookie.split(';').next().unwrap();
            assert_that!(app.ranking(cookie).await?.status().as_u16(), eq(200));
        }
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn rejects_invalid_identity_claims_signatures_and_callback_state() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    for (field, value) in [
        ("iss", json!("https://attacker.example")),
        ("aud", json!("other-client")),
        ("azp", json!("other-client")),
        ("exp", json!(1)),
        ("nonce", json!("other-login")),
        ("sub", json!("")),
    ] {
        let login = app.begin().await?;
        let mut claims = Fixture::claims(&login, "reader");
        claims[field] = value;
        assert_that!(app.finish(&login, claims).await?.status().as_u16(), eq(401));
    }
    let login = app.begin().await?;
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &Fixture::claims(&login, "reader"),
        &jsonwebtoken::EncodingKey::from_secret(b"public-invalid-test-key"),
    )?;
    assert_that!(
        app.finish_token(&login, &token).await?.status().as_u16(),
        eq(401)
    );
    let login = app.begin().await?;
    assert_that!(
        app.finish_token(&login, "not.a.signature")
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    let login = app.begin().await?;
    let key =
        jsonwebtoken::EncodingKey::from_rsa_pem(include_bytes!("fixtures/public-test-key.pem"))?;
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some("fixture".into());
    let token = jsonwebtoken::encode(&header, &Fixture::claims(&login, "reader"), &key)?;
    let signature = token.rfind('.').unwrap() + 1;
    let mut corrupted = token.into_bytes();
    corrupted[signature] = if corrupted[signature] == b'A' {
        b'B'
    } else {
        b'A'
    };
    assert_that!(
        app.finish_token(&login, &String::from_utf8(corrupted)?)
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    for missing in ["exp", "aud", "iss", "nonce", "sub"] {
        let login = app.begin().await?;
        let mut claims = Fixture::claims(&login, "reader");
        claims.as_object_mut().unwrap().remove(missing);
        assert_that!(app.finish(&login, claims).await?.status().as_u16(), eq(401));
    }
    let a = app.begin().await?;
    let b = app.begin().await?;
    assert_that!(
        app.callback(&a, "unexchanged", &a.state, &b.cookie)
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    assert_that!(
        app.callback(&a, "unexchanged", &a.state, "")
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    let response = app.finish(&a, Fixture::claims(&a, "reader")).await?;
    assert_that!(response.status().as_u16(), eq(303));
    assert_that!(
        app.finish(&a, Fixture::claims(&a, "reader"))
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    Ok(())
}

fn csrf(html: &str) -> String {
    regex::Regex::new(r#"name="csrf" value="([^"]+)"#)
        .unwrap()
        .captures(html)
        .unwrap()[1]
        .to_owned()
}
async fn logout(
    app: &Fixture,
    cookie: &str,
    token: &str,
    origin: Option<&str>,
) -> anyhow::Result<reqwest::Response> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("csrf", token)
        .finish();
    let mut request = app
        .client
        .post(format!("{}/logout", app.base))
        .header("cookie", cookie)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body);
    if let Some(origin) = origin {
        request = request.header("origin", origin);
    }
    Ok(request.send().await?)
}

#[googletest::test]
#[tokio::test]
async fn logout_requires_its_own_session_csrf_and_canonical_origin_and_revokes_only_this_browser()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let a = app.login("reader").await?;
    let b = app.login("reader").await?;
    assert_that!(
        app.client
            .get(format!("{}/logout", app.base))
            .header("cookie", &a)
            .send()
            .await?
            .status()
            .as_u16(),
        eq(405)
    );
    assert_that!(app.ranking(&a).await?.status().as_u16(), eq(200));
    let token = csrf(&app.ranking(&a).await?.text().await?);
    let other = csrf(&app.ranking(&b).await?.text().await?);
    assert_that!(
        logout(&app, &a, &other, Some("https://reader.example"))
            .await?
            .status()
            .as_u16(),
        eq(403)
    );
    assert_that!(
        logout(&app, &a, &token, None).await?.status().as_u16(),
        eq(403)
    );
    assert_that!(
        logout(&app, &a, &token, Some("https://attacker.example"))
            .await?
            .status()
            .as_u16(),
        eq(403)
    );
    let response = app
        .client
        .post(format!("{}/logout", app.base))
        .header("cookie", &a)
        .header("origin", "http://attacker.example")
        .header("host", "attacker.example")
        .header("x-forwarded-host", "reader.example")
        .header("x-forwarded-proto", "https")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(format!("csrf={token}"))
        .send()
        .await?;
    assert_that!(response.status().as_u16(), eq(403));
    assert_that!(
        logout(&app, "", &token, Some("https://reader.example"))
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    assert_that!(
        logout(&app, &a, &token, Some("https://reader.example"))
            .await?
            .status()
            .as_u16(),
        eq(303)
    );
    assert_that!(app.ranking(&a).await?.status().as_u16(), eq(401));
    assert_that!(app.ranking(&b).await?.status().as_u16(), eq(200));
    sqlx::query("UPDATE book_smartz_web.sessions SET expires_at = now() - interval '1 second'")
        .execute(&app.pool)
        .await?;
    assert_that!(app.ranking(&b).await?.status().as_u16(), eq(401));
    assert_that!(
        logout(&app, &b, &other, Some("https://reader.example"))
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn login_discards_expired_challenges_and_requests_only_openid() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    app.begin().await?;
    sqlx::query(
        "UPDATE book_smartz_web.login_challenges SET expires_at = now() - interval '1 second'",
    )
    .execute(&app.pool)
    .await?;
    app.begin().await?;
    let expired: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM book_smartz_web.login_challenges WHERE expires_at <= now()",
    )
    .fetch_one(&app.pool)
    .await?;
    assert_that!(expired, eq(0));
    let response = app
        .client
        .get(format!("{}/auth/google", app.base))
        .send()
        .await?;
    let destination = url::Url::parse(response.headers().get("location").unwrap().to_str()?)?;
    let scope = destination
        .query_pairs()
        .find(|(key, _)| key == "scope")
        .unwrap()
        .1
        .into_owned();
    assert_that!(scope, eq("openid"));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn valid_cached_keys_survive_a_key_endpoint_outage() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    app.login("first-reader").await?;
    app.provider.reset().await;
    // The token endpoint is reinstalled by login; the key endpoint stays absent.
    app.login("second-reader").await?;
    let requests = app.provider.received_requests().await.unwrap();
    assert_that!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/keys")
            .count(),
        eq(0)
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn unknown_keys_refresh_and_expired_keys_do_not_bypass_an_outage() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    app.login("before-rotation").await?;
    app.provider.reset().await;
    app.install_keys("rotated", "public, max-age=0").await?;
    let login = app.begin().await?;
    let response = app
        .finish_with_kid(&login, Fixture::claims(&login, "after-rotation"), "rotated")
        .await?;
    assert_that!(response.status().as_u16(), eq(303));
    let requests = app.provider.received_requests().await.unwrap();
    assert_that!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/keys")
            .count(),
        eq(1)
    );
    app.provider.reset().await;
    let login = app.begin().await?;
    let response = app
        .finish_with_kid(&login, Fixture::claims(&login, "expired-key"), "rotated")
        .await?;
    assert_that!(response.status().as_u16(), eq(401));
    Ok(())
}
