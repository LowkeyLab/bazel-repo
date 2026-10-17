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
        let pending = body.split("<aside>").nth(1).unwrap();
        assert_that!(
            pending,
            contains_substring("https://openlibrary.org/works/OL222W")
        );
        assert_that!(pending, contains_substring("paused"));
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
