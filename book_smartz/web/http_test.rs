//! Exercise the packaged production router, not a substitute HTTP implementation.
use std::{
    fs,
    process::{Child, Command, Stdio},
    time::Duration,
};

use googletest::{assert_that, matchers::eq};
use runfiles::{Runfiles, rlocation};
use testcontainers_modules::testcontainers::runners::AsyncRunner;

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[googletest::test]
#[tokio::test]
async fn protected_ranking_requires_a_stored_browser_session() -> anyhow::Result<()> {
    let postgres = test_images::postgres().await.start().await?;
    let database = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        postgres.get_host().await?,
        postgres.get_host_port_ipv4(5432).await?
    );
    let files = Runfiles::create()?;
    let archive = rlocation!(files, std::env::var("PACKAGE_ARCHIVE")?).unwrap();
    let package = tempfile::tempdir()?;
    tar::Archive::new(fs::File::open(archive)?).unpack(package.path())?;
    let cwd = tempfile::tempdir()?;
    let binary = package.path().join("book-smartz");
    let status = Command::new(&binary)
        .arg("migrate")
        .env("DATABASE_URL", &database)
        .current_dir(cwd.path())
        .status()?;
    assert_that!(status.success(), eq(true));
    let socket = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    drop(socket);
    let mut process = Process(
        Command::new(&binary)
            .arg("serve")
            .env("DATABASE_URL", &database)
            .env("PORT", port.to_string())
            .env("BOOK_SMARTZ_ORIGIN", "https://reader.example")
            .env("GOOGLE_CLIENT_ID", "fixture-client")
            .env("GOOGLE_CLIENT_SECRET", "fixture-secret")
            .current_dir(cwd.path())
            .stdout(Stdio::null())
            .spawn()?,
    );
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let base = format!("http://127.0.0.1:{port}");
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(status) = process.0.try_wait()? {
                anyhow::bail!("application exited {status}");
            }
            if client
                .get(format!("{base}/health/ready"))
                .send()
                .await
                .is_ok()
            {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::task::yield_now().await;
        }
    })
    .await??;
    let response = client.get(format!("{base}/ranking")).send().await?;
    assert_that!(response.status().as_u16(), eq(401));
    for cookie in [
        "__Host-book-smartz-session=malformed",
        "__Host-book-smartz-session=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    ] {
        let response = client
            .get(format!("{base}/ranking"))
            .header("cookie", cookie)
            .send()
            .await?;
        assert_that!(response.status().as_u16(), eq(401));
    }
    let response = client.get(format!("{base}/logout")).send().await?;
    assert_that!(response.status().as_u16(), eq(405));
    Ok(())
}

mod identity_test;
mod test_support;
