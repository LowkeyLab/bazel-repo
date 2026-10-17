//! Google identity verification and durable browser authentication.
use std::time::{Duration, Instant};

use base64::Engine;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{
    PgPool, SqlSafeStr,
    migrate::{Migration, MigrationType, Migrator},
};
use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::{Cookie, Cookies, SameSite, cookies},
    router::{
        content::Form,
        error::{see_other, unauthorized},
        route,
    },
    session::{self, Token, TokenStore, TokenStoreFuture},
};
use uuid::Uuid;

use crate::Application;

const STATE_COOKIE: &str = "__Host-book-smartz-login";
const SESSION_COOKIE: &str = "__Host-book-smartz-session";
const ISSUER: &str = "https://accounts.google.com";

/// Uses Topcoat's opaque token mechanics with explicit hardened cookie attributes.
pub struct BrowserTokens;
impl TokenStore for BrowserTokens {
    fn read<'a>(&'a self, cx: &'a Cx) -> TokenStoreFuture<'a, Option<Token>> {
        Box::pin(async move {
            Ok(cookies(cx)
                .get(SESSION_COOKIE)
                .and_then(|cookie| Token::decode(cookie.value()).ok()))
        })
    }
    fn write<'a>(&'a self, cx: &'a Cx, token: Token, age: Duration) -> TokenStoreFuture<'a, ()> {
        Box::pin(async move {
            cookies(cx).add(
                Cookie::build((SESSION_COOKIE, token.encode()))
                    .secure(true)
                    .http_only(true)
                    .same_site(SameSite::Lax)
                    .path("/")
                    .max_age(topcoat::cookie::time::Duration::try_from(age)?)
                    .build(),
            );
            Ok(())
        })
    }
    fn delete<'a>(&'a self, cx: &'a Cx) -> TokenStoreFuture<'a, ()> {
        Box::pin(async move {
            cookies(cx).remove(
                Cookie::build((SESSION_COOKIE, ""))
                    .path("/")
                    .secure(true)
                    .http_only(true)
                    .same_site(SameSite::Lax)
                    .build(),
            );
            Ok(())
        })
    }
}

pub struct Google {
    client: reqwest::Client,
    client_id: String,
    client_secret: String,
    redirect: String,
    authorization: url::Url,
    token: url::Url,
    keys: url::Url,
    cached_keys: tokio::sync::Mutex<Option<CachedKeys>>,
}
struct CachedKeys {
    keys: JwkSet,
    expires: Instant,
}
impl Google {
    pub fn from_environment(origin: &str) -> anyhow::Result<Self> {
        fn endpoint(key: &str, default: &str) -> anyhow::Result<url::Url> {
            let url = url::Url::parse(&std::env::var(key).unwrap_or_else(|_| default.into()))?;
            anyhow::ensure!(
                url.scheme() == "https"
                    || (url.scheme() == "http"
                        && matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))),
                "OIDC endpoint must use HTTPS or an explicit loopback fixture"
            );
            anyhow::ensure!(
                url.username().is_empty() && url.password().is_none() && url.fragment().is_none(),
                "invalid OIDC endpoint"
            );
            Ok(url)
        }
        let client_id = std::env::var("GOOGLE_CLIENT_ID")?;
        let client_secret = std::env::var("GOOGLE_CLIENT_SECRET")?;
        anyhow::ensure!(
            !client_id.is_empty() && !client_secret.is_empty(),
            "Google client configuration is empty"
        );
        Ok(Self {
            cached_keys: tokio::sync::Mutex::new(None),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            client_id,
            client_secret,
            redirect: format!("{origin}/auth/google/callback"),
            authorization: endpoint(
                "GOOGLE_AUTHORIZATION_ENDPOINT",
                "https://accounts.google.com/o/oauth2/v2/auth",
            )?,
            token: endpoint(
                "GOOGLE_TOKEN_ENDPOINT",
                "https://oauth2.googleapis.com/token",
            )?,
            keys: endpoint(
                "GOOGLE_JWKS_ENDPOINT",
                "https://www.googleapis.com/oauth2/v3/certs",
            )?,
        })
    }
    async fn key(&self, kid: &str) -> anyhow::Result<DecodingKey> {
        let mut cached = self.cached_keys.lock().await;
        if let Some(cached) = cached.as_ref()
            && cached.expires > Instant::now()
            && let Some(key) = cached.keys.find(kid)
        {
            return Ok(DecodingKey::from_jwk(key)?);
        }
        // Serialize refreshes; an unknown kid triggers rotation discovery even
        // during a valid cache lifetime. Failed refreshes never extend that lifetime.
        let response = self.client.get(self.keys.clone()).send().await?;
        let policy = response
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let directives: Vec<_> = policy.split(',').map(str::trim).collect();
        let seconds = if directives.iter().any(|part| {
            part.eq_ignore_ascii_case("no-store") || part.eq_ignore_ascii_case("no-cache")
        }) {
            0
        } else {
            directives
                .iter()
                .find_map(|part| {
                    part.split_once('=')
                        .filter(|(key, _)| key.trim().eq_ignore_ascii_case("max-age"))
                        .and_then(|(_, value)| value.trim().trim_matches('"').parse::<u64>().ok())
                })
                .unwrap_or(0)
        };
        let age = response
            .headers()
            .get("age")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let expires = Instant::now() + Duration::from_secs(seconds.saturating_sub(age).min(86_400));
        let keys: JwkSet = bounded(response).await?;
        let key = keys.find(kid).map(DecodingKey::from_jwk).transpose()?;
        *cached = Some(CachedKeys { keys, expires });
        drop(cached);
        key.ok_or_else(|| anyhow::anyhow!("unknown identity key"))
    }
    async fn claims(&self, code: &str, verifier: &str, nonce: &str) -> anyhow::Result<Claims> {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs([
                ("code", code),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("redirect_uri", self.redirect.as_str()),
                ("grant_type", "authorization_code"),
                ("code_verifier", verifier),
            ])
            .finish();
        let tokens: Tokens = bounded(
            self.client
                .post(self.token.clone())
                .header("content-type", "application/x-www-form-urlencoded")
                .body(body)
                .send()
                .await?,
        )
        .await?;
        let header = decode_header(&tokens.id_token)?;
        anyhow::ensure!(
            header.alg == Algorithm::RS256,
            "unaccepted identity algorithm"
        );
        let key = self
            .key(
                header
                    .kid
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("missing identity key"))?,
            )
            .await?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = 0;
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.set_audience(&[&self.client_id]);
        validation.set_issuer(&[ISSUER, "accounts.google.com"]);
        let claims = decode::<Claims>(&tokens.id_token, &key, &validation)?.claims;
        anyhow::ensure!(
            !claims.sub.is_empty() && claims.sub.len() <= 255 && claims.nonce == nonce,
            "invalid identity claims"
        );
        let multiple_audiences = claims
            .aud
            .as_array()
            .is_some_and(|audiences| audiences.len() > 1);
        anyhow::ensure!(
            claims
                .azp
                .as_deref()
                .is_none_or(|presenter| presenter == self.client_id)
                && (!multiple_audiences || claims.azp.as_deref() == Some(self.client_id.as_str())),
            "invalid identity presenter"
        );
        Ok(claims)
    }
}

async fn bounded<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> anyhow::Result<T> {
    let mut response = response.error_for_status()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes.len() + chunk.len() <= 1_048_576,
            "identity response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice(&bytes)?)
}
#[derive(Deserialize)]
struct Tokens {
    id_token: String,
}
#[derive(Clone, Deserialize)]
struct Claims {
    sub: String,
    nonce: String,
    aud: serde_json::Value,
    azp: Option<String>,
}
#[derive(Deserialize)]
pub struct Callback {
    code: String,
    state: String,
}

#[route(GET "/auth/google")]
pub async fn login(cx: &Cx) -> Result<()> {
    let app: &Application = app_context(cx);
    let state = Token::random();
    let nonce = Token::random().encode();
    let verifier = Token::random().encode().trim_end_matches('=').to_owned();
    let mut transaction = app.pool.begin().await?;
    sqlx::query("DELETE FROM book_smartz_web.login_challenges WHERE expires_at <= now()")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("DELETE FROM book_smartz_web.sessions WHERE expires_at <= now()")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("INSERT INTO book_smartz_web.login_challenges (state_hash, nonce, verifier, expires_at) VALUES ($1, $2, $3, now() + interval '10 minutes')")
        .bind(state.hash().as_slice()).bind(&nonce).bind(&verifier).execute(&mut *transaction).await?;
    transaction.commit().await?;
    cookies(cx).add(
        Cookie::build((STATE_COOKIE, state.encode()))
            .secure(true)
            .http_only(true)
            .same_site(SameSite::Lax)
            .path("/")
            .max_age(topcoat::cookie::time::Duration::minutes(10))
            .build(),
    );
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    let mut destination = app.google.authorization.clone();
    destination.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("client_id", app.google.client_id.as_str()),
        ("redirect_uri", app.google.redirect.as_str()),
        ("scope", "openid"),
        ("state", state.encode().as_str()),
        ("nonce", nonce.as_str()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ]);
    Err(see_other(destination.as_str()).into())
}

#[route(GET "/auth/google/callback")]
pub async fn callback(cx: &Cx, Form(input): Form<Callback>) -> Result<()> {
    let state = Token::decode(&input.state).map_err(|_| unauthorized())?;
    let cookie = cookies(cx).get(STATE_COOKIE).ok_or_else(unauthorized)?;
    let browser = Token::decode(cookie.value()).map_err(|_| unauthorized())?;
    if browser.hash() != state.hash() {
        return Err(unauthorized().into());
    }
    let app: &Application = app_context(cx);
    let challenge: Option<(String, String)> = sqlx::query_as("DELETE FROM book_smartz_web.login_challenges WHERE state_hash = $1 AND expires_at > now() RETURNING nonce, verifier")
        .bind(state.hash().as_slice()).fetch_optional(&app.pool).await?;
    let (nonce, verifier) = challenge.ok_or_else(unauthorized)?;
    let claims = app
        .google
        .claims(&input.code, &verifier, &nonce)
        .await
        .map_err(|_| unauthorized())?;
    let issued = session::start(cx).await?;
    let csrf = Token::random().encode();
    let mut transaction = app.pool.begin().await?;
    let reader: Uuid = sqlx::query_scalar("INSERT INTO book_smartz_web.accounts (issuer, subject, reader_id) VALUES ($1, $2, $3) ON CONFLICT (issuer, subject) DO UPDATE SET subject = EXCLUDED.subject RETURNING reader_id")
        .bind(ISSUER).bind(claims.sub).bind(Uuid::new_v4()).fetch_one(&mut *transaction).await?;
    let expires: chrono::DateTime<chrono::Utc> = issued.expires_at.into();
    sqlx::query("INSERT INTO book_smartz_web.sessions (token_hash, reader_id, csrf, expires_at) VALUES ($1, $2, $3, $4)")
        .bind(issued.token_hash.as_slice()).bind(reader).bind(csrf).bind(expires).execute(&mut *transaction).await?;
    transaction.commit().await?;
    cookies(cx).remove(
        Cookie::build((STATE_COOKIE, ""))
            .path("/")
            .secure(true)
            .build(),
    );
    Err(see_other("/ranking").into())
}

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    let mut connection = pool.acquire().await?;
    connection.close_on_drop();
    sqlx::query("BEGIN").execute(&mut *connection).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(706165241, 2)")
        .execute(&mut *connection)
        .await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS book_smartz_web")
        .execute(&mut *connection)
        .await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS book_smartz_catalog")
        .execute(&mut *connection)
        .await?;
    sqlx::query("COMMIT").execute(&mut *connection).await?;
    sqlx::query("SET search_path TO book_smartz_web, pg_catalog")
        .execute(&mut *connection)
        .await?;
    Migrator::with_migrations(vec![Migration::new(
        1,
        "browser identity and sessions".into(),
        MigrationType::Simple,
        include_str!("identity.sql").into_sql_str(),
        false,
    )])
    .run(&mut *connection)
    .await?;
    // Keep the identity ledger at version 1 so the previous executable remains
    // ready and restartable during catalog rollout or rollback.
    sqlx::query("SET search_path TO book_smartz_catalog, pg_catalog")
        .execute(&mut *connection)
        .await?;
    Migrator::with_migrations(vec![Migration::new(
        1,
        "catalog admission".into(),
        MigrationType::Simple,
        include_str!("catalog.sql").into_sql_str(),
        false,
    )])
    .run(&mut *connection)
    .await?;
    crate::actions::migrate(pool).await?;
    Ok(())
}

pub async fn ready(pool: &PgPool) -> anyhow::Result<()> {
    crate::actions::ready(pool).await?;
    let versions: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT max(version) FROM book_smartz._sqlx_migrations WHERE success), (SELECT max(version) FROM book_smartz_web._sqlx_migrations WHERE success), (SELECT max(version) FROM book_smartz_catalog._sqlx_migrations WHERE success)").fetch_one(pool).await?;
    anyhow::ensure!(versions == (2, 1, 1), "incompatible schema");
    sqlx::query("SELECT a.issuer, a.subject, a.reader_id, s.token_hash, s.reader_id, s.csrf, s.expires_at, c.state_hash, c.nonce, c.verifier, c.expires_at FROM book_smartz_web.accounts a CROSS JOIN book_smartz_web.sessions s CROSS JOIN book_smartz_web.login_challenges c LIMIT 0").execute(pool).await?;
    sqlx::query("SELECT g.singleton, g.next_search_at, a.reader_id, a.admitted_at FROM book_smartz_catalog.search_gate g CROSS JOIN book_smartz_catalog.search_admissions a LIMIT 0").execute(pool).await?;
    let gate: bool = sqlx::query_scalar(
        "SELECT count(*) = 1 FROM book_smartz_catalog.search_gate WHERE singleton",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(gate, "catalog admission gate is missing");
    sqlx::query("SELECT b.id, b.work_id, r.reader_id, e.sequence, e.envelope FROM book_smartz.books b CROSS JOIN book_smartz.reader_streams r CROSS JOIN book_smartz.ranking_events e LIMIT 0").execute(pool).await?;
    Ok(())
}
