use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use insta::assert_yaml_snapshot;
use nicknamer_server::entities::name;
use nicknamer_server::name::api::v1::create_api_router;
use nicknamer_server::name::web::{NameState, create_name_router};
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
use std::sync::Arc;
use testcontainers_modules::{postgres, testcontainers};
use tower::ServiceExt;

mod common;

use common::HttpResponseSnapshot;

/// Test context for endpoint tests.
pub struct TestContext {
    #[allow(dead_code)] // container is kept to ensure it's not dropped
    pub container: testcontainers::ContainerAsync<postgres::Postgres>,
    pub db: DatabaseConnection,
}

/// Setup function for endpoint tests using PostgreSQL container.
async fn setup() -> anyhow::Result<TestContext> {
    // Allow multiple calls to init for tests.
    let _ = tracing_subscriber::fmt().try_init();
    let container = common::setup_container().await?;
    let db = common::setup_db(&container).await?;
    Ok(TestContext { db, container })
}

/// Test helper to create test names in the database.
async fn create_test_names(db: &DatabaseConnection) {
    let name1 = name::ActiveModel {
        discord_id: Set(123456789),
        name: Set("TestUser1".to_string()),
        server_id: Set("test-server-1".to_string()),
        ..Default::default()
    };

    let name2 = name::ActiveModel {
        discord_id: Set(987654321),
        name: Set("TestUser2".to_string()),
        server_id: Set("test-server-1".to_string()),
        ..Default::default()
    };

    let _result1 = name1.insert(db).await.unwrap();
    let _result2 = name2.insert(db).await.unwrap();
}

/// Test helper to create test names in multiple servers.
async fn create_test_names_multiple_servers(db: &DatabaseConnection) {
    // Server 1 names
    let name1 = name::ActiveModel {
        discord_id: Set(123456789),
        name: Set("Alice".to_string()),
        server_id: Set("server1".to_string()),
        ..Default::default()
    };

    let name2 = name::ActiveModel {
        discord_id: Set(987654321),
        name: Set("Bob".to_string()),
        server_id: Set("server1".to_string()),
        ..Default::default()
    };

    // Server 2 names
    let name3 = name::ActiveModel {
        discord_id: Set(555666777),
        name: Set("Charlie".to_string()),
        server_id: Set("server2".to_string()),
        ..Default::default()
    };

    let name4 = name::ActiveModel {
        discord_id: Set(444333222),
        name: Set("David".to_string()),
        server_id: Set("server2".to_string()),
        ..Default::default()
    };

    let _result1 = name1.insert(db).await.unwrap();
    let _result2 = name2.insert(db).await.unwrap();
    let _result3 = name3.insert(db).await.unwrap();
    let _result4 = name4.insert(db).await.unwrap();
}

/// Test helper to create test names in the database and return their IDs.
async fn create_test_names_with_ids(db: &DatabaseConnection) -> Vec<i32> {
    let name1 = name::ActiveModel {
        discord_id: Set(123456789),
        name: Set("TestUser1".to_string()),
        server_id: Set("test-server-1".to_string()),
        ..Default::default()
    };

    let name2 = name::ActiveModel {
        discord_id: Set(987654321),
        name: Set("TestUser2".to_string()),
        server_id: Set("test-server-1".to_string()),
        ..Default::default()
    };

    let name3 = name::ActiveModel {
        discord_id: Set(555444333),
        name: Set("TestUser3".to_string()),
        server_id: Set("test-server-1".to_string()),
        ..Default::default()
    };

    let result1 = name1.insert(db).await.unwrap();
    let result2 = name2.insert(db).await.unwrap();
    let result3 = name3.insert(db).await.unwrap();

    vec![result1.id, result2.id, result3.id]
}

/// Test helper to create a single test name and return its ID.
async fn create_single_test_name(db: &DatabaseConnection) -> i32 {
    let name = name::ActiveModel {
        discord_id: Set(555444333),
        name: Set("DeleteTestUser".to_string()),
        server_id: Set("test-server-1".to_string()),
        ..Default::default()
    };

    let result = name.insert(db).await.unwrap();
    result.id
}

/// Test helper to create a single test name for editing and return its ID.
async fn create_editable_test_name(db: &DatabaseConnection) -> i32 {
    let name = name::ActiveModel {
        discord_id: Set(777888999),
        name: Set("EditableTestUser".to_string()),
        server_id: Set("test-server-1".to_string()),
        ..Default::default()
    };

    let result = name.insert(db).await.unwrap();
    result.id
}

/// Test helper to create a `NameState` wrapped in `Arc` for use in tests.
///
/// This function is used to create a shared state (`NameState`) that can be safely
/// accessed across multiple threads during tests. The `Arc` wrapper ensures that
/// the state can be shared and accessed concurrently without ownership issues.
///
/// # Parameters
/// - `db`: A `DatabaseConnection` instance used to initialize the `NameState`.
///
/// # Returns
/// An `Arc<NameState>` instance that wraps the shared state.
fn create_name_state(db: DatabaseConnection) -> Arc<NameState> {
    Arc::new(NameState { db: Arc::new(db) })
}

#[tokio::test]
async fn can_display_names_table_when_names_exist() {
    let state = setup().await.expect("Failed to setup test context");
    create_test_names(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .uri("/names")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "names_table_with_existing_names",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_display_empty_names_table_when_no_names_exist() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .uri("/names")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot_data = HttpResponseSnapshot::new(body_text, status, &headers, "empty_names_table");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn names_endpoint_returns_correct_content_type() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .uri("/names")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "names_endpoint_content_type_check",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_create_name_successfully() {
    let state = setup().await.expect("Failed to setup test context");
    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state.clone());

    let form_data = "discord_id=555666777&name=NewTestUser&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::POST)
        .uri("/names")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(form_data))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot_data = HttpResponseSnapshot::new(body_text, status, &headers, "create_name_successfully");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_create_multiple_names_and_update_count() {
    let state = setup().await.expect("Failed to setup test context");
    create_test_names(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state.clone());

    let form_data = "discord_id=111222333&name=ThirdUser&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::POST)
        .uri("/names")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(form_data))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "create_multiple_names_update_count",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_handle_form_with_special_characters_in_name() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state.clone());

    let form_data = "discord_id=888999000&name=User%20With%20Spaces%21&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::POST)
        .uri("/names")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(form_data))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot_data = HttpResponseSnapshot::new(body_text, status, &headers, "form_with_special_characters");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_serve_add_name_form() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .uri("/names/add")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot_data = HttpResponseSnapshot::new(body_text, status, &headers, "add_name_form");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn post_endpoint_returns_table_fragment_not_full_page() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state.clone());

    let form_data = "discord_id=777888999&name=FragmentTestUser&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::POST)
        .uri("/names")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(form_data))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot_data = HttpResponseSnapshot::new(body_text, status, &headers, "table_fragment_not_full_page");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn cannot_create_name_with_duplicate_discord_id() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state.clone());

    // First, create a name with a specific Discord ID
    let form_data = "discord_id=123456789&name=FirstUser&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::POST)
        .uri("/names")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(form_data))
        .unwrap();

    let _response = app.oneshot(request).await.unwrap();

    // Now try to create another name with the same Discord ID
    let duplicate_form_data = "discord_id=123456789&name=SecondUser&server_id=test-server-1";
    let app2 = create_name_router(name_state.clone());
    let duplicate_request = Request::builder()
        .method(Method::POST)
        .uri("/names")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(duplicate_form_data))
        .unwrap();

    let duplicate_response = app2.oneshot(duplicate_request).await.unwrap();

    let headers = duplicate_response.headers().clone();
    let body = axum::body::to_bytes(duplicate_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Verify the error message is user-friendly
    assert!(body_text.contains("A name entry already exists for this Discord ID"));

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        StatusCode::UNPROCESSABLE_ENTITY,
        &headers,
        "duplicate_discord_id_error",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_delete_name_successfully() {
    let state = setup().await.expect("Failed to setup test context");
    let name_id = create_single_test_name(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/names/{}", name_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Should return the updated names table (empty in this case)
    assert!(body_text.contains("No names found in the database"));

    let snapshot_data = HttpResponseSnapshot::new(body_text, status, &headers, "delete_name_successfully");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_delete_name_and_update_table_count() {
    let state = setup().await.expect("Failed to setup test context");

    // Create multiple names
    create_test_names(&state.db).await;
    let delete_name_id = create_single_test_name(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/names/{}", delete_name_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Should show remaining names (2) and updated count
    assert!(body_text.contains("TestUser1"));
    assert!(body_text.contains("TestUser2"));
    assert!(!body_text.contains("DeleteTestUser"));
    assert!(body_text.contains("<div class=\"stat-value\">2</div>"));

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "delete_name_and_update_table_count",
    );

    assert_yaml_snapshot!(snapshot_data);
}

/// API v1 tests module for JSON endpoints
pub mod api {
    pub mod v1 {
        use super::super::*;
        use common::JsonApiResponseSnapshot;
        use serde_json::Value;

        #[tokio::test]
        async fn can_get_names_as_json_when_names_exist() {
            let state = setup().await.expect("Failed to setup test context");
            create_test_names(&state.db).await;

            let name_state = create_name_state(state.db);
            let app = create_api_router(name_state);

            let request = Request::builder()
                .method(Method::GET)
                .uri("/names")
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();

            let status = response.status();
            let headers = response.headers().clone();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body_text = std::str::from_utf8(&body).unwrap();

            // Should return 200 OK
            assert_eq!(status, StatusCode::OK);

            // Should return JSON content type
            assert_eq!(headers.get("content-type").unwrap(), "application/json");

            // Parse and validate JSON structure
            let json: Value = serde_json::from_str(body_text).expect("Should be valid JSON");
            assert!(json["names"].is_array());
            assert_eq!(json["count"], 2);

            // Validate the names array contains our test data
            let names = json["names"].as_array().unwrap();
            assert_eq!(names.len(), 2);

            // Check that both test users are present
            let name_values: Vec<&str> =
                names.iter().map(|n| n["name"].as_str().unwrap()).collect();
            assert!(name_values.contains(&"TestUser1"));
            assert!(name_values.contains(&"TestUser2"));

            let snapshot_data =
                JsonApiResponseSnapshot::new(body_text, status, &headers, "api_v1_names_with_data");

            assert_yaml_snapshot!(snapshot_data);
        }

        #[tokio::test]
        async fn can_filter_names_by_server_id() {
            let state = setup().await.expect("Failed to setup test context");
            create_test_names_multiple_servers(&state.db).await;

            let name_state = create_name_state(state.db);
            let app = create_api_router(name_state);

            // Request names for server1
            let request = Request::builder()
                .method(Method::GET)
                .uri("/names?server_id=server1")
                .body(Body::empty())
                .unwrap();

            let response = app.oneshot(request).await.unwrap();

            let status = response.status();
            let headers = response.headers().clone();
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body_text = std::str::from_utf8(&body).unwrap();

            // Should return 200 OK
            assert_eq!(status, StatusCode::OK);

            // Should return JSON content type
            assert_eq!(headers.get("content-type").unwrap(), "application/json");

            // Parse and validate JSON structure
            let json: Value = serde_json::from_str(body_text).expect("Should be valid JSON");
            assert!(json["names"].is_array());
            assert_eq!(json["count"], 2);

            // Validate that only server1 names are returned
            let names = json["names"].as_array().unwrap();
            assert_eq!(names.len(), 2);

            let name_values: Vec<&str> =
                names.iter().map(|n| n["name"].as_str().unwrap()).collect();
            assert!(name_values.contains(&"Alice"));
            assert!(name_values.contains(&"Bob"));
            assert!(!name_values.contains(&"Charlie"));
            assert!(!name_values.contains(&"David"));

            // Verify all names have the correct server_id
            for name in names {
                assert_eq!(name["server_id"].as_str().unwrap(), "server1");
            }

            let snapshot_data = JsonApiResponseSnapshot::new(
                body_text,
                status,
                &headers,
                "api_v1_names_filtered_server1",
            );

            assert_yaml_snapshot!(snapshot_data);
        }
    }
}
