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

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_names_empty() {
    let context = setup_test_context().await;
    let server_id = 12345;

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
        server_id
    );

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    assert!(body.get("errors").is_none());

    let server = body["data"]["server"].as_object().expect("Missing server");
    // Server ID is now a global Relay ID (base64 encoded "Server:{id}")
    let expected_id = graphql_relay::RelayId::encode_server(server_id);
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
    let server_id = 99999;

    let user1 = insert_user(&context.pool, 111).await;
    let user2 = insert_user(&context.pool, 222).await;
    let user3 = insert_user(&context.pool, 333).await;

    // Insert in any order
    insert_name(&context.pool, user2, server_id, "Charlie").await;
    insert_name(&context.pool, user1, server_id, "Alice").await;
    insert_name(&context.pool, user3, server_id, "Bob").await;

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
        server_id
    );

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
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
    let server_id = 88888;

    let user1 = insert_user(&context.pool, 111).await;
    let user2 = insert_user(&context.pool, 222).await;
    let user3 = insert_user(&context.pool, 333).await;

    insert_name(&context.pool, user1, server_id, "Alice").await;
    insert_name(&context.pool, user2, server_id, "Bob").await;
    insert_name(&context.pool, user3, server_id, "Charlie").await;

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
        server_id
    );

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
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
    let server_id = 77777;

    let user1 = insert_user(&context.pool, 111).await;
    let user2 = insert_user(&context.pool, 222).await;
    let user3 = insert_user(&context.pool, 333).await;

    insert_name(&context.pool, user1, server_id, "Alice").await;
    insert_name(&context.pool, user2, server_id, "Bob").await;
    insert_name(&context.pool, user3, server_id, "Charlie").await;

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
        server_id
    );

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
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
        server_id, first_cursor
    );

    let (status, body_text) =
        execute_graphql(&context.app, &query_with_cursor, json!({}), None).await;
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
    let server_id = 66666;

    let user1 = insert_user(&context.pool, 111).await;
    insert_name(&context.pool, user1, server_id, "Alice").await;

    // Create a cursor with max UUID (greater than any actual user_id)
    let max_uuid = uuid::Uuid::from_u128(u128::MAX);
    let cursor = graphql_relay::Cursor::new(max_uuid);
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
        server_id, encoded_cursor
    );

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
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
    let server1 = 11111;
    let server2 = 22222;

    let user1 = insert_user(&context.pool, 111).await;
    let user2 = insert_user(&context.pool, 222).await;

    insert_name(&context.pool, user1, server1, "ServerOne").await;
    insert_name(&context.pool, user2, server2, "ServerTwo").await;

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
        server1
    );

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
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
        server2
    );

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
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
    let server_id = 55555;

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
        server_id
    );

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    // Should return an error for invalid cursor
    let errors = body.get("errors");
    assert!(errors.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn test_query_server_names_max_page_size() {
    let context = setup_test_context().await;
    let server_id = 44444;

    // Insert more than MAX_PAGE_SIZE (100) names to test enforcement
    for i in 0..105 {
        let user = insert_user(&context.pool, 1000 + i).await;
        insert_name(&context.pool, user, server_id, &format!("User{:03}", i)).await;
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
        server_id
    );

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
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

    let (status, body_text) = execute_graphql(&context.app, query, json!({}), None).await;
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

    let server1: u64 = 11111;
    let server2: u64 = 22222;
    let server3: u64 = 33333;

    let user1 = insert_user(&context.pool, 111).await;
    let user2 = insert_user(&context.pool, 222).await;
    let user3 = insert_user(&context.pool, 333).await;

    // Two names on server1, one each on server2 and server3
    insert_name(&context.pool, user1, server1, "Alice").await;
    insert_name(&context.pool, user2, server1, "Bob").await;
    insert_name(&context.pool, user3, server2, "Charlie").await;
    insert_name(&context.pool, user1, server3, "AliceOnThree").await;

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

    let (status, body_text) = execute_graphql(&context.app, query, json!({}), None).await;
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

    let user1 = insert_user(&context.pool, 111).await;
    let user2 = insert_user(&context.pool, 222).await;
    let user3 = insert_user(&context.pool, 333).await;

    insert_name(&context.pool, user1, 11111, "Alice").await;
    insert_name(&context.pool, user2, 22222, "Bob").await;
    insert_name(&context.pool, user3, 33333, "Charlie").await;

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

    let (status, body_text) = execute_graphql(&context.app, query, json!({}), None).await;
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

    let (status, body_text) = execute_graphql(&context.app, &query2, json!({}), None).await;
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

    let user1 = insert_user(&context.pool, 111).await;
    insert_name(&context.pool, user1, 11111, "Alice").await;

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

    let (status, body_text) = execute_graphql(&context.app, &query, json!({}), None).await;
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

    let (status, body_text) = execute_graphql(&context.app, query, json!({}), None).await;
    let body = parse_graphql_body(&body_text);

    assert_eq!(status, StatusCode::OK, "response body: {body_text}");
    // Should return an error for invalid cursor
    let errors = body.get("errors");
    assert!(errors.is_some());
}
