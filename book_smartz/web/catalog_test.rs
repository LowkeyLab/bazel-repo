//! Catalog acceptance through the real packaged HTTP application.
use googletest::{
    assert_that,
    matchers::{contains_substring, eq},
};
use serde_json::json;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

use crate::test_support::Fixture;

#[googletest::test]
#[tokio::test]
async fn explicit_search_normalizes_work_results_and_issues_selections() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("search-reader").await?;
    let csrf = csrf(&app, &cookie).await?;
    Mock::given(method("GET")).and(path("/search.json"))
        .and(query_param("q", "Tolkien")).and(query_param("page", "2"))
        .and(query_param("limit", "20"))
        .and(header("user-agent", "Book Smartz (maintainer@example.test)"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"start":20,"numFound":42,"docs":[
            {"key":"/works/OL27448W","title":"The Lord of the Rings","author_name":["J. R. R. Tolkien"],"first_publish_year":1954,"cover_i":258_027},
            {"key":"OL27448W","title":"Duplicate"},
            {"key":"OL9W","title":"The Lord of the Rings"}
        ]}))).mount(&app.provider).await;
    let response = post(
        &app,
        &app.base,
        &cookie,
        "search",
        &[("csrf", &csrf), ("query", "Tolkien"), ("page", "2")],
    )
    .await?;
    assert_that!(response.status().as_u16(), eq(200));
    let body = response.text().await?;
    assert_that!(
        body.as_str(),
        contains_substring("https://openlibrary.org/works/OL27448W")
    );
    assert_that!(
        body.as_str(),
        contains_substring("https://covers.openlibrary.org/b/id/258027-M.jpg")
    );
    assert_that!(body.as_str(), contains_substring("Author unavailable"));
    assert_that!(body.as_str(), contains_substring("1954"));
    assert_that!(body.matches("name=\"selection\"").count(), eq(2));
    assert_that!(body.as_str(), contains_substring("Next page"));
    Ok(())
}

async fn csrf(app: &Fixture, cookie: &str) -> anyhow::Result<String> {
    let ranking = app.ranking(cookie).await?.text().await?;
    Ok(regex::Regex::new(r#"name="csrf" value="([^"]+)"#)?
        .captures(&ranking)
        .unwrap()[1]
        .to_owned())
}
async fn post(
    app: &Fixture,
    base: &str,
    cookie: &str,
    route: &str,
    values: &[(&str, &str)],
) -> anyhow::Result<reqwest::Response> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(values.iter().copied())
        .finish();
    Ok(app
        .client
        .post(format!("{base}/{route}"))
        .header("cookie", cookie)
        .header("origin", "https://reader.example")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?)
}
#[googletest::test]
#[tokio::test]
async fn selections_reject_tampering_expiry_wrong_reader_and_invalid_metadata_without_refetch()
-> anyhow::Result<()> {
    use base64::Engine;
    let app = Fixture::new().await?;
    let cookie = app.login("selection-owner").await?;
    let other = app.login("other-reader").await?;
    let csrf = csrf(&app, &cookie).await?;
    let other_csrf = crate::catalog_test::csrf(&app, &other).await?;
    Mock::given(method("GET")).and(path("/search.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"start":0,"num_found":1,"docs":[{"key":"OL123W","title":"<script>alert(1)</script>","author_name":["A & B"]}]})))
        .mount(&app.provider).await;
    let body = post(
        &app,
        &app.base,
        &cookie,
        "search",
        &[("csrf", &csrf), ("query", "title"), ("page", "1")],
    )
    .await?
    .text()
    .await?;
    assert_that!(body.as_str(), contains_substring("&lt;script&gt;"));
    let token = regex::Regex::new(r#"name="selection" value="([^"]+)"#)?
        .captures(&body)
        .unwrap()[1]
        .to_owned();
    app.provider.reset().await;
    let response = post(
        &app,
        &app.base,
        &cookie,
        "select",
        &[("csrf", &csrf), ("selection", &token)],
    )
    .await?;
    assert_that!(response.status().as_u16(), eq(200));
    assert_that!(
        response.text().await?.as_str(),
        contains_substring("Selected work")
    );
    let response = post(
        &app,
        &app.base,
        &other,
        "select",
        &[("csrf", &other_csrf), ("selection", &token)],
    )
    .await?;
    assert_that!(response.status().as_u16(), eq(403));
    let mut forged = token.clone().into_bytes();
    let signature_start = token.rfind('.').unwrap() + 1;
    forged[signature_start] = if forged[signature_start] == b'a' {
        b'b'
    } else {
        b'a'
    };
    let forged = String::from_utf8(forged)?;
    assert_that!(
        post(
            &app,
            &app.base,
            &cookie,
            "select",
            &[("csrf", &csrf), ("selection", &forged)]
        )
        .await?
        .status()
        .as_u16(),
        eq(403)
    );
    let payload: serde_json::Value = serde_json::from_slice(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(token.split('.').nth(1).unwrap())?,
    )?;
    assert_that!(
        payload["exp"].as_i64().unwrap() - payload["iat"].as_i64().unwrap(),
        eq(900)
    );
    for (field, value) in [
        ("exp", json!(0)),
        ("iat", json!(chrono::Utc::now().timestamp() + 100)),
        ("iss", json!("other")),
        ("aud", json!("other")),
        (
            "work",
            json!({"work_id":"OL1M","title":"x","authors":[],"year":null,"cover":null}),
        ),
        (
            "work",
            json!({"work_id":"OL1W","title":" ","authors":[],"year":null,"cover":null}),
        ),
    ] {
        let mut claims = payload.clone();
        claims[field] = value;
        let forged = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(
                b"public-fixture-key-at-least-thirty-two-bytes",
            ),
        )?;
        assert_that!(
            post(
                &app,
                &app.base,
                &cookie,
                "select",
                &[("csrf", &csrf), ("selection", &forged)]
            )
            .await?
            .status()
            .as_u16(),
            eq(403)
        );
    }
    assert_that!(app.provider.received_requests().await.unwrap().len(), eq(0));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn provider_failures_are_actionable_and_never_hide_saved_state() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("catalog-outage-reader").await?;
    let csrf = csrf(&app, &cookie).await?;
    let cases = [
        (
            ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":0,"docs":[]})),
            "No matches found.",
        ),
        (
            ResponseTemplate::new(200).set_body_string("not JSON"),
            "malformed response",
        ),
        (
            ResponseTemplate::new(200).set_body_string("x".repeat(1_048_577)),
            "too large",
        ),
        (ResponseTemplate::new(503), "unavailable"),
        (ResponseTemplate::new(429), "rate limit"),
        (
            ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(6)),
            "timed out",
        ),
        (
            ResponseTemplate::new(302).insert_header("location", "http://127.0.0.1:1/leak"),
            "unavailable",
        ),
    ];
    for (template, message) in cases {
        app.provider.reset().await;
        // Move the deployment gate past the preceding test request without waiting.
        sqlx::query("UPDATE book_smartz_web.search_gate SET next_search_at='-infinity'")
            .execute(&app.pool)
            .await?;
        Mock::given(method("GET"))
            .and(path("/search.json"))
            .respond_with(template)
            .mount(&app.provider)
            .await;
        let response = post(
            &app,
            &app.base,
            &cookie,
            "search",
            &[("csrf", &csrf), ("query", "book"), ("page", "1")],
        )
        .await?;
        assert_that!(response.status().as_u16(), eq(200));
        let body = response.text().await?;
        assert_that!(body.as_str(), contains_substring(message));
        assert_that!(body.as_str(), contains_substring("Retry search"));
        assert_that!(body.as_str(), contains_substring("Your saved ranking"));
        assert_that!(body.as_str(), contains_substring("Your ranking is empty."));
        let saved = app.ranking(&cookie).await?.text().await?;
        assert_that!(saved.as_str(), contains_substring("Your ranking is empty."));
        // Exactly one outgoing request is the budget contract: no automatic retry.
        assert_that!(app.provider.received_requests().await.unwrap().len(), eq(1));
    }
    app.provider.reset().await;
    sqlx::query("UPDATE book_smartz_web.search_gate SET next_search_at='-infinity'")
        .execute(&app.pool)
        .await?;
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"start":0,"numFound":8,"docs":[
                {"key":"/books/OL123M","title":"edition"},
                {"key":"https://evil.example/OL123W","title":"url"},
                {"key":"OL0W","title":"invalid"},
                {"key":"OL1W","title":" "},
                {"key":"/works/OL2W/extra","title":"path"},
                {"key":"OL3W","title":"bad cover","cover_i":-1},
                {"key":"OL4W","title":"Same title"},
                {"key":"/works/OL5W","title":"Same title"}
            ]}),
        ))
        .mount(&app.provider)
        .await;
    let body = post(
        &app,
        &app.base,
        &cookie,
        "search",
        &[("csrf", &csrf), ("query", "book"), ("page", "1")],
    )
    .await?
    .text()
    .await?;
    assert_that!(body.matches("name=\"selection\"").count(), eq(2));
    assert_that!(body.as_str(), contains_substring("/works/OL4W"));
    assert_that!(body.as_str(), contains_substring("/works/OL5W"));
    assert_that!(body.as_str(), contains_substring("cover-placeholder"));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn search_budgets_are_shared_across_overlapping_instances_and_do_not_queue()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("budget-reader").await?;
    let other = app.login("independent-reader").await?;
    let csrf = csrf(&app, &cookie).await?;
    let other_csrf = crate::catalog_test::csrf(&app, &other).await?;
    let (_second, second_base) = app.another_instance().await?;
    app.provider.reset().await;
    Mock::given(method("GET"))
        .and(path("/search.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":0,"docs":[]})),
        )
        .mount(&app.provider)
        .await;
    let values = [("csrf", csrf.as_str()), ("query", "book"), ("page", "1")];
    // Hold the real deployment coordinator, rather than racing a timed sleep.
    let mut connection = app.pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock(706165241,3)")
        .execute(&mut *connection)
        .await?;
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        post(&app, &second_base, &cookie, "search", &values),
    )
    .await??;
    assert_that!(
        response.text().await?.as_str(),
        contains_substring("rate limit")
    );
    sqlx::query("SELECT pg_advisory_unlock(706165241,3)")
        .execute(&mut *connection)
        .await?;
    assert_that!(app.provider.received_requests().await.unwrap().len(), eq(0));
    // A permit consumed by the old process rejects the next process, including
    // while the first process is still present. The persisted future gate removes
    // timing assumptions from this boundary assertion.
    assert_that!(
        post(&app, &app.base, &cookie, "search", &values)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("No matches found.")
    );
    sqlx::query("UPDATE book_smartz_web.search_gate SET next_search_at=clock_timestamp()+interval '1 minute'").execute(&app.pool).await?;
    assert_that!(
        post(
            &app,
            &second_base,
            &other,
            "search",
            &[("csrf", &other_csrf), ("query", "other"), ("page", "1")]
        )
        .await?
        .text()
        .await?
        .as_str(),
        contains_substring("rate limit")
    );
    assert_that!(app.provider.received_requests().await.unwrap().len(), eq(1));
    // Seed nine more recent admissions: the original reader has used all ten.
    sqlx::query("INSERT INTO book_smartz_web.search_admissions SELECT reader_id, clock_timestamp() FROM book_smartz_web.accounts CROSS JOIN generate_series(1,9) WHERE subject='budget-reader'").execute(&app.pool).await?;
    sqlx::query("UPDATE book_smartz_web.search_gate SET next_search_at='-infinity'")
        .execute(&app.pool)
        .await?;
    assert_that!(
        post(&app, &second_base, &cookie, "search", &values)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("rate limit")
    );
    assert_that!(
        post(
            &app,
            &second_base,
            &other,
            "search",
            &[("csrf", &other_csrf), ("query", "other"), ("page", "1")]
        )
        .await?
        .text()
        .await?
        .as_str(),
        contains_substring("No matches found.")
    );
    sqlx::query("UPDATE book_smartz_web.search_gate SET next_search_at='-infinity'")
        .execute(&app.pool)
        .await?;
    sqlx::query("UPDATE book_smartz_web.search_admissions SET admitted_at=clock_timestamp()-interval '61 seconds'").execute(&app.pool).await?;
    assert_that!(
        post(&app, &second_base, &cookie, "search", &values)
            .await?
            .text()
            .await?
            .as_str(),
        contains_substring("No matches found.")
    );
    assert_that!(app.provider.received_requests().await.unwrap().len(), eq(3));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn catalog_boundaries_require_live_sessions_csrf_origin_and_bounded_queries()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("protected-search-reader").await?;
    let csrf = csrf(&app, &cookie).await?;
    for route in ["search", "select"] {
        let fields = [
            ("csrf", csrf.as_str()),
            ("query", "book"),
            ("page", "1"),
            ("selection", "invalid"),
        ];
        assert_that!(
            post(&app, &app.base, "", route, &fields)
                .await?
                .status()
                .as_u16(),
            eq(401)
        );
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish();
        for origin in [None, Some("https://attacker.example")] {
            let mut request = app
                .client
                .post(format!("{}/{route}", app.base))
                .header("cookie", &cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body.clone());
            if let Some(origin) = origin {
                request = request.header("origin", origin);
            }
            assert_that!(request.send().await?.status().as_u16(), eq(403));
        }
        assert_that!(
            post(
                &app,
                &app.base,
                &cookie,
                route,
                &[
                    ("csrf", "wrong"),
                    ("query", "book"),
                    ("page", "1"),
                    ("selection", "invalid")
                ]
            )
            .await?
            .status()
            .as_u16(),
            eq(403)
        );
        assert_that!(
            app.client
                .get(format!("{}/{route}", app.base))
                .header("cookie", &cookie)
                .send()
                .await?
                .status()
                .as_u16(),
            eq(405)
        );
    }
    for (query, page) in [
        ("x".repeat(257), "1"),
        (" ".into(), "1"),
        ("book".into(), "0"),
        ("book".into(), "1000001"),
    ] {
        assert_that!(
            post(
                &app,
                &app.base,
                &cookie,
                "search",
                &[("csrf", &csrf), ("query", &query), ("page", page)]
            )
            .await?
            .status()
            .as_u16(),
            eq(400)
        );
    }
    app.provider.reset().await;
    Mock::given(method("GET")).and(path("/search.json")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"start":0,"numFound":1,"docs":[{"key":"OL123W","title":"📚".repeat(1024),"author_name":vec!["📚".repeat(256);20]}]}))).mount(&app.provider).await;
    let response = post(
        &app,
        &app.base,
        &cookie,
        "search",
        &[("csrf", &csrf), ("query", &"📚".repeat(256)), ("page", "1")],
    )
    .await?;
    assert_that!(response.status().as_u16(), eq(200));
    let body = response.text().await?;
    let token = regex::Regex::new(r#"name="selection" value="([^"]+)"#)?
        .captures(&body)
        .unwrap()[1]
        .to_owned();
    assert_that!(
        post(
            &app,
            &app.base,
            &cookie,
            "select",
            &[("csrf", &csrf), ("selection", &token)]
        )
        .await?
        .status()
        .as_u16(),
        eq(200)
    );
    sqlx::query("UPDATE book_smartz_web.sessions SET expires_at=now()-interval '1 second'")
        .execute(&app.pool)
        .await?;
    for route in ["search", "select"] {
        assert_that!(
            post(
                &app,
                &app.base,
                &cookie,
                route,
                &[
                    ("csrf", &csrf),
                    ("query", "book"),
                    ("page", "1"),
                    ("selection", &token)
                ]
            )
            .await?
            .status()
            .as_u16(),
            eq(401)
        );
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn readiness_ignores_provider_outages_but_requires_catalog_schema() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    app.provider.reset().await;
    assert_that!(
        app.client
            .get(format!("{}/health/ready", app.base))
            .send()
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    sqlx::query("DROP TABLE book_smartz_web.search_gate")
        .execute(&app.pool)
        .await?;
    assert_that!(
        app.client
            .get(format!("{}/health/ready", app.base))
            .send()
            .await?
            .status()
            .as_u16(),
        eq(503)
    );
    Ok(())
}
