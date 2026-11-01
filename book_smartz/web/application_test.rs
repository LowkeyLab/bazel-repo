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
    assert_that!(body, contains_substring("You have no books yet."));
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
        not(contains_substring("You have no books yet."))
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

#[googletest::test]
#[tokio::test]
async fn private_profile_uses_the_current_session_and_saved_ranking() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let profile = || app.client.get(format!("{}/profile", app.base));
    assert_that!(profile().send().await?.status().as_u16(), eq(401));
    let cookie = app.login("profile-reader").await?;
    let response = profile().header("cookie", &cookie).send().await?;
    assert_that!(response.status().as_u16(), eq(200));
    assert_that!(
        response.headers()["cache-control"].to_str()?,
        contains_substring("no-store")
    );
    let body = response.text().await?;
    assert_that!(body.as_str(), contains_substring("Your profile"));
    assert_that!(
        body.as_str(),
        contains_substring("Only you can see your profile.")
    );
    assert_that!(body.as_str(), contains_substring("0 ranked books"));
    assert_that!(
        body.as_str(),
        contains_substring("href=\"/ranking#saved-ranking\"")
    );
    let reader: Uuid = sqlx::query_scalar(
        "SELECT reader_id FROM book_smartz_web.accounts WHERE subject = 'profile-reader'",
    )
    .fetch_one(&app.pool)
    .await?;
    let store = Store::new(app.pool.clone(), tracing_observer());
    let book = Book::new(
        BookId::new(Uuid::new_v4()),
        OpenLibraryWorkId::try_from("OL111W")?,
    );
    store
        .register_book(&book, OperationContext::default())
        .await?;
    assert_that!(
        profile()
            .header("cookie", &cookie)
            .send()
            .await?
            .text()
            .await?,
        contains_substring("0 ranked books")
    );
    store
        .execute(
            ReaderId::new(reader),
            CommandContext {
                expected_revision: 0,
                event_id: EventId::new(Uuid::new_v4()),
                time: chrono::Utc::now(),
            },
            Command::Start { book_id: book.id() },
            OperationContext::default(),
        )
        .await?;
    assert_that!(
        profile()
            .header("cookie", &cookie)
            .send()
            .await?
            .text()
            .await?,
        contains_substring("1 ranked book")
    );
    let pending = Book::new(
        BookId::new(Uuid::new_v4()),
        OpenLibraryWorkId::try_from("OL222W")?,
    );
    store
        .register_book(&pending, OperationContext::default())
        .await?;
    for (revision, command, expected) in [
        (
            1,
            Command::Start {
                book_id: pending.id(),
            },
            "1 ranked book",
        ),
        (2, Command::Pause, "1 ranked book"),
        (3, Command::Resume, "1 ranked book"),
        (
            4,
            Command::Answer {
                opponent: book.id(),
                choice: book_smartz_domain::ComparisonChoice::PreferOpponent,
            },
            "2 ranked books",
        ),
    ] {
        store
            .execute(
                ReaderId::new(reader),
                CommandContext {
                    expected_revision: revision,
                    event_id: EventId::new(Uuid::new_v4()),
                    time: chrono::Utc::now(),
                },
                command,
                OperationContext::default(),
            )
            .await?;
        assert_that!(
            profile()
                .header("cookie", &cookie)
                .send()
                .await?
                .text()
                .await?,
            contains_substring(expected)
        );
    }
    let other = app.login("other-profile-reader").await?;
    assert_that!(
        app.client
            .get(format!("{}/profile?reader_id={reader}", app.base))
            .header("cookie", &other)
            .send()
            .await?
            .text()
            .await?,
        contains_substring("0 ranked books")
    );
    sqlx::query("UPDATE book_smartz_web.sessions SET expires_at = now() - interval '1 second' WHERE reader_id = $1").bind(reader).execute(&app.pool).await?;
    assert_that!(
        profile()
            .header("cookie", &cookie)
            .send()
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    sqlx::query("DELETE FROM book_smartz_web.sessions")
        .execute(&app.pool)
        .await?;
    assert_that!(
        profile()
            .header("cookie", &other)
            .send()
            .await?
            .status()
            .as_u16(),
        eq(401)
    );
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn private_profile_escapes_saved_text_and_never_turns_read_failure_into_zero()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("profile-text-reader").await?;
    let reader: Uuid = sqlx::query_scalar(
        "SELECT reader_id FROM book_smartz_web.accounts WHERE subject = 'profile-text-reader'",
    )
    .fetch_one(&app.pool)
    .await?;
    sqlx::query("INSERT INTO book_smartz_profiles.profiles (reader_id, display_name, biography) VALUES ($1, $2, $3)")
        .bind(reader).bind("<script>private name</script>").bind("First paragraph.\n\nSecond & <b>plain</b>.").execute(&app.pool).await?;
    app.restart().await?;
    let body = app
        .client
        .get(format!("{}/profile", app.base))
        .header("cookie", &cookie)
        .send()
        .await?
        .text()
        .await?;
    assert_that!(
        body.as_str(),
        contains_substring(
            "<h2 class=\"profile-name\">&lt;script&gt;private name&lt;/script&gt;</h2>"
        )
    );
    assert_that!(
        body.as_str(),
        contains_substring("First paragraph.\n\nSecond &amp; &lt;b&gt;plain&lt;/b&gt;.")
    );
    assert_that!(
        body.as_str(),
        not(contains_substring(
            "<h2 class=\"profile-name\"><script>private name"
        ))
    );
    let other = app.login("profile-text-other").await?;
    let other_body = app
        .client
        .get(format!("{}/profile?reader_id={reader}", app.base))
        .header("cookie", &other)
        .send()
        .await?
        .text()
        .await?;
    assert_that!(other_body, not(contains_substring("private name")));
    sqlx::query("ALTER TABLE book_smartz.ranking_events RENAME TO unavailable_ranking_events")
        .execute(&app.pool)
        .await?;
    let response = app
        .client
        .get(format!("{}/profile", app.base))
        .header("cookie", &cookie)
        .send()
        .await?;
    assert_that!(response.status().as_u16(), eq(503));
    assert_that!(
        response.text().await?,
        not(contains_substring("0 ranked books"))
    );
    sqlx::query("ALTER TABLE book_smartz.unavailable_ranking_events RENAME TO ranking_events")
        .execute(&app.pool)
        .await?;
    assert_that!(
        app.client
            .get(format!("{}/profile", app.base))
            .header("cookie", &cookie)
            .send()
            .await?
            .status()
            .as_u16(),
        eq(200)
    );
    let diagnostics = std::fs::read_to_string(app.diagnostics.path())?;
    assert_that!(diagnostics, not(contains_substring("private name")));
    assert_that!(diagnostics, not(contains_substring("First paragraph")));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn profile_changes_save_once_and_survive_restart() -> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("profile-save-reader").await?;
    let page = app
        .client
        .get(format!("{}/profile", app.base))
        .header("cookie", &cookie)
        .send()
        .await?
        .text()
        .await?;
    assert_that!(page.as_str(), contains_substring("Save changes"));
    let field = |body: &str, name: &str| -> anyhow::Result<String> {
        Ok(
            regex::Regex::new(&format!(r#"name="{name}" value="([^"]+)"#))?
                .captures(body)
                .ok_or_else(|| anyhow::anyhow!("missing profile field {name}"))?[1]
                .to_owned(),
        )
    };
    let csrf = field(&page, "csrf")?;
    let offer = field(&page, "offer")?;
    let client = app.client.clone();
    let base = app.base.clone();
    let post = |offer: String, name: &str| {
        client
            .post(format!("{base}/profile/edit"))
            .header("cookie", &cookie)
            .header("origin", "https://reader.example")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(
                url::form_urlencoded::Serializer::new(String::new())
                    .extend_pairs([
                        ("csrf", csrf.clone()),
                        ("offer", offer),
                        ("name", name.to_owned()),
                        ("biography", "First paragraph.\n\nSecond paragraph.".into()),
                        ("command", "save".into()),
                    ])
                    .finish(),
            )
            .send()
    };
    let (first, duplicate) = tokio::join!(
        post(offer.clone(), "  Reader  "),
        post(offer.clone(), "  Reader  ")
    );
    for response in [first?, duplicate?] {
        assert_that!(response.status().as_u16(), eq(200));
        assert_that!(
            response.text().await?,
            contains_substring("Your profile changes were saved.")
        );
    }
    app.restart().await?;
    let current = app
        .client
        .get(format!("{}/profile", app.base))
        .header("cookie", &cookie)
        .send()
        .await?
        .text()
        .await?;
    assert_that!(
        current.as_str(),
        contains_substring("class=\"profile-name\">Reader</h2>")
    );
    Ok(())
}

mod profile_save_test;
