//! Package and HTTP evidence through real Topcoat and isolated PostgreSQL.
use std::{
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use googletest::{
    assert_that,
    matchers::{contains_substring, eq},
};
use reqwest::{Client, StatusCode};
use runfiles::{Runfiles, rlocation};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use uuid::Uuid;

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn start(
    root: &Path,
    cwd: &Path,
    url: &str,
    reader: Uuid,
    client: &Client,
) -> (Server, String) {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let log = fs::File::create(cwd.join(format!("app-{port}.log"))).unwrap();
    let child = Command::new(root.join("app"))
        .current_dir(cwd)
        .env("DATABASE_URL", url)
        .env("FIXTURE_READER_ID", reader.to_string())
        .env("HOST", "127.0.0.1")
        .env("PORT", port.to_string())
        .env_remove("BUILD_WORKSPACE_DIRECTORY")
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn()
        .unwrap();
    let mut server = Server(child);
    let base = format!("http://127.0.0.1:{port}");
    for _ in 0..200 {
        if let Ok(response) = client.get(&base).send().await {
            assert_that!(response.status(), eq(StatusCode::UNAUTHORIZED));
            return (server, base);
        }
        if let Some(status) = server.0.try_wait().unwrap() {
            panic!(
                "packaged application exited {status}: {}",
                fs::read_to_string(cwd.join(format!("app-{port}.log"))).unwrap()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("packaged application did not start");
}

fn input(name: &str) -> PathBuf {
    let runfiles = Runfiles::create().unwrap();
    rlocation!(runfiles, std::env::var(name).unwrap()).unwrap()
}

#[googletest::test]
#[tokio::test]
async fn packaged_http_and_session_survive_restart() {
    let container = test_images::postgres().await.start().await.unwrap();
    let url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    );
    let reader = Uuid::new_v4();
    let package = tempfile::tempdir().unwrap();
    tar::Archive::new(fs::File::open(input("PACKAGE_ARCHIVE")).unwrap())
        .unpack(package.path())
        .unwrap();
    let cwd = tempfile::tempdir().unwrap();
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let (server, base) = start(package.path(), cwd.path(), &url, reader, &client).await;
    let login = client
        .post(format!("{base}/fixture-login"))
        .send()
        .await
        .unwrap();
    assert_that!(login.status(), eq(StatusCode::OK));
    let cookie = login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert_that!(login.text().await.unwrap(), eq(&reader.to_string()));
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    let primary_hash: Vec<u8> =
        sqlx::query_scalar("SELECT token_hash FROM topcoat_fixture_sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
    let other_login = client
        .post(format!("{base}/fixture-login"))
        .send()
        .await
        .unwrap();
    assert_that!(other_login.status(), eq(StatusCode::OK));
    let orphaned_cookie = other_login.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let deleted = sqlx::query("DELETE FROM topcoat_fixture_sessions WHERE token_hash <> $1")
        .bind(primary_hash)
        .execute(&pool)
        .await
        .unwrap();
    assert_that!(deleted.rows_affected(), eq(1));
    for credential in [
        None,
        Some("__Host-session=invalid"),
        Some(orphaned_cookie.as_str()),
    ] {
        for (path, body) in [
            ("", None),
            ("/protected-procedure", Some("[]")),
            ("/protected-shard", Some(r#"{"args":[],"signals":{}}"#)),
        ] {
            let mut request = body.map_or_else(
                || client.get(format!("{base}{path}")),
                |body| {
                    client
                        .post(format!("{base}{path}"))
                        .header("content-type", "application/json")
                        .body(body)
                },
            );
            if let Some(value) = credential {
                request = request.header("cookie", value);
            }
            assert_that!(
                request.send().await.unwrap().status(),
                eq(StatusCode::UNAUTHORIZED)
            );
        }
    }
    let page = client
        .get(&base)
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_that!(page.status(), eq(StatusCode::OK));
    let html = page.text().await.unwrap();
    assert_that!(&html, contains_substring(reader.to_string()));
    assert_that!(&html, contains_substring("<svg"));
    assert_that!(&html, contains_substring("Fixture feather"));
    let mut stylesheet = false;
    let mut font = false;
    let mut runtime = false;
    let mut library = false;
    let mut urls: Vec<String> = regex::Regex::new(r#"(?:href|src)="([^"]+)""#)
        .unwrap()
        .captures_iter(&html)
        .map(|c| c[1].to_owned())
        .collect();
    let font_urls = regex::Regex::new(r#"url\(["']?([^"'()]+)["']?\)"#).unwrap();
    while let Some(path) = urls.pop() {
        assert_that!(path.starts_with('/'), eq(true));
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert_that!(response.status(), eq(StatusCode::OK));
        let bytes = response.bytes().await.unwrap();
        let extension = Path::new(&path)
            .extension()
            .and_then(|value| value.to_str());
        if extension == Some("css") {
            let css = std::str::from_utf8(&bytes).unwrap();
            stylesheet |= css.contains(
                &std::env::var("EXPECT_STYLE_CLASS")
                    .unwrap_or_else(|_| "text-emerald-700".to_owned()),
            );
            urls.extend(font_urls.captures_iter(css).map(|c| c[1].to_owned()));
        }
        if extension == Some("woff2") {
            assert_that!(&bytes[..4], eq(b"wOF2"));
            font = true;
        }
        if extension == Some("js") {
            assert_that!(bytes.len() > 1000, eq(true));
            runtime = true;
        }
        if extension == Some("txt") {
            assert_that!(
                std::str::from_utf8(&bytes).unwrap(),
                contains_substring(format!(
                    "Topcoat declared library asset v{}",
                    std::env::var("EXPECT_ASSET_REVISION").unwrap_or_else(|_| "1".to_owned())
                ))
            );
            library = true;
        }
    }
    assert_that!(
        (stylesheet, font, runtime, library),
        eq((true, true, true, true))
    );
    for (path, body) in [
        ("/protected-procedure", "[]"),
        ("/protected-shard", r#"{"args":[],"signals":{}}"#),
    ] {
        let response = client
            .post(format!("{base}{path}"))
            .header("cookie", &cookie)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap();
        assert_that!(response.status(), eq(StatusCode::OK));
        assert_that!(
            response.text().await.unwrap(),
            contains_substring(reader.to_string())
        );
    }
    let stored: Uuid = sqlx::query_scalar("SELECT reader_id FROM topcoat_fixture_sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_that!(stored, eq(reader));
    drop(server);
    let (_restarted, base) = start(package.path(), cwd.path(), &url, Uuid::new_v4(), &client).await;
    let restored = client
        .get(base)
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert_that!(restored.status(), eq(StatusCode::OK));
    assert_that!(
        restored.text().await.unwrap(),
        contains_substring(reader.to_string())
    );
}

#[googletest::test]
fn formatter_checks_and_converges() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("example.rs");
    fs::write(
        &file,
        "fn main( ){let x=1;}\nfn f(){let _=view! {<p>\"Macro\"</p>};}\n",
    )
    .unwrap();
    let format = input("FORMAT_TOOL");
    assert_that!(
        Command::new(&format)
            .arg("--check")
            .arg(&file)
            .status()
            .unwrap()
            .success(),
        eq(false)
    );
    assert_that!(
        Command::new(&format).arg(&file).status().unwrap().success(),
        eq(true)
    );
    let once = fs::read(&file).unwrap();
    assert_that!(
        Command::new(&format).arg(&file).status().unwrap().success(),
        eq(true)
    );
    assert_that!(fs::read(&file).unwrap(), eq(&once));
    assert_that!(
        Command::new(&format)
            .arg("--check")
            .arg(&file)
            .status()
            .unwrap()
            .success(),
        eq(true)
    );
}
