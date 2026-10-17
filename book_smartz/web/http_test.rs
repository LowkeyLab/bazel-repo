//! Exercise the packaged production router, not a substitute HTTP implementation.
use googletest::{assert_that, matchers::eq};

#[googletest::test]
#[tokio::test]
async fn protected_ranking_requires_a_stored_browser_session() -> anyhow::Result<()> {
    let app = test_support::Fixture::new().await?;
    let response = app
        .client
        .get(format!("{}/ranking", app.base))
        .send()
        .await?;
    assert_that!(response.status().as_u16(), eq(401));
    for cookie in [
        "__Host-book-smartz-session=malformed",
        "__Host-book-smartz-session=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    ] {
        let response = app
            .client
            .get(format!("{}/ranking", app.base))
            .header("cookie", cookie)
            .send()
            .await?;
        assert_that!(response.status().as_u16(), eq(401));
    }
    let response = app
        .client
        .get(format!("{}/logout", app.base))
        .send()
        .await?;
    assert_that!(response.status().as_u16(), eq(405));
    Ok(())
}

mod identity_test;
mod test_support;

mod catalog_test;
