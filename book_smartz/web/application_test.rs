//! Account ownership and saved state through normal application composition.
mod test_support;
use googletest::{
    assert_that,
    matchers::{contains_substring, eq, not},
};
use uuid::Uuid;

use book_smartz_domain::{
    Book, BookId, Command, CommandContext, EventId, OpenLibraryWorkId, ReaderId,
};
use book_smartz_storage::{OperationContext, Store, tracing_observer};
use test_support::Fixture;

#[googletest::test]
#[tokio::test]
async fn concurrent_signup_and_changed_email_restore_one_private_ranking_and_pending_book()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let (a, b) = tokio::join!(app.login("same-account"), app.login("same-account"));
    let (a, b) = (a?, b?);
    // Arrange saved history using the real public store. Assertions stay at HTTP.
    let reader: Uuid = sqlx::query_scalar(
        "SELECT reader_id FROM book_smartz_web.accounts WHERE subject = 'same-account'",
    )
    .fetch_one(&app.pool)
    .await?;
    let reader = ReaderId::new(reader);
    let store = Store::new(app.pool.clone(), tracing_observer());
    let operation = OperationContext::default();
    let first = Book::new(
        BookId::new(Uuid::new_v4()),
        OpenLibraryWorkId::try_from("OL111W")?,
    );
    let pending = Book::new(
        BookId::new(Uuid::new_v4()),
        OpenLibraryWorkId::try_from("OL222W")?,
    );
    store.register_book(&first, operation).await?;
    store.register_book(&pending, operation).await?;
    let time = chrono::DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")?.to_utc();
    for (revision, command) in [
        Command::Start {
            book_id: first.id(),
        },
        Command::Start {
            book_id: pending.id(),
        },
        Command::Pause,
    ]
    .into_iter()
    .enumerate()
    {
        store
            .execute(
                reader,
                CommandContext {
                    expected_revision: u64::try_from(revision)?,
                    event_id: EventId::new(Uuid::new_v4()),
                    time: time + chrono::Duration::seconds(i64::try_from(revision)?),
                },
                command,
                operation,
            )
            .await?;
    }
    for cookie in [&a, &b] {
        let body = app.ranking(cookie).await?.text().await?;
        assert_that!(body.as_str(), contains_substring("Title unavailable"));
        let saved = body
            .split("<ol>")
            .nth(1)
            .unwrap()
            .split("</ol>")
            .next()
            .unwrap();
        assert_that!(
            saved,
            contains_substring("https://openlibrary.org/works/OL111W")
        );
        assert_that!(saved, not(contains_substring("OL222W")));
        assert_that!(
            body.as_str(),
            contains_substring("https://openlibrary.org/works/OL222W")
        );
        assert_that!(
            body.as_str(),
            contains_substring(
                "You paused the comparisons for this book. Resume when you are ready."
            )
        );
    }
    let other = app.login("different-account").await?;
    let response = app
        .client
        .get(format!(
            "{}/ranking?reader_id={}",
            app.base,
            reader.as_uuid()
        ))
        .header("cookie", &other)
        .send()
        .await?;
    let body = response.text().await?;
    assert_that!(body, contains_substring("Your ranking is empty."));
    assert_that!(body, not(contains_substring("OL111W")));
    let login = app.begin().await?;
    let mut claims = Fixture::claims(&login, "same-account");
    claims["email"] = serde_json::json!("changed@example.test");
    claims["iss"] = serde_json::json!("accounts.google.com");
    let response = app.finish(&login, claims).await?;
    let cookie = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.starts_with("__Host-book-smartz-session="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert_that!(
        app.ranking(&cookie).await?.text().await?,
        contains_substring("OL111W")
    );
    app.provider.reset().await;
    app.restart().await?;
    let body = app.ranking(&b).await?.text().await?;
    assert_that!(body, contains_substring("OL111W"));
    assert_that!(body, contains_substring("OL222W"));
    assert_that!(
        store.load_ranking(reader, operation).await?.history().len(),
        eq(3)
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn first_add_is_saved_once_and_restored_with_display_metadata() -> anyhow::Result<()> {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let mut app = Fixture::new().await?;
    let cookie = app.login("first-add-reader").await?;
    let ranking = app.ranking(&cookie).await?.text().await?;
    let field = |html: &str, name: &str| -> anyhow::Result<String> {
        Ok(
            regex::Regex::new(&format!(r#"name="{name}" value="([^"]+)"#))?
                .captures(html)
                .ok_or_else(|| anyhow::anyhow!("missing {name}"))?[1]
                .to_owned(),
        )
    };
    let csrf = field(&ranking, "csrf")?;
    Mock::given(method("GET")).and(path("/search.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"start":0,"numFound":1,"docs":[{"key":"OL27448W","title":"The Lord of the Rings","author_name":["J. R. R. Tolkien"],"cover_i":258_027}]})))
        .mount(&app.provider).await;
    let post = |route: &str, values: Vec<(&str, String)>| {
        app.client
            .post(format!("{}{route}", app.base))
            .header("cookie", &cookie)
            .header("origin", "https://reader.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(
                url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(values)
                    .finish(),
            )
            .send()
    };
    let search = post(
        "/search",
        vec![
            ("csrf", csrf.clone()),
            ("query", "Tolkien".into()),
            ("page", "1".into()),
        ],
    )
    .await?
    .text()
    .await?;
    let selection = field(&search, "selection")?;
    let selected = post(
        "/select",
        vec![("csrf", csrf.clone()), ("selection", selection.clone())],
    )
    .await?
    .text()
    .await?;
    let action = field(&selected, "action")?;
    let revision = field(&selected, "revision")?;
    let offer = field(&selected, "offer")?;
    let values = vec![
        ("csrf", csrf),
        ("selection", selection),
        ("action", action),
        ("revision", revision),
        ("offer", offer),
    ];
    let (first, duplicate) =
        tokio::join!(post("/add", values.clone()), post("/add", values.clone()));
    for response in [first?, duplicate?] {
        assert_that!(response.status().as_u16(), eq(303));
    }
    app.provider.reset().await;
    app.restart().await?;
    let body = app.ranking(&cookie).await?.text().await?;
    assert_that!(body.matches("The Lord of the Rings").count(), eq(1));
    assert_that!(body.as_str(), contains_substring("J. R. R. Tolkien"));
    assert_that!(
        body.as_str(),
        not(contains_substring("Your ranking is empty."))
    );
    Ok(())
}

mod action_test;

#[path = "diagnostic_events.rs"]
#[allow(
    dead_code,
    reason = "The packaged application owns event construction."
)]
mod diagnostic_events;
