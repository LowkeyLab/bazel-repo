//! Security and account continuity through the packaged HTTP application.
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
    assert_that!(
        response.text().await?,
        contains_substring("Your ranking is empty.")
    );
    app.restart().await?;
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    let login = app.begin().await?;
    let mut claims = app.claims(&login, "reader-one");
    claims["email"] = json!("changed@example.test");
    let response = app.finish(&login, claims).await?;
    assert_that!(response.status().as_u16(), eq(303));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
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
        let mut claims = app.claims(&login, "reader");
        claims[field] = value;
        assert_that!(app.finish(&login, claims).await?.status().as_u16(), eq(401));
    }
    let login = app.begin().await?;
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &app.claims(&login, "reader"),
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
    let token = jsonwebtoken::encode(&header, &app.claims(&login, "reader"), &key)?;
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
        let mut claims = app.claims(&login, "reader");
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
    let response = app.finish(&a, app.claims(&a, "reader")).await?;
    assert_that!(response.status().as_u16(), eq(303));
    assert_that!(
        app.finish(&a, app.claims(&a, "reader"))
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
