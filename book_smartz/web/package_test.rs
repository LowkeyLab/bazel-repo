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
        .filter(|name| std::path::Path::new(name).extension() == Some(std::ffi::OsStr::new("css")))
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
        if content_type.contains("javascript") || content_type.contains("text/css") {
            let cache = response
                .headers()
                .get_all("cache-control")
                .iter()
                .map(|value| value.to_str().unwrap())
                .collect::<Vec<_>>()
                .join(",");
            assert_that!(cache.as_str(), contains_substring("immutable"));
            assert_that!(cache.as_str(), not(contains_substring("no-store")));
        }
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
            .env("BOOK_SMARTZ_MAINTAINER_CONTACT", "maintainer@example.test")
            .env(
                "BOOK_SMARTZ_SELECTION_KEY",
                "public-fixture-key-at-least-thirty-two-bytes",
            )
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
        .find(|name| std::path::Path::new(name).extension() == Some(std::ffi::OsStr::new("css")))
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

#[googletest::test]
#[tokio::test]
async fn concurrent_fresh_migrations_wait_for_the_schema_lock() -> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    app.process.0.kill()?;
    app.process.0.wait()?;
    sqlx::query("DROP SCHEMA book_smartz_web CASCADE")
        .execute(&app.pool)
        .await?;
    let mut lock = app.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(706165241, 2)")
        .execute(&mut *lock)
        .await?;
    let spawn = || -> anyhow::Result<Process> {
        Ok(Process(
            Command::new(app.package.path().join("book-smartz"))
                .arg("migrate")
                .env("DATABASE_URL", &app.database)
                .current_dir(app.cwd.path())
                .stdout(Stdio::null())
                .spawn()?,
        ))
    };
    let mut first = spawn()?;
    let mut second = spawn()?;
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            anyhow::ensure!(first.0.try_wait()?.is_none() && second.0.try_wait()?.is_none(), "migration bypassed the schema lock");
            let waiting: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND classid = 706165241 AND objid = 2 AND NOT granted").fetch_one(&app.pool).await?;
            if waiting == 2 { return Ok::<_, anyhow::Error>(()); }
            tokio::task::yield_now().await;
        }
    }).await??;
    lock.commit().await?;
    assert_that!(first.0.wait()?.success(), eq(true));
    assert_that!(second.0.wait()?.success(), eq(true));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn readiness_rechecks_assets_without_contacting_providers() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    app.provider.reset().await;
    let ready = || app.client.get(format!("{}/health/ready", app.base)).send();
    assert_that!(ready().await?.status().as_u16(), eq(200));
    let manifest = app.package.path().join("assets/manifest.toml");
    let original = fs::read(&manifest)?;
    for contents in [
        Some(b"mismatched release".as_slice()),
        Some(original.as_slice()),
        None,
    ] {
        if let Some(contents) = contents {
            fs::write(&manifest, contents)?;
        } else {
            fs::remove_file(&manifest)?;
        }
        let expected = if contents == Some(original.as_slice()) {
            200
        } else {
            503
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if ready().await?.status().as_u16() == expected {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await??;
        // Concurrent probes observe the same verification state.
        let mut probes = tokio::task::JoinSet::new();
        for _ in 0..32 {
            let client = app.client.clone();
            let url = format!("{}/health/ready", app.base);
            probes.spawn(async move { client.get(url).send().await });
        }
        while let Some(response) = probes.join_next().await {
            assert_that!(response??.status().as_u16(), eq(expected));
        }
    }
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn stalled_integrity_verification_expires_readiness_and_cannot_delay_shutdown()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let manifest = app.package.path().join("integrity.json");
    fs::remove_file(&manifest)?;
    assert_that!(
        Command::new("mkfifo").arg(&manifest).status()?.success(),
        eq(true)
    );
    // The verifier blocks opening this FIFO. Readiness must remain responsive
    // and expire its earlier success without starting another verifier.
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            if app
                .client
                .get(format!("{}/health/ready", app.base))
                .send()
                .await?
                .status()
                .as_u16()
                == 503
            {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .context("stalled verifier must expire readiness")??;
    assert_that!(
        Command::new("kill")
            .args(["-TERM", &app.process.0.id().to_string()])
            .status()?
            .success(),
        eq(true)
    );
    let status = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(status) = app.process.0.try_wait()? {
                return Ok::<_, anyhow::Error>(status);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .context("stalled verifier must not delay process exit")??;
    assert_that!(status.success(), eq(true));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn graceful_shutdown_drains_an_admitted_request_and_preserves_the_session()
-> anyhow::Result<()> {
    let mut app = Fixture::new().await?;
    let cookie = app.login("shutdown-reader").await?;
    let mut lock = app.pool.begin().await?;
    sqlx::query("LOCK book_smartz_web.sessions IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await?;
    let request = {
        let client = app.client.clone();
        let base = app.base.clone();
        let cookie = cookie.clone();
        tokio::spawn(async move {
            client
                .get(format!("{base}/ranking"))
                .header("cookie", cookie)
                .send()
                .await
        })
    };
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid <> pg_backend_pid() AND wait_event_type='Lock' AND query LIKE 'SELECT reader_id, csrf%')").fetch_one(&app.pool).await?;
            if waiting { return Ok::<_, anyhow::Error>(()); }
            tokio::task::yield_now().await;
        }
    }).await??;
    let start = std::time::Instant::now();
    assert_that!(
        Command::new("kill")
            .args(["-TERM", &app.process.0.id().to_string()])
            .status()?
            .success(),
        eq(true)
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while std::net::TcpStream::connect(app.base.trim_start_matches("http://")).is_ok() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_that!(app.process.0.try_wait()?.is_none(), eq(true));
    lock.commit().await?;
    assert_that!(request.await??.status().as_u16(), eq(200));
    let status = tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            if let Some(status) = app.process.0.try_wait()? {
                return Ok::<_, anyhow::Error>(status);
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    assert_that!(status.success(), eq(true));
    assert_that!(start.elapsed() < Duration::from_secs(30), eq(true));
    // Restart the package without another migration or login.
    app.restart_after_exit().await?;
    assert_that!(app.ranking(&cookie).await?.status().as_u16(), eq(200));
    Ok(())
}

#[googletest::test]
#[tokio::test]
async fn serving_connections_have_bounded_pool_and_database_timeouts() -> anyhow::Result<()> {
    let app = Fixture::new().await?;
    sqlx::raw_sql("CREATE TABLE public.runtime_settings(lock_timeout text, statement_timeout text); CREATE FUNCTION public.capture_settings() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN INSERT INTO public.runtime_settings VALUES(current_setting('lock_timeout'), current_setting('statement_timeout')); RETURN NEW; END $$; CREATE TRIGGER capture_settings AFTER INSERT ON book_smartz_web.sessions FOR EACH ROW EXECUTE FUNCTION public.capture_settings();").execute(&app.pool).await?;
    let cookie = app.login("pool-reader").await?;
    let settings: (String, String) =
        sqlx::query_as("SELECT lock_timeout, statement_timeout FROM public.runtime_settings")
            .fetch_one(&app.pool)
            .await?;
    assert_that!(settings.0, eq("5s"));
    assert_that!(settings.1, eq("15s"));
    let mut lock = app.pool.begin().await?;
    sqlx::query("LOCK book_smartz_web.sessions IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await?;
    let mut requests = Vec::new();
    for _ in 0..6 {
        let client = app.client.clone();
        let base = app.base.clone();
        let cookie = cookie.clone();
        requests.push(tokio::spawn(async move {
            client
                .get(format!("{base}/ranking"))
                .header("cookie", cookie)
                .send()
                .await
        }));
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let waiting: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE application_name='book-smartz' AND wait_event_type='Lock'").fetch_one(&app.pool).await?;
            if waiting == 5 { return Ok::<_, anyhow::Error>(()); }
            tokio::task::yield_now().await;
        }
    }).await??;
    let connected: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_stat_activity WHERE application_name='book-smartz'",
    )
    .fetch_one(&app.pool)
    .await?;
    assert_that!(connected, eq(5));
    lock.commit().await?;
    for request in requests {
        assert_that!(request.await??.status().as_u16(), eq(200));
    }
    Ok(())
}
