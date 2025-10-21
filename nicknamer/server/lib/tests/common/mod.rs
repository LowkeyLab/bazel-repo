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
use std::sync::OnceLock;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::{postgres, testcontainers};

/// Global container instance shared across all tests in a binary.
/// Initialized only once per test binary run using OnceLock.
static GLOBAL_CONTAINER: OnceLock<testcontainers::ContainerAsync<postgres::Postgres>> =
    OnceLock::new();

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

/// Initialize the global container once per test binary run.
/// This function is safe to call multiple times - the container will only be created once.
/// Uses a simple mutex-based approach to ensure single initialization.
async fn init_global_container() -> &'static testcontainers::ContainerAsync<postgres::Postgres> {
    // Try to get the existing container first (fast path)
    if let Some(container) = GLOBAL_CONTAINER.get() {
        return container;
    }

    // Slow path: need to initialize the container
    // Use a static mutex to ensure only one thread initializes
    use std::sync::Mutex;
    static INIT_LOCK: Mutex<()> = Mutex::new(());
    
    let _guard = INIT_LOCK.lock().unwrap();
    
    // Check again after acquiring the lock (another thread might have initialized)
    if let Some(container) = GLOBAL_CONTAINER.get() {
        return container;
    }

    // Create the container
    let container = postgres::Postgres::default()
        .start()
        .await
        .expect("Failed to start PostgreSQL container");
    
    // Set the container (should always succeed since we hold the lock)
    GLOBAL_CONTAINER
        .set(container)
        .expect("Failed to set global container");

    GLOBAL_CONTAINER
        .get()
        .expect("Container should be initialized")
}

/// Setup a new database connection using the shared global container.
/// Each test gets its own isolated database within the shared PostgreSQL container.
/// This approach provides test isolation while minimizing resource usage.
pub async fn setup_db_with_global_container() -> anyhow::Result<DatabaseConnection> {
    use std::sync::atomic::{AtomicU32, Ordering};
    use sea_orm::ConnectOptions;
    
    // Generate a unique database name for each test
    static DB_COUNTER: AtomicU32 = AtomicU32::new(0);
    let db_num = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let db_name = format!("test_db_{}", db_num);
    
    let container = init_global_container().await;
    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    
    // First, connect to the default postgres database to create a new test database
    // Use minimal connection pool settings for the admin connection
    let admin_url = format!("postgres://postgres:postgres@{}:{}/postgres", host, port);
    let mut admin_opt = ConnectOptions::new(admin_url);
    admin_opt
        .max_connections(1)
        .min_connections(1)
        .sqlx_logging(false);
    let admin_db = Database::connect(admin_opt).await?;
    
    // Create a new database for this test
    use sea_orm::ConnectionTrait;
    admin_db
        .execute(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            format!("CREATE DATABASE {}", db_name),
        ))
        .await?;
    
    // Drop the admin connection
    drop(admin_db);
    
    // Connect to the newly created test database with minimal connection pool
    let db_url = format!("postgres://postgres:postgres@{}:{}/{}", host, port, db_name);
    let mut db_opt = ConnectOptions::new(db_url);
    db_opt
        .max_connections(5)
        .min_connections(1)
        .connect_timeout(std::time::Duration::from_secs(30))
        .sqlx_logging(false);
    let db = Database::connect(db_opt).await?;
    
    // Run migrations on the test database
    migration::Migrator::up(&db, None).await?;
    
    Ok(db)
}

/// Legacy function for backwards compatibility.
/// Prefer using `setup_db_with_global_container()` for better performance.
#[deprecated(note = "Use setup_db_with_global_container() instead for shared container")]
pub async fn setup_container() -> anyhow::Result<testcontainers::ContainerAsync<postgres::Postgres>>
{
    let container = postgres::Postgres::default().start().await?;
    Ok(container)
}

/// Legacy function for backwards compatibility.
/// Prefer using `setup_db_with_global_container()` for better performance.
#[deprecated(note = "Use setup_db_with_global_container() instead for shared container")]
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
