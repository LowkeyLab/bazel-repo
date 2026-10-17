//! Acceptance of the delivered archive and launcher from unrelated directories.
mod test_support;
use std::{
    fs,
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::Context;
use googletest::{
    assert_that,
    matchers::{contains_substring, eq, ge, not},
};

use test_support::{Fixture, Process};

#[googletest::test]
#[tokio::test]
async fn packaged_pages_deliver_local_css_fonts_runtime_and_icons() -> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("packaged-reader").await?;
    app.restart().await?;
    let body = app.ranking(&cookie).await?.text().await?;
    assert_that!(body.as_str(), contains_substring("<svg"));
    assert_that!(body.as_str(), not(contains_substring("bazel-out")));
    let integrity: serde_json::Value =
        serde_json::from_slice(&fs::read(app.package.path().join("integrity.json"))?)?;
    let packaged_styles: Vec<Vec<u8>> = integrity["files"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|name| name.ends_with(".css"))
        .map(|name| fs::read(app.package.path().join("assets").join(name)))
        .collect::<std::io::Result<_>>()?;
    let mut served_styles = vec![false; packaged_styles.len()];
    let references = regex::Regex::new(r#"(?:href|src)="([^"]+)"#)?;
    let font_references = regex::Regex::new(r#"url\(["']?([^"')]+)"#)?;
    let base = url::Url::parse(&app.base)?;
    let (mut css, mut scripts, mut fonts) = (0, 0, 0);
    for capture in references.captures_iter(&body) {
        let url = base.join(&capture[1])?;
        anyhow::ensure!(
            url.host_str() == base.host_str(),
            "unexpected remote page asset"
        );
        let response = app
            .client
            .get(url.clone())
            .header("cookie", &cookie)
            .send()
            .await?;
        assert_that!(response.status().as_u16(), eq(200));
        let content_type = response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()?
            .to_owned();
        if content_type.contains("javascript") {
            scripts += 1;
            assert_that!(response.bytes().await?.is_empty(), eq(false));
        } else if content_type.contains("text/css") {
            css += 1;
            let stylesheet = response.text().await?;
            for (expected, seen) in packaged_styles.iter().zip(&mut served_styles) {
                *seen |= expected.as_slice() == stylesheet.as_bytes();
            }
            for font in font_references.captures_iter(&stylesheet) {
                let font_url = url.join(&font[1])?;
                anyhow::ensure!(
                    font_url.host_str() == base.host_str(),
                    "unexpected remote font"
                );
                let response = app.client.get(font_url).send().await?;
                assert_that!(response.status().as_u16(), eq(200));
                assert_that!(response.bytes().await?.is_empty(), eq(false));
                fonts += 1;
            }
        }
    }
    assert_that!(packaged_styles.is_empty(), eq(false));
    assert_that!(served_styles.iter().all(|seen| *seen), eq(true));
    assert_that!(css, ge(2));
    assert_that!(scripts, ge(1));
    assert_that!(fonts, ge(1));
    Ok(())
}

async fn rejected(app: &Fixture, origin: &str) -> anyhow::Result<()> {
    let mut child = Process(
        Command::new(app.package.path().join("book-smartz"))
            .arg("serve")
            .env("DATABASE_URL", &app.database)
            .env("PORT", "0")
            .env("HOST", "127.0.0.1")
            .env("BOOK_SMARTZ_ORIGIN", origin)
            .env("GOOGLE_CLIENT_ID", "fixture-client")
            .env("GOOGLE_CLIENT_SECRET", "fixture-secret")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let status = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Some(status) = child.0.try_wait()? {
                return Ok::<_, std::io::Error>(status);
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    assert_that!(status.success(), eq(false));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn startup_rejects_missing_mismatched_assets_and_incomplete_schema() -> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    app.process.0.kill()?;
    app.process.0.wait()?;
    rejected(&app, "http://reader.example")
        .await
        .context("startup must reject a non-HTTPS canonical origin")?;
    let integrity: serde_json::Value =
        serde_json::from_slice(&fs::read(app.package.path().join("integrity.json"))?)?;
    let name = integrity["files"]
        .as_object()
        .unwrap()
        .keys()
        .find(|name| name.ends_with(".css"))
        .unwrap();
    let path = app.package.path().join("assets").join(name);
    let original = fs::read(&path)?;
    fs::write(&path, b"mismatched release")?;
    rejected(&app, "https://reader.example")
        .await
        .context("startup must reject mismatched assets")?;
    fs::write(&path, original)?;
    let manifest = app.package.path().join("assets/manifest.toml");
    let original = fs::read(&manifest)?;
    fs::remove_file(&manifest)?;
    rejected(&app, "https://reader.example")
        .await
        .context("startup must reject missing manifest")?;
    fs::write(&manifest, original)?;
    sqlx::query("DROP TABLE book_smartz_web.sessions")
        .execute(&app.pool)
        .await?;
    rejected(&app, "https://reader.example")
        .await
        .context("startup must reject incomplete schema")?;
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn launcher_forwards_explicit_migration_and_propagates_failure_without_erasing_sessions()
-> anyhow::Result<()> {
    let app = Fixture::new().await?;
    let cookie = app.login("migrating-reader").await?;
    let files = runfiles::Runfiles::create()?;
    let launcher =
        runfiles::rlocation!(files, std::env::var("RUN_LAUNCHER")?).context("package launcher")?;
    let run = || {
        Command::new(&launcher)
            .arg("migrate")
            .env("DATABASE_URL", &app.database)
            .current_dir(app.cwd.path())
            .status()
    };
    assert_that!(run()?.success(), eq(true));
    assert_that!(run()?.success(), eq(true));
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    sqlx::query("UPDATE book_smartz_web._sqlx_migrations SET checksum = decode('00', 'hex') WHERE version = 1").execute(&app.pool).await?;
    assert_that!(run()?.success(), eq(false));
    Ok(())
}
