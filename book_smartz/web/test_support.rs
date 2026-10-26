//! Real PostgreSQL and packaged application with a controlled OIDC provider.
use std::{
    fs,
    process::{Child, Command, Stdio},
    time::Duration,
};

use jsonwebtoken::{Algorithm, EncodingKey, Header, jwk::Jwk};
use reqwest::{Client, Response};
use runfiles::{Runfiles, rlocation};
use serde_json::{Value, json};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, runners::AsyncRunner},
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

pub struct Process(pub Child);
impl Drop for Process {
    fn drop(&mut self) {
        // Normal teardown lets the server drain and LLVM write child profiles.
        // Explicit cancellation tests retain their direct kill operations.
        if self.0.try_wait().ok().flatten().is_some() {
            return;
        }
        let _ = Command::new("kill")
            .args(["-TERM", &self.0.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        for _ in 0..200 {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
pub struct Fixture {
    pub process: Process,
    pub pool: sqlx::PgPool,
    pub provider: MockServer,
    pub client: Client,
    pub base: String,
    pub package: tempfile::TempDir,
    pub cwd: tempfile::TempDir,
    pub database: String,
    pub diagnostics: tempfile::NamedTempFile,
    port: u16,
    _postgres: ContainerAsync<Postgres>,
}
pub struct Login {
    pub state: String,
    pub nonce: String,
    pub cookie: String,
    pub challenge: String,
}

impl Fixture {
    pub async fn new() -> anyhow::Result<Self> {
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
        let status = Command::new(package.path().join("book-smartz"))
            .arg("migrate")
            .env("DATABASE_URL", &database)
            .current_dir(cwd.path())
            .status()?;
        anyhow::ensure!(status.success(), "migration failed");
        let socket = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = socket.local_addr()?.port();
        drop(socket);
        let provider = MockServer::start().await;
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let diagnostics = tempfile::NamedTempFile::new()?;
        let process = Self::spawn(
            &package,
            &cwd,
            &database,
            port,
            &provider,
            diagnostics.as_file(),
        )?;
        let mut fixture = Self {
            process,
            pool: sqlx::PgPool::connect(&database).await?,
            provider,
            client,
            base: format!("http://127.0.0.1:{port}"),
            package,
            cwd,
            database,
            diagnostics,
            port,
            _postgres: postgres,
        };
        fixture.wait().await?;
        fixture
            .install_keys("fixture", "public, max-age=3600")
            .await?;
        Ok(fixture)
    }
    pub async fn install_keys(&self, kid: &str, policy: &str) -> anyhow::Result<()> {
        let key = EncodingKey::from_rsa_pem(include_bytes!("fixtures/public-test-key.pem"))?;
        let mut jwk = Jwk::from_encoding_key(&key, Algorithm::RS256)?;
        jwk.common.key_id = Some(kid.to_owned());
        Mock::given(method("GET"))
            .and(path("/keys"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("cache-control", policy.to_owned())
                    .set_body_json(json!({"keys": [jwk]})),
            )
            .mount(&self.provider)
            .await;
        Ok(())
    }
    fn spawn(
        package: &tempfile::TempDir,
        cwd: &tempfile::TempDir,
        database: &str,
        port: u16,
        provider: &MockServer,
        diagnostics: &std::fs::File,
    ) -> anyhow::Result<Process> {
        Ok(Process(
            Command::new(package.path().join("book-smartz"))
                .arg("serve")
                .env("DATABASE_URL", database)
                .env("PORT", port.to_string())
                .env("BOOK_SMARTZ_ORIGIN", "https://reader.example")
                .env(
                    "OPEN_LIBRARY_SEARCH_ENDPOINT",
                    format!("{}/search.json", provider.uri()),
                )
                .env("BOOK_SMARTZ_MAINTAINER_CONTACT", "maintainer@example.test")
                .env(
                    "BOOK_SMARTZ_SELECTION_KEY",
                    "public-fixture-key-at-least-thirty-two-bytes",
                )
                .env("GOOGLE_CLIENT_ID", "fixture-client")
                .env("GOOGLE_CLIENT_SECRET", "fixture-secret")
                .env(
                    "GOOGLE_AUTHORIZATION_ENDPOINT",
                    format!("{}/authorize", provider.uri()),
                )
                .env("GOOGLE_TOKEN_ENDPOINT", format!("{}/token", provider.uri()))
                .env("GOOGLE_JWKS_ENDPOINT", format!("{}/keys", provider.uri()))
                .current_dir(cwd.path())
                .stdout(Stdio::null())
                .stderr(diagnostics.try_clone()?)
                .spawn()?,
        ))
    }
    async fn wait(&mut self) -> anyhow::Result<()> {
        Self::wait_for(&mut self.process, &self.client, &self.base).await
    }
    #[allow(
        dead_code,
        reason = "Used by the catalog suite in this shared test fixture."
    )]
    pub async fn another_instance(&self) -> anyhow::Result<(Process, String)> {
        let socket = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = socket.local_addr()?.port();
        drop(socket);
        let mut process = Self::spawn(
            &self.package,
            &self.cwd,
            &self.database,
            port,
            &self.provider,
            self.diagnostics.as_file(),
        )?;
        let base = format!("http://127.0.0.1:{port}");
        Self::wait_for(&mut process, &self.client, &base).await?;
        Ok((process, base))
    }
    async fn wait_for(process: &mut Process, client: &Client, base: &str) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                if let Some(status) = process.0.try_wait()? {
                    anyhow::bail!("application exited {status}");
                }
                if let Ok(response) = client.get(format!("{base}/health/ready")).send().await
                    && response.status().is_success()
                {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::task::yield_now().await;
            }
        })
        .await??;
        Ok(())
    }
    pub async fn restart(&mut self) -> anyhow::Result<()> {
        self.process.0.kill()?;
        self.process.0.wait()?;
        self.restart_after_exit().await
    }
    #[allow(
        dead_code,
        reason = "Used by package acceptance after graceful shutdown."
    )]
    pub async fn restart_after_exit(&mut self) -> anyhow::Result<()> {
        self.process = Self::spawn(
            &self.package,
            &self.cwd,
            &self.database,
            self.port,
            &self.provider,
            self.diagnostics.as_file(),
        )?;
        self.wait().await
    }
    #[allow(dead_code, reason = "Used by diagnostic failure acceptance.")]
    pub async fn break_diagnostic_delivery(&mut self) -> anyhow::Result<()> {
        self.process.0.kill()?;
        self.process.0.wait()?;
        self.process = Self::spawn(
            &self.package,
            &self.cwd,
            &self.database,
            self.port,
            &self.provider,
            &std::fs::File::options().write(true).open("/dev/full")?,
        )?;
        self.wait().await
    }
    #[allow(dead_code, reason = "Used by stalled diagnostic acceptance.")]
    pub async fn stall_diagnostic_delivery(
        &mut self,
    ) -> anyhow::Result<std::os::unix::net::UnixStream> {
        self.process.0.kill()?;
        self.process.0.wait()?;
        let (reader, writer) = std::os::unix::net::UnixStream::pair()?;
        let writer = std::fs::File::from(std::os::fd::OwnedFd::from(writer));
        self.process = Self::spawn(
            &self.package,
            &self.cwd,
            &self.database,
            self.port,
            &self.provider,
            &writer,
        )?;
        self.wait().await?;
        Ok(reader)
    }
    #[allow(dead_code, reason = "Used by diagnostic composition acceptance.")]
    pub async fn diagnostic(&self, category: &str, outcome: &str) -> anyhow::Result<Value> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                for line in fs::read_to_string(self.diagnostics.path())?.lines() {
                    let fact: Value = serde_json::from_str(line)?;
                    if fact["category"] == category && fact["outcome"] == outcome {
                        return Ok::<_, anyhow::Error>(fact);
                    }
                }
                tokio::task::yield_now().await;
            }
        })
        .await?
    }
    pub async fn begin(&self) -> anyhow::Result<Login> {
        let response = self
            .client
            .get(format!("{}/auth/google", self.base))
            .send()
            .await?;
        anyhow::ensure!(
            response.status().as_u16() == 303,
            "login not redirected: {}",
            response.status()
        );
        let cookie = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()?
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let destination = url::Url::parse(response.headers().get("location").unwrap().to_str()?)?;
        let parameters: std::collections::HashMap<_, _> =
            destination.query_pairs().into_owned().collect();
        Ok(Login {
            state: parameters["state"].clone(),
            nonce: parameters["nonce"].clone(),
            cookie,
            challenge: parameters["code_challenge"].clone(),
        })
    }
    pub fn claims(login: &Login, subject: &str) -> Value {
        json!({"sub": subject, "nonce": login.nonce, "iss": "https://accounts.google.com", "aud": "fixture-client", "azp": "fixture-client", "exp": chrono::Utc::now().timestamp()+300, "email": "before@example.test"})
    }
    pub async fn finish(&self, login: &Login, claims: Value) -> anyhow::Result<Response> {
        self.finish_with_kid(login, claims, "fixture").await
    }
    pub async fn finish_with_kid(
        &self,
        login: &Login,
        claims: Value,
        kid: &str,
    ) -> anyhow::Result<Response> {
        let key = EncodingKey::from_rsa_pem(include_bytes!("fixtures/public-test-key.pem"))?;
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_owned());
        let token = jsonwebtoken::encode(&header, &claims, &key)?;
        self.finish_token(login, &token).await
    }
    pub async fn finish_token(&self, login: &Login, token: &str) -> anyhow::Result<Response> {
        let code = uuid::Uuid::new_v4().to_string();
        let expected = login.challenge.clone();
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains(format!("code={code}")))
            .and(move |request: &wiremock::Request| {
                use base64::Engine;
                use sha2::Digest;
                let parameters: std::collections::HashMap<_, _> =
                    url::form_urlencoded::parse(&request.body)
                        .into_owned()
                        .collect();
                let Some(verifier) = parameters.get("code_verifier") else {
                    return false;
                };
                (43..=128).contains(&verifier.len())
                    && parameters.get("client_id").map(String::as_str) == Some("fixture-client")
                    && parameters.get("client_secret").map(String::as_str) == Some("fixture-secret")
                    && parameters.get("grant_type").map(String::as_str)
                        == Some("authorization_code")
                    && parameters.get("redirect_uri").map(String::as_str)
                        == Some("https://reader.example/auth/google/callback")
                    && verifier
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b))
                    && base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .encode(sha2::Sha256::digest(verifier.as_bytes()))
                        == expected
            })
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id_token": token})))
            .mount(&self.provider)
            .await;
        self.callback(login, &code, &login.state, &login.cookie)
            .await
    }
    pub async fn callback(
        &self,
        _: &Login,
        code: &str,
        state: &str,
        cookie: &str,
    ) -> anyhow::Result<Response> {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([("code", code), ("state", state)])
            .finish();
        Ok(self
            .client
            .get(format!("{}/auth/google/callback?{query}", self.base))
            .header("cookie", cookie)
            .send()
            .await?)
    }
    pub async fn login(&self, subject: &str) -> anyhow::Result<String> {
        let login = self.begin().await?;
        let response = self.finish(&login, Self::claims(&login, subject)).await?;
        anyhow::ensure!(
            response.status().as_u16() == 303,
            "callback failed: {}",
            response.status()
        );
        let value = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|v| v.starts_with("__Host-book-smartz-session="))
            .unwrap();
        anyhow::ensure!(
            value.contains("Secure")
                && value.contains("HttpOnly")
                && value.contains("SameSite=Lax")
                && value.contains("Max-Age=2592000")
                && value.contains("Path=/"),
            "session cookie policy"
        );
        Ok(value.split(';').next().unwrap().to_owned())
    }
    pub async fn ranking(&self, cookie: &str) -> anyhow::Result<Response> {
        Ok(self
            .client
            .get(format!("{}/ranking", self.base))
            .header("cookie", cookie)
            .send()
            .await?)
    }
}
