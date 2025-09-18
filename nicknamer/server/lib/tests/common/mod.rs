#![allow(dead_code)]
use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use migration::MigratorTrait;
use nicknamer_server::auth::CurrentUser;
use sea_orm::{Database, DatabaseConnection};
use serde::Serialize;
use std::collections::BTreeMap;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::{postgres, testcontainers};

/// Headers that vary between test runs and should be filtered out for stable snapshots.
pub const VARIABLE_HEADERS: &[&str] = &[
    "date",
    "expires",
    "last-modified",
    "etag",
    "server",
    "x-request-id",
    "x-trace-id",
    "set-cookie",
    "content-length",
];

/// HTTP response snapshot for testing endpoints.
#[derive(Debug, Serialize)]
pub struct HttpResponseSnapshot {
    pub test_context: String,
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub html_body: Vec<String>,
}

impl HttpResponseSnapshot {
    /// Create a new HTTP response snapshot.
    pub fn new(
        body_text: &str,
        status: StatusCode,
        headers: &axum::http::HeaderMap,
        test_context: &str,
    ) -> Self {
        Self {
            test_context: test_context.to_string(),
            status: status.as_u16(),
            headers: filter_variable_headers(headers),
            html_body: normalize_html_for_snapshot(body_text),
        }
    }
}

/// Snapshot structure for JSON API responses
#[derive(serde::Serialize)]
pub struct JsonApiResponseSnapshot {
    status: u16,
    headers: std::collections::BTreeMap<String, String>,
    body: serde_json::Value,
    test_name: String,
}

impl JsonApiResponseSnapshot {
    pub fn new(
        body_text: &str,
        status: axum::http::StatusCode,
        headers: &axum::http::HeaderMap,
        test_name: &str,
    ) -> Self {
        let body = serde_json::from_str(body_text)
            .unwrap_or_else(|_| serde_json::Value::String(body_text.to_string()));

        Self {
            status: status.as_u16(),
            headers: filter_variable_headers(headers),
            body,
            test_name: test_name.to_string(),
        }
    }
}

/// Normalize HTML content for consistent snapshots by removing dynamic values.
pub fn normalize_html_for_snapshot(html: &str) -> Vec<String> {
    // Split HTML by newlines and convert to Vec<String>
    // In the future, we could add more sophisticated normalization
    html.lines().map(|line| line.to_string()).collect()
}

/// Filter out variable headers from response headers for snapshot testing.
pub fn filter_variable_headers(headers: &axum::http::HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            let name_str = name.as_str().to_lowercase();
            if VARIABLE_HEADERS.contains(&name_str.as_str()) {
                None
            } else {
                value.to_str().ok().map(|v| (name_str, v.to_string()))
            }
        })
        .collect()
}

pub async fn setup_container() -> anyhow::Result<testcontainers::ContainerAsync<postgres::Postgres>>
{
    let container = postgres::Postgres::default().start().await?;
    Ok(container)
}

pub async fn setup_db(
    container: &testcontainers::ContainerAsync<postgres::Postgres>,
) -> anyhow::Result<DatabaseConnection> {
    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let db_url = format!("postgres://postgres:postgres@{}:{}/postgres", host, port);
    let db = Database::connect(&db_url).await?;
    migration::Migrator::up(&db, None).await?;
    Ok(db)
}

/// Stub middleware that injects a logged-in user for testing.
/// This middleware always injects a CurrentUser with the specified username.
pub async fn stub_user_middleware(mut request: Request<Body>, next: Next) -> Response {
    // For tests, we inject a hardcoded user
    let current_user = CurrentUser::new("testuser".to_string());
    request.extensions_mut().insert(current_user);
    next.run(request).await
}
