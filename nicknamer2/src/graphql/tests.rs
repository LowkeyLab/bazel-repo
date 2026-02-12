use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use base64::Engine;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use testcontainers_modules::postgres;
use testcontainers_modules::testcontainers::ContainerAsync;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tower::ServiceExt;
use uuid::Uuid;

use graphql_context::Context;
use graphql_relay::RelayId;
use graphql_schema::create_schema;

struct TestContext {
    app: axum::Router,
    pool: PgPool,
    _container: ContainerAsync<postgres::Postgres>,
}

async fn setup_test_context() -> TestContext {
    let container = postgres::Postgres::default()
        .start()
        .await
        .expect("Failed to start PostgreSQL container");

    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let db_url = format!("postgres://postgres:postgres@{}:{}/postgres", host, port);

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    migrations::run_migrations(&pool)
        .await
        .expect("Failed to run migrations");

    let repo = name_repo::Repo::new(pool.clone());
    let service = Arc::new(name_service::Service::new(repo));

    let schema = Arc::new(create_schema());
    let context = Arc::new(Context {
        name_service: service,
    });

    let app = server::create_router(schema, context);

    TestContext {
        app,
        pool,
        _container: container,
    }
}

async fn insert_user(pool: &PgPool, discord_id: u64) -> Uuid {
    let id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO users (id, discord_id, created_at, updated_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(id)
    .bind(discord_id as i64)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to insert user");

    id
}

async fn insert_name(pool: &PgPool, user_id: Uuid, server_id: u64, name: &str) {
    let id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO names (id, user_id, server_id, name, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(server_id as i64)
    .bind(name)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to insert name");
}

async fn execute_graphql(
    app: &axum::Router,
    query: &str,
    variables: Value,
    auth_token: Option<&str>,
) -> (StatusCode, String) {
    let payload = json!({
        "query": query,
        "variables": variables,
    });

    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/graphql")
        .header("content-type", "application/json");

    if let Some(token) = auth_token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }

    let request = builder
        .body(Body::from(payload.to_string()))
        .expect("Failed to build request");

    let response = app
        .clone()
        .oneshot(request)
        .await
        .expect("Failed to execute request");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("Failed to read response body");
    let body_text = String::from_utf8(body.to_vec()).expect("Failed to decode response body");

    (status, body_text)
}

fn parse_graphql_body(body_text: &str) -> Value {
    serde_json::from_str(body_text)
        .unwrap_or_else(|_| panic!("Failed to parse JSON response: {body_text}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_existing_name_success() {
    let context = setup_test_context().await;
    let user_id = insert_user(&context.pool, 123456789).await;
    let server_id = 987654321_u64;
    let name_value = "TestName";
    insert_name(&context.pool, user_id, server_id, name_value).await;

    let relay_id = RelayId::encode_name(user_id, server_id);
    let query = r#"
        query {
            node(id: "RELAY_ID") {
                id
                ... on Name {
                    name
                    createdAt
                    updatedAt
                }
            }
        }
    "#
    .replace("RELAY_ID", &relay_id.to_string());

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let data = body.get("data").expect("Missing data");
    let node = data.get("node").expect("Missing node field");
    assert_eq!(
        node.get("id").and_then(Value::as_str),
        Some(relay_id.to_string().as_str())
    );
    assert_eq!(node.get("name").and_then(Value::as_str), Some(name_value));
    assert!(node.get("createdAt").and_then(Value::as_str).is_some());
    assert!(node.get("updatedAt").and_then(Value::as_str).is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_non_existent_name_returns_null() {
    let context = setup_test_context().await;
    let user_id = insert_user(&context.pool, 222333444).await;
    let server_id = 999999999_u64;

    let relay_id = RelayId::encode_name(user_id, server_id);
    let query = r#"
        query {
            node(id: "RELAY_ID") {
                id
                ... on Name {
                    name
                }
            }
        }
    "#
    .replace("RELAY_ID", &relay_id.to_string());

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let data = body.get("data").expect("Missing data");
    assert!(data.get("node").map(Value::is_null).unwrap_or(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_invalid_base64_id() {
    let context = setup_test_context().await;

    let query = r#"
        query {
            node(id: "not-valid-base64!!!") {
                id
                ... on Name {
                    name
                }
            }
        }
    "#;

    let (status, body_text) = execute_graphql(&context.app, query, json!({}), None).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    let errors = body.get("errors").expect("Expected errors");
    assert!(errors.as_array().is_some());

    let data = body.get("data").expect("Missing data");
    assert!(data.get("node").map(Value::is_null).unwrap_or(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_unknown_node_type() {
    let context = setup_test_context().await;

    // Create an ID for an unknown type "User"
    let fake_id = base64::engine::general_purpose::STANDARD
        .encode("User:550e8400-e29b-41d4-a716-446655440000:12345");

    let query = format!(
        r#"
        query {{
            node(id: "{}") {{
                id
            }}
        }}
        "#,
        fake_id
    );

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
    let body = parse_graphql_body(&body_text);

    eprintln!("Response body: {}", body_text);
    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    // Unknown types should return null, not an error
    let data = body.get("data").expect("Missing data");
    assert!(data.get("node").map(Value::is_null).unwrap_or(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_with_variables() {
    let context = setup_test_context().await;
    let user_id = insert_user(&context.pool, 444555666).await;
    let server_id = 555666777_u64;
    insert_name(&context.pool, user_id, server_id, "VariableName").await;

    let relay_id = RelayId::encode_name(user_id, server_id);
    let query = r#"
        query GetNode($id: ID!) {
            node(id: $id) {
                ... on Name {
                    name
                }
            }
        }
    "#;

    let variables = json!({
        "id": relay_id.to_string(),
    });

    let (status, body_text) = execute_graphql(&context.app, query, variables, None).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let data = body.get("data").expect("Missing data");
    let node = data.get("node").expect("Missing node field");
    assert_eq!(
        node.get("name").and_then(Value::as_str),
        Some("VariableName")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_with_future_auth_placeholder() {
    let context = setup_test_context().await;
    let user_id = insert_user(&context.pool, 555666777).await;
    let server_id = 888999000_u64;
    insert_name(&context.pool, user_id, server_id, "AuthPlaceholder").await;

    let relay_id = RelayId::encode_name(user_id, server_id);
    let query = r#"
        query {
            node(id: "RELAY_ID") {
                ... on Name {
                    name
                }
            }
        }
    "#
    .replace("RELAY_ID", &relay_id.to_string());

    // TODO: Provide auth token once authentication is implemented.
    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());
}
