use std::sync::Arc;

use auth_claims::AuthService;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use base64::Engine;
use chrono::Utc;
use name::DiscordId;
use name::DiscordServerId;
use serde_json::{Value, json};
use sqlx::PgPool;
use testcontainers_modules::postgres;
use testcontainers_modules::testcontainers::ContainerAsync;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use tower::ServiceExt;
use uuid::Uuid;

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

    let server_repo = discord_server_repo::Repo::new(pool.clone());
    let server_service = Arc::new(discord_server_service::Service::new(server_repo));

    let schema = Arc::new(create_schema());
    let jwks_validator: Arc<dyn AuthService> = Arc::new(auth_claims::AlwaysAllow);

    let app = server::create_router(schema, service, server_service, jwks_validator, None);

    TestContext {
        app,
        pool,
        _container: container,
    }
}

async fn setup_test_context_with_auth_denial() -> TestContext {
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

    let server_repo = discord_server_repo::Repo::new(pool.clone());
    let server_service = Arc::new(discord_server_service::Service::new(server_repo));

    let schema = Arc::new(create_schema());
    let jwks_validator: Arc<dyn AuthService> = Arc::new(auth_claims::AlwaysDeny);

    let app = server::create_router(schema, service, server_service, jwks_validator, None);

    TestContext {
        app,
        pool,
        _container: container,
    }
}

async fn insert_name(pool: &PgPool, discord_id: u64, discord_server: u64, name: &str) {
    let id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO names (id, discord_id, discord_server, name, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(discord_id as i64)
    .bind(discord_server as i64)
    .bind(name)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to insert name");
}

async fn insert_server(pool: &PgPool, discord_server: u64, display_name: &str) {
    let id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        r#"
        INSERT INTO servers (id, discord_server, display_name, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(id)
    .bind(discord_server as i64)
    .bind(display_name)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("Failed to insert server");
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
    let discord_id = 123456789u64;
    let discord_server = 987654321_u64;
    let name_value = "TestName";
    insert_name(&context.pool, discord_id, discord_server, name_value).await;

    let relay_id = RelayId::encode_name(name::DiscordId(discord_id), discord_server);
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

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
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
    let discord_id = 222333444u64;
    let discord_server = 999999999_u64;

    let relay_id = RelayId::encode_name(DiscordId(discord_id), discord_server);
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

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
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

    let (status, body_text) =
        execute_graphql(&context.app, query, json!({}), Some("test-token")).await;
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

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
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
    let discord_id = 444555666u64;
    let discord_server = 555666777_u64;
    insert_name(&context.pool, discord_id, discord_server, "VariableName").await;

    let relay_id = RelayId::encode_name(DiscordId(discord_id), discord_server);
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

    let (status, body_text) =
        execute_graphql(&context.app, query, variables, Some("test-token")).await;
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
async fn test_query_node_with_auth_succeeds() {
    let context = setup_test_context().await;
    let discord_id = 555666777u64;
    let discord_server = 888999000_u64;
    insert_name(&context.pool, discord_id, discord_server, "AuthPlaceholder").await;

    let relay_id = RelayId::encode_name(DiscordId(discord_id), discord_server);
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

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_node_requires_auth() {
    let context = setup_test_context().await;

    let relay_id = RelayId::encode_name(DiscordId(123456789), 987654321);
    let query = r#"
        query {
            node(id: "RELAY_ID") {
                ... on Name { name }
            }
        }
    "#
    .replace("RELAY_ID", &relay_id.to_string());

    // No auth token — should fail
    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
    assert_eq!(status, StatusCode::OK);

    let body = parse_graphql_body(&body_text);
    let errors = body.get("errors").expect("Expected auth error");
    let errors_arr = errors.as_array().expect("errors should be an array");
    assert!(!errors_arr.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_requires_auth() {
    let context = setup_test_context().await;

    let query = r#"
        query {
            server(id: "123456789") {
                serverId
            }
        }
    "#;

    let (status, body_text) = execute_graphql(&context.app, query, json!({}), None).await;
    assert_eq!(status, StatusCode::OK);

    let body = parse_graphql_body(&body_text);
    let errors = body.get("errors").expect("Expected auth error");
    assert!(!errors.as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_servers_requires_auth() {
    let context = setup_test_context().await;

    let query = r#"
        query {
            servers(first: 10) {
                edges { node { serverId } }
            }
        }
    "#;

    let (status, body_text) = execute_graphql(&context.app, query, json!({}), None).await;
    assert_eq!(status, StatusCode::OK);

    let body = parse_graphql_body(&body_text);
    let errors = body.get("errors").expect("Expected auth error");
    assert!(!errors.as_array().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_names_empty() {
    let context = setup_test_context().await;
    let discord_server = DiscordServerId(12345);
    insert_server(&context.pool, discord_server.0, "Test Server").await;

    let query = format!(
        r#"
        query {{
            server(id: "{}") {{
                id
                names(first: 10) {{
                    edges {{
                        cursor
                        node {{
                            name
                        }}
                    }}
                    pageInfo {{
                        hasNextPage
                        hasPreviousPage
                        startCursor
                        endCursor
                    }}
                }}
            }}
        }}
        "#,
        discord_server.0
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let server = body["data"]["server"].as_object().expect("Missing server");
    // Server ID is now a global Relay ID (base64 encoded "Server:{id}")
    let expected_id = graphql_relay::RelayId::encode_server(discord_server.0);
    assert_eq!(server["id"].as_str(), Some(expected_id.as_ref()));

    let names = server["names"].as_object().expect("Missing names");
    let edges = names["edges"].as_array().expect("Missing edges");
    assert_eq!(edges.len(), 0);

    let page_info = names["pageInfo"].as_object().expect("Missing pageInfo");
    assert_eq!(page_info["hasNextPage"].as_bool(), Some(false));
    assert_eq!(page_info["hasPreviousPage"].as_bool(), Some(false));
    assert!(page_info["startCursor"].is_null());
    assert!(page_info["endCursor"].is_null());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_names_first_page() {
    let context = setup_test_context().await;
    let discord_server = DiscordServerId(99999);
    insert_server(&context.pool, discord_server.0, "Test Server").await;

    let discord_id1 = DiscordId(111);
    let discord_id2 = DiscordId(222);
    let discord_id3 = DiscordId(333);

    // Insert in any order
    insert_name(&context.pool, discord_id2.0, discord_server.0, "Charlie").await;
    insert_name(&context.pool, discord_id1.0, discord_server.0, "Alice").await;
    insert_name(&context.pool, discord_id3.0, discord_server.0, "Bob").await;

    let query = format!(
        r#"
        query {{
            server(id: "{}") {{
                names(first: 10) {{
                    edges {{
                        cursor
                        node {{
                            name
                        }}
                    }}
                    pageInfo {{
                        hasNextPage
                        hasPreviousPage
                        startCursor
                        endCursor
                    }}
                }}
            }}
        }}
        "#,
        discord_server.0
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let names = body["data"]["server"]["names"]
        .as_object()
        .expect("Missing names");
    let edges = names["edges"].as_array().expect("Missing edges");

    // Should have all 3 names, ordered by user_id (UUID ordering)
    assert_eq!(edges.len(), 3);

    // Verify all three names are present (order determined by UUID)
    let returned_names: Vec<&str> = edges
        .iter()
        .map(|e| e["node"]["name"].as_str().unwrap())
        .collect();
    assert!(returned_names.contains(&"Alice"));
    assert!(returned_names.contains(&"Bob"));
    assert!(returned_names.contains(&"Charlie"));

    // All edges should have cursors
    assert!(edges[0]["cursor"].as_str().is_some());
    assert!(edges[1]["cursor"].as_str().is_some());
    assert!(edges[2]["cursor"].as_str().is_some());

    let page_info = names["pageInfo"].as_object().expect("Missing pageInfo");
    assert_eq!(page_info["hasNextPage"].as_bool(), Some(false));
    assert_eq!(page_info["hasPreviousPage"].as_bool(), Some(false));
    assert_eq!(
        page_info["startCursor"].as_str(),
        edges[0]["cursor"].as_str()
    );
    assert_eq!(page_info["endCursor"].as_str(), edges[2]["cursor"].as_str());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_names_with_limit() {
    let context = setup_test_context().await;
    let discord_server = DiscordServerId(88888);
    insert_server(&context.pool, discord_server.0, "Test Server").await;

    let discord_id1 = DiscordId(111);
    let discord_id2 = DiscordId(222);
    let discord_id3 = DiscordId(333);

    insert_name(&context.pool, discord_id1.0, discord_server.0, "Alice").await;
    insert_name(&context.pool, discord_id2.0, discord_server.0, "Bob").await;
    insert_name(&context.pool, discord_id3.0, discord_server.0, "Charlie").await;

    let query = format!(
        r#"
        query {{
            server(id: "{}") {{
                names(first: 2) {{
                    edges {{
                        node {{
                            name
                        }}
                    }}
                    pageInfo {{
                        hasNextPage
                    }}
                }}
            }}
        }}
        "#,
        discord_server.0
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let names = body["data"]["server"]["names"]
        .as_object()
        .expect("Missing names");
    let edges = names["edges"].as_array().expect("Missing edges");

    // Should return first 2 names (ordered by user_id)
    assert_eq!(edges.len(), 2);
    // Verify names are present (specific order depends on UUID values)
    assert!(edges[0]["node"]["name"].as_str().is_some());
    assert!(edges[1]["node"]["name"].as_str().is_some());

    let page_info = names["pageInfo"].as_object().expect("Missing pageInfo");
    assert_eq!(page_info["hasNextPage"].as_bool(), Some(true));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_names_with_cursor() {
    let context = setup_test_context().await;
    let discord_server = DiscordServerId(77777);
    insert_server(&context.pool, discord_server.0, "Test Server").await;

    let discord_id1 = DiscordId(111);
    let discord_id2 = DiscordId(222);
    let discord_id3 = DiscordId(333);

    insert_name(&context.pool, discord_id1.0, discord_server.0, "Alice").await;
    insert_name(&context.pool, discord_id2.0, discord_server.0, "Bob").await;
    insert_name(&context.pool, discord_id3.0, discord_server.0, "Charlie").await;

    // First, get all names to know the order and get the first cursor
    let query = format!(
        r#"
        query {{
            server(id: "{}") {{
                names(first: 3) {{
                    edges {{
                        cursor
                        node {{
                            name
                        }}
                    }}
                }}
            }}
        }}
        "#,
        discord_server.0
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    let all_edges = body["data"]["server"]["names"]["edges"]
        .as_array()
        .expect("Missing edges");
    assert_eq!(all_edges.len(), 3);

    // Get the cursor of the first name (whatever it is)
    let first_cursor = all_edges[0]["cursor"].as_str().expect("Missing cursor");

    // Get expected remaining names (everything except the first)
    let expected_names: Vec<&str> = all_edges
        .iter()
        .skip(1)
        .filter_map(|edge| edge["node"]["name"].as_str())
        .collect();

    // Now query with cursor to get names after the first one
    let query_with_cursor = format!(
        r#"
        query {{
            server(id: "{}") {{
                names(first: 10, after: "{}") {{
                    edges {{
                        node {{
                            name
                        }}
                    }}
                    pageInfo {{
                        hasNextPage
                        hasPreviousPage
                    }}
                }}
            }}
        }}
        "#,
        discord_server.0, first_cursor
    );

    let (status, body_text) = execute_graphql(
        &context.app,
        &query_with_cursor,
        json!({}),
        Some("test-token"),
    )
    .await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let edges = body["data"]["server"]["names"]["edges"]
        .as_array()
        .expect("Missing edges");

    // Should get 2 names after the first cursor
    assert_eq!(edges.len(), 2);
    let names: Vec<&str> = edges
        .iter()
        .filter_map(|edge| edge["node"]["name"].as_str())
        .collect();
    // Verify we got exactly the expected names (the two that weren't first)
    assert_eq!(names.len(), expected_names.len());
    for expected in &expected_names {
        assert!(
            names.contains(expected),
            "Missing expected name: {}",
            expected
        );
    }

    let page_info = body["data"]["server"]["names"]["pageInfo"]
        .as_object()
        .expect("Missing pageInfo");
    assert_eq!(page_info["hasNextPage"].as_bool(), Some(false));
    assert_eq!(page_info["hasPreviousPage"].as_bool(), Some(true));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_names_cursor_past_end() {
    let context = setup_test_context().await;
    let discord_server = DiscordServerId(66666);
    insert_server(&context.pool, discord_server.0, "Test Server").await;

    let discord_id1 = DiscordId(111);
    insert_name(&context.pool, discord_id1.0, discord_server.0, "Alice").await;

    // Create a cursor with a DiscordId larger than any existing (but fits in i64)
    let max_id = i64::MAX as u64;
    let cursor = graphql_relay::Cursor::new(DiscordId(max_id));
    let encoded_cursor = cursor.encode();

    let query = format!(
        r#"
        query {{
            server(id: "{}") {{
                names(first: 10, after: "{}") {{
                    edges {{
                        node {{
                            name
                        }}
                    }}
                    pageInfo {{
                        hasNextPage
                    }}
                }}
            }}
        }}
        "#,
        discord_server.0, encoded_cursor
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let edges = body["data"]["server"]["names"]["edges"]
        .as_array()
        .expect("Missing edges");
    assert_eq!(edges.len(), 0);

    let page_info = body["data"]["server"]["names"]["pageInfo"]
        .as_object()
        .expect("Missing pageInfo");
    assert_eq!(page_info["hasNextPage"].as_bool(), Some(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_names_different_servers() {
    let context = setup_test_context().await;
    let discord_server1 = DiscordServerId(11111);
    let discord_server2 = DiscordServerId(22222);
    insert_server(&context.pool, discord_server1.0, "Test Server 1").await;
    insert_server(&context.pool, discord_server2.0, "Test Server 2").await;

    let discord_id1 = DiscordId(111);
    let discord_id2 = DiscordId(222);

    insert_name(&context.pool, discord_id1.0, discord_server1.0, "ServerOne").await;
    insert_name(&context.pool, discord_id2.0, discord_server2.0, "ServerTwo").await;

    // Query server1
    let query = format!(
        r#"
        query {{
            server(id: "{}") {{
                names(first: 10) {{
                    edges {{
                        node {{
                            name
                        }}
                    }}
                }}
            }}
        }}
        "#,
        discord_server1.0
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    let edges = body["data"]["server"]["names"]["edges"]
        .as_array()
        .expect("Missing edges");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["node"]["name"].as_str(), Some("ServerOne"));

    // Query server2
    let query = format!(
        r#"
        query {{
            server(id: "{}") {{
                names(first: 10) {{
                    edges {{
                        node {{
                            name
                        }}
                    }}
                }}
            }}
        }}
        "#,
        discord_server2.0
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    let edges = body["data"]["server"]["names"]["edges"]
        .as_array()
        .expect("Missing edges");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["node"]["name"].as_str(), Some("ServerTwo"));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_names_invalid_cursor() {
    let context = setup_test_context().await;
    let discord_server = DiscordServerId(55555);
    insert_server(&context.pool, discord_server.0, "Test Server").await;

    let query = format!(
        r#"
        query {{
            server(id: "{}") {{
                names(first: 10, after: "invalid-cursor!!!") {{
                    edges {{
                        node {{
                            name
                        }}
                    }}
                }}
            }}
        }}
        "#,
        discord_server.0
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    // Should return an error for invalid cursor
    let errors = body.get("errors");
    assert!(errors.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_names_max_page_size() {
    let context = setup_test_context().await;
    let discord_server = DiscordServerId(44444);
    insert_server(&context.pool, discord_server.0, "Test Server").await;

    // Insert more than MAX_PAGE_SIZE (100) names to test enforcement
    for i in 0..105 {
        let discord_id = DiscordId(1000 + i);
        insert_name(
            &context.pool,
            discord_id.0,
            discord_server.0,
            &format!("User{:03}", i),
        )
        .await;
    }

    // Request more than MAX_PAGE_SIZE
    let query = format!(
        r#"
        query {{
            server(id: "{}") {{
                names(first: 150) {{
                    edges {{
                        node {{
                            name
                        }}
                    }}
                }}
            }}
        }}
        "#,
        discord_server.0
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let edges = body["data"]["server"]["names"]["edges"]
        .as_array()
        .expect("Missing edges");

    // Should return exactly MAX_PAGE_SIZE (100) even though we requested 150
    assert_eq!(
        edges.len(),
        100,
        "Should enforce MAX_PAGE_SIZE of 100, got {}",
        edges.len()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_servers_empty() {
    let context = setup_test_context().await;

    let query = r#"
        query {
            servers(first: 10) {
                edges {
                    cursor
                    node {
                        id
                    }
                }
                pageInfo {
                    hasNextPage
                    hasPreviousPage
                    startCursor
                    endCursor
                }
            }
        }
    "#;

    let (status, body_text) =
        execute_graphql(&context.app, query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let servers = body["data"]["servers"]
        .as_object()
        .expect("Missing servers");
    let edges = servers["edges"].as_array().expect("Missing edges");
    assert_eq!(edges.len(), 0);

    let page_info = servers["pageInfo"].as_object().expect("Missing pageInfo");
    assert_eq!(page_info["hasNextPage"].as_bool(), Some(false));
    assert_eq!(page_info["hasPreviousPage"].as_bool(), Some(false));
    assert!(page_info["startCursor"].is_null());
    assert!(page_info["endCursor"].is_null());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_servers_distinct() {
    let context = setup_test_context().await;

    let server1 = DiscordServerId(11111);
    let server2 = DiscordServerId(22222);
    let server3 = DiscordServerId(33333);
    insert_server(&context.pool, server1.0, "Server 1").await;
    insert_server(&context.pool, server2.0, "Server 2").await;
    insert_server(&context.pool, server3.0, "Server 3").await;

    let discord_id1 = DiscordId(111);
    let discord_id2 = DiscordId(222);
    let discord_id3 = DiscordId(333);

    // Two names on server1, one each on server2 and server3
    insert_name(&context.pool, discord_id1.0, server1.0, "Alice").await;
    insert_name(&context.pool, discord_id2.0, server1.0, "Bob").await;
    insert_name(&context.pool, discord_id3.0, server2.0, "Charlie").await;
    insert_name(&context.pool, discord_id1.0, server3.0, "AliceOnThree").await;

    let query = r#"
        query {
            servers(first: 10) {
                edges {
                    cursor
                    node {
                        id
                    }
                }
                pageInfo {
                    hasNextPage
                    hasPreviousPage
                }
            }
        }
    "#;

    let (status, body_text) =
        execute_graphql(&context.app, query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let servers = body["data"]["servers"]
        .as_object()
        .expect("Missing servers");
    let edges = servers["edges"].as_array().expect("Missing edges");

    // Should have 3 distinct servers
    assert_eq!(edges.len(), 3);

    // All edges should have cursors
    for edge in edges {
        assert!(edge["cursor"].as_str().is_some());
        assert!(edge["node"]["id"].as_str().is_some());
    }

    let page_info = servers["pageInfo"].as_object().expect("Missing pageInfo");
    assert_eq!(page_info["hasNextPage"].as_bool(), Some(false));
    assert_eq!(page_info["hasPreviousPage"].as_bool(), Some(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_servers_pagination() {
    let context = setup_test_context().await;
    insert_server(&context.pool, 11111, "Server 1").await;
    insert_server(&context.pool, 22222, "Server 2").await;
    insert_server(&context.pool, 33333, "Server 3").await;

    let discord_id1 = DiscordId(111);
    let discord_id2 = DiscordId(222);
    let discord_id3 = DiscordId(333);

    insert_name(&context.pool, discord_id1.0, 11111, "Alice").await;
    insert_name(&context.pool, discord_id2.0, 22222, "Bob").await;
    insert_name(&context.pool, discord_id3.0, 33333, "Charlie").await;

    // Page 1: first 2
    let query = r#"
        query {
            servers(first: 2) {
                edges {
                    cursor
                    node {
                        id
                    }
                }
                pageInfo {
                    hasNextPage
                    hasPreviousPage
                    endCursor
                }
            }
        }
    "#;

    let (status, body_text) =
        execute_graphql(&context.app, query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let servers = body["data"]["servers"]
        .as_object()
        .expect("Missing servers");
    let edges = servers["edges"].as_array().expect("Missing edges");
    assert_eq!(edges.len(), 2);

    let page_info = servers["pageInfo"].as_object().expect("Missing pageInfo");
    assert_eq!(page_info["hasNextPage"].as_bool(), Some(true));
    assert_eq!(page_info["hasPreviousPage"].as_bool(), Some(false));

    let end_cursor = page_info["endCursor"].as_str().expect("Missing endCursor");

    // Page 2: after end_cursor
    let query2 = format!(
        r#"
        query {{
            servers(first: 10, after: "{}") {{
                edges {{
                    cursor
                    node {{
                        id
                    }}
                }}
                pageInfo {{
                    hasNextPage
                    hasPreviousPage
                }}
            }}
        }}
        "#,
        end_cursor
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query2, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let servers = body["data"]["servers"]
        .as_object()
        .expect("Missing servers");
    let edges = servers["edges"].as_array().expect("Missing edges");
    assert_eq!(edges.len(), 1);

    let page_info = servers["pageInfo"].as_object().expect("Missing pageInfo");
    assert_eq!(page_info["hasNextPage"].as_bool(), Some(false));
    assert_eq!(page_info["hasPreviousPage"].as_bool(), Some(true));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_servers_cursor_past_end() {
    let context = setup_test_context().await;
    insert_server(&context.pool, 11111, "Test Server").await;

    let discord_id1 = DiscordId(111);
    insert_name(&context.pool, discord_id1.0, 11111, "Alice").await;

    // Create a cursor with a server_id larger than any existing
    // Use a value that fits in i64 since server_id is stored as i64 in PostgreSQL
    let cursor = graphql_relay::ServerCursor::new(99999999);
    let encoded_cursor = cursor.encode();

    let query = format!(
        r#"
        query {{
            servers(first: 10, after: "{}") {{
                edges {{
                    node {{
                        id
                    }}
                }}
                pageInfo {{
                    hasNextPage
                }}
            }}
        }}
        "#,
        encoded_cursor
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let edges = body["data"]["servers"]["edges"]
        .as_array()
        .expect("Missing edges");
    assert_eq!(edges.len(), 0);

    let page_info = body["data"]["servers"]["pageInfo"]
        .as_object()
        .expect("Missing pageInfo");
    assert_eq!(page_info["hasNextPage"].as_bool(), Some(false));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_servers_invalid_cursor() {
    let context = setup_test_context().await;

    let query = r#"
        query {
            servers(first: 10, after: "invalid-cursor!!!") {
                edges {
                    node {
                        id
                    }
                }
            }
        }
    "#;

    let (status, body_text) =
        execute_graphql(&context.app, query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    // Should return an error for invalid cursor
    let errors = body.get("errors");
    assert!(errors.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_servers_total_count() {
    let context = setup_test_context().await;
    insert_server(&context.pool, 100, "Server 100").await;
    insert_server(&context.pool, 200, "Server 200").await;
    insert_name(&context.pool, 1, 100, "Alice").await;
    insert_name(&context.pool, 2, 200, "Bob").await;
    insert_name(&context.pool, 3, 200, "Carol").await;

    let query = r#"
        query {
            servers(first: 10) {
                totalCount
                edges {
                    node {
                        serverId
                    }
                }
            }
        }
    "#;

    let (status, body_text) =
        execute_graphql(&context.app, query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none(), "errors: {body_text}");

    let servers = &body["data"]["servers"];
    assert_eq!(servers["totalCount"].as_i64(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_names_total_count() {
    let context = setup_test_context().await;
    let server = 100u64;
    insert_server(&context.pool, server, "Test Server").await;
    insert_name(&context.pool, 1, server, "Alice").await;
    insert_name(&context.pool, 2, server, "Bob").await;
    insert_name(&context.pool, 3, server, "Carol").await;

    let query = format!(
        r#"
        query {{
            server(id: "{}") {{
                names(first: 10) {{
                    totalCount
                    edges {{
                        node {{
                            name
                        }}
                    }}
                }}
            }}
        }}
        "#,
        server
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none(), "errors: {body_text}");

    let names = &body["data"]["server"]["names"];
    assert_eq!(names["totalCount"].as_i64(), Some(3));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_create_name_mutation_requires_auth() {
    let context = setup_test_context_with_auth_denial().await;

    let query = r#"
        mutation CreateName($input: CreateNameInput!) {
            createName(input: $input) {
                clientMutationId
                name {
                    id
                    name
                    createdAt
                    updatedAt
                }
            }
        }
    "#;

    let variables = json!({
        "input": {
            "clientMutationId": "test-1",
            "discordId": "123456789",
            "discordServerId": "987654321",
            "name": "TestNickname"
        }
    });

    // Without an auth token, the mutation should return an auth error.
    let (status, body_text) = execute_graphql(&context.app, query, variables, None).await;
    assert_eq!(status, StatusCode::OK);

    let body = parse_graphql_body(&body_text);
    let errors = body.get("errors").expect("Expected auth error");
    let errors_arr = errors.as_array().expect("errors should be an array");
    assert!(!errors_arr.is_empty(), "errors array should not be empty");
    assert!(
        body.pointer("/data/createName").is_none()
            || body.pointer("/data/createName").unwrap().is_null(),
        "data.createName should be null on auth error"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_create_name_mutation_rejects_invalid_token() {
    let context = setup_test_context_with_auth_denial().await;

    let query = r#"
        mutation CreateName($input: CreateNameInput!) {
            createName(input: $input) {
                clientMutationId
                name {
                    id
                    name
                }
            }
        }
    "#;

    let variables = json!({
        "input": {
            "discordId": "123",
            "discordServerId": "456",
            "name": "TestName"
        }
    });

    let (status, body_text) = execute_graphql(
        &context.app,
        query,
        variables,
        Some("Bearer invalid.jwt.token"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let body = parse_graphql_body(&body_text);
    let errors = body.get("errors").expect("Expected auth error");
    let errors_arr = errors.as_array().expect("errors should be an array");
    assert!(!errors_arr.is_empty(), "errors array should not be empty");
    assert!(
        errors_arr[0]["message"]
            .as_str()
            .unwrap_or("")
            .contains("Invalid authentication token"),
        "Expected 'Invalid authentication token' error, got: {}",
        errors_arr[0]["message"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_create_names_mutation() {
    let context = setup_test_context().await;

    let query = r#"
        mutation CreateNames($input: CreateNamesInput!) {
            createNames(input: $input) {
                clientMutationId
                names {
                    id
                    name
                    createdAt
                    updatedAt
                }
            }
        }
    "#;

    let variables = json!({
        "input": {
            "clientMutationId": "batch-test-1",
            "discordServerId": "1",
            "names": [
                { "discordId": "111", "name": "Alice" },
                { "discordId": "222", "name": "Bob" }
            ]
        }
    });

    let token = "test-token".to_string();
    let (status, body_text) = execute_graphql(&context.app, query, variables, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "response body: {body_text}");

    let body = parse_graphql_body(&body_text);
    assert!(
        body.get("errors").is_none(),
        "unexpected errors: {body_text}"
    );

    let payload = &body["data"]["createNames"];
    assert_eq!(payload["clientMutationId"], "batch-test-1");

    let names = payload["names"]
        .as_array()
        .expect("names should be an array");
    assert_eq!(names.len(), 2);

    let returned_names: Vec<&str> = names
        .iter()
        .map(|n| n["name"].as_str().expect("name should be a string"))
        .collect();
    assert!(
        returned_names.contains(&"Alice"),
        "Alice missing from {returned_names:?}"
    );
    assert!(
        returned_names.contains(&"Bob"),
        "Bob missing from {returned_names:?}"
    );

    for name in names {
        assert!(name["id"].is_string(), "id should be a string");
        assert!(
            name["createdAt"].is_string(),
            "createdAt should be a string"
        );
        assert!(
            name["updatedAt"].is_string(),
            "updatedAt should be a string"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_create_names_mutation_upserts_duplicates() {
    let context = setup_test_context().await;

    let token = "test-token".to_string();

    // First, create a name via createName
    let create_query = r#"
        mutation CreateName($input: CreateNameInput!) {
            createName(input: $input) {
                name {
                    name
                }
            }
        }
    "#;

    let create_variables = json!({
        "input": {
            "discordId": "111",
            "discordServerId": "999",
            "name": "OriginalName"
        }
    });

    let (status, body_text) =
        execute_graphql(&context.app, create_query, create_variables, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "create failed: {body_text}");
    let body = parse_graphql_body(&body_text);
    assert!(body.get("errors").is_none(), "create errors: {body_text}");
    assert_eq!(body["data"]["createName"]["name"]["name"], "OriginalName");

    // Now batch upsert with overlapping discord_id (111) and a new one (222)
    let batch_query = r#"
        mutation CreateNames($input: CreateNamesInput!) {
            createNames(input: $input) {
                names {
                    name
                }
            }
        }
    "#;

    let batch_variables = json!({
        "input": {
            "discordServerId": "999",
            "names": [
                { "discordId": "111", "name": "UpdatedName" },
                { "discordId": "222", "name": "NewUser" }
            ]
        }
    });

    let (status, body_text) =
        execute_graphql(&context.app, batch_query, batch_variables, Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "batch upsert failed: {body_text}");

    let body = parse_graphql_body(&body_text);
    assert!(
        body.get("errors").is_none(),
        "unexpected errors in batch upsert: {body_text}"
    );

    let names = body["data"]["createNames"]["names"]
        .as_array()
        .expect("names should be an array");
    assert_eq!(names.len(), 2);

    let returned_names: Vec<&str> = names
        .iter()
        .map(|n| n["name"].as_str().expect("name should be a string"))
        .collect();
    assert!(
        returned_names.contains(&"UpdatedName"),
        "UpdatedName missing: {returned_names:?}"
    );
    assert!(
        returned_names.contains(&"NewUser"),
        "NewUser missing: {returned_names:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_create_name_mutation_without_auth_returns_error() {
    let context = setup_test_context_with_auth_denial().await;
    insert_name(&context.pool, 123456789, 987654321, "ExistingName").await;

    let query = r#"
        mutation CreateName($input: CreateNameInput!) {
            createName(input: $input) {
                clientMutationId
                name {
                    id
                    name
                }
            }
        }
    "#;

    let variables = json!({
        "input": {
            "discordId": "123456789",
            "discordServerId": "987654321",
            "name": "DuplicateName"
        }
    });

    // Without an auth token, the mutation should return an auth error.
    let (status, body_text) = execute_graphql(&context.app, query, variables, None).await;
    assert_eq!(status, StatusCode::OK);

    let body = parse_graphql_body(&body_text);
    let errors = body.get("errors").expect("Expected errors");
    let errors_arr = errors.as_array().expect("errors should be an array");
    assert!(!errors_arr.is_empty(), "errors array should not be empty");
    assert!(
        body.pointer("/data/createName").is_none()
            || body.pointer("/data/createName").unwrap().is_null(),
        "data.createName should be null on error"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_static_file_serving_with_spa_fallback() {
    let tmp_dir = std::env::temp_dir().join(format!("nicknamer2-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir).expect("Failed to create temp dir");
    std::fs::write(tmp_dir.join("index.html"), "<html><body>SPA</body></html>")
        .expect("Failed to write index.html");
    std::fs::write(tmp_dir.join("style.css"), "body { color: red; }")
        .expect("Failed to write style.css");

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

    let server_repo = discord_server_repo::Repo::new(pool);
    let server_service = Arc::new(discord_server_service::Service::new(server_repo));

    let schema = Arc::new(graphql_schema::create_schema());
    let jwks_validator: Arc<dyn AuthService> = Arc::new(auth_claims::AlwaysAllow);

    let app = server::create_router(
        schema,
        service,
        server_service,
        jwks_validator,
        Some(tmp_dir.to_str().unwrap()),
    );

    // Known static file returns its content
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/style.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        String::from_utf8(body.to_vec()).unwrap(),
        "body { color: red; }"
    );

    // Unknown route falls back to index.html (SPA routing)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/some/unknown/route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8(body.to_vec()).unwrap().contains("SPA"));

    // GraphQL endpoint still works
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/graphql")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"query": "{ __typename }"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    std::fs::remove_dir_all(&tmp_dir).ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_create_server_success() {
    let context = setup_test_context().await;

    let query = r#"
        mutation CreateServer($input: CreateServerInput!) {
            createServer(input: $input) {
                server {
                    serverId
                    displayName
                    createdAt
                    updatedAt
                }
            }
        }
    "#;

    let variables = json!({
        "input": {
            "discordServerId": "123456",
            "displayName": "My Test Server"
        }
    });

    let (status, body_text) =
        execute_graphql(&context.app, query, variables, Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(
        body.get("errors").is_none(),
        "unexpected errors: {body_text}"
    );

    let server = &body["data"]["createServer"]["server"];
    assert_eq!(server["serverId"].as_str(), Some("123456"));
    assert_eq!(server["displayName"].as_str(), Some("My Test Server"));
    assert!(
        server["createdAt"].is_string(),
        "createdAt should be a string"
    );
    assert!(
        server["updatedAt"].is_string(),
        "updatedAt should be a string"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_not_found() {
    let context = setup_test_context().await;

    let query = r#"
        query {
            server(id: "12345") {
                serverId
                displayName
            }
        }
    "#;

    let (status, body_text) =
        execute_graphql(&context.app, query, json!({}), Some("test-token")).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");

    let errors = body
        .get("errors")
        .expect("Expected errors for non-existent server");
    let errors_arr = errors.as_array().expect("errors should be an array");
    assert!(!errors_arr.is_empty(), "errors array should not be empty");
}
