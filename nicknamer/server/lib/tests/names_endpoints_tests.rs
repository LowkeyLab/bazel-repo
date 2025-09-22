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

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "create_name_successfully");

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

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "form_with_special_characters");

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

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "table_fragment_not_full_page");

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

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "delete_name_successfully");

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

#[tokio::test]
async fn can_handle_delete_request_for_nonexistent_name() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    // Try to delete a name with ID that doesn't exist
    let request = Request::builder()
        .method(Method::DELETE)
        .uri("/names/99999")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Should contain error message
    assert!(body_text.contains("An unexpected error occurred"));

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "delete_nonexistent_name_error");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn delete_endpoint_returns_table_fragment_not_full_page() {
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

    // Should return only the table fragment, not a full HTML page
    assert!(!body_text.contains("<html"));
    assert!(!body_text.contains("<head"));
    assert!(!body_text.contains("<body"));

    // Should contain the table structure or empty message
    assert!(body_text.contains("No names found") || body_text.contains("<table"));

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "delete_returns_table_fragment");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn delete_endpoint_returns_correct_content_type() {
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

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "delete_endpoint_content_type_check",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_serve_edit_name_form() {
    let state = setup().await.expect("Failed to setup test context");
    let name_id = create_editable_test_name(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .uri(format!("/names/{}/edit", name_id))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Should contain the edit form with the current name
    assert!(body_text.contains("EditableTestUser"));
    assert!(body_text.contains("name=\"name\""));
    assert!(body_text.contains("hx-put"));

    let snapshot_data = HttpResponseSnapshot::new(body_text, status, &headers, "edit_name_form");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_handle_edit_form_request_for_nonexistent_name() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .uri("/names/99999/edit")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Should contain error message
    assert!(body_text.contains("An unexpected error occurred"));

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "edit_form_nonexistent_name_error",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_update_name_successfully() {
    let state = setup().await.expect("Failed to setup test context");
    let name_id = create_editable_test_name(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let form_data = "name=UpdatedTestUser&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("/names/{}", name_id))
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

    // Should return the updated name row
    assert!(body_text.contains("UpdatedTestUser"));
    assert!(!body_text.contains("EditableTestUser"));

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "update_name_successfully");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_update_name_with_special_characters() {
    let state = setup().await.expect("Failed to setup test context");
    let name_id = create_editable_test_name(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let form_data = "name=Updated%20User%20With%20Spaces%21%40%23&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("/names/{}", name_id))
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

    // Should handle URL decoding and contain the updated name
    assert!(body_text.contains("Updated User With Spaces!@#"));

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "update_name_with_special_characters",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_handle_update_request_for_nonexistent_name() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let form_data = "name=NonexistentUser&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::PUT)
        .uri("/names/99999")
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

    // Should contain error message
    assert!(body_text.contains("An unexpected error occurred"));

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "update_nonexistent_name_error");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn update_endpoint_returns_name_row_fragment_not_full_page() {
    let state = setup().await.expect("Failed to setup test context");
    let name_id = create_editable_test_name(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let form_data = "name=FragmentTestUser&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("/names/{}", name_id))
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

    // Should return only the name row fragment, not a full HTML page
    assert!(!body_text.contains("<html"));
    assert!(!body_text.contains("<head"));
    assert!(!body_text.contains("<body"));

    // Should contain the table row structure
    assert!(body_text.contains("<tr"));
    assert!(body_text.contains("FragmentTestUser"));

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "update_returns_name_row_fragment",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn update_endpoint_returns_correct_content_type() {
    let state = setup().await.expect("Failed to setup test context");
    let name_id = create_editable_test_name(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let form_data = "name=ContentTypeTestUser&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("/names/{}", name_id))
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
        "update_endpoint_content_type_check",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn edit_form_endpoint_returns_correct_content_type() {
    let state = setup().await.expect("Failed to setup test context");
    let name_id = create_editable_test_name(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .uri(format!("/names/{}/edit", name_id))
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
        "edit_form_endpoint_content_type_check",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_update_name_with_empty_string() {
    let state = setup().await.expect("Failed to setup test context");
    let name_id = create_editable_test_name(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let form_data = "name=&server_id=test-server-1";
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("/names/{}", name_id))
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

    // Should handle empty string as name
    assert!(body_text.contains("<td></td>") || body_text.contains("</td>"));

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "update_name_with_empty_string");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_update_name_with_very_long_string() {
    let state = setup().await.expect("Failed to setup test context");
    let name_id = create_editable_test_name(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let long_name = "A".repeat(100); // 100 character name
    let form_data = format!("name={}&server_id=test-server-1", long_name);
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("/names/{}", name_id))
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

    // Should handle very long names
    assert!(body_text.contains(&long_name));

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "update_name_with_very_long_string",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_get_names_table_fragment_when_names_exist() {
    let state = setup().await.expect("Failed to setup test context");
    create_test_names(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/names/table")
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
        "names_table_fragment_with_data",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_get_empty_names_table_fragment_when_no_names_exist() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/names/table")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "empty_names_table_fragment");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn names_table_endpoint_returns_correct_content_type() {
    let state = setup().await.expect("Failed to setup test context");
    create_test_names(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/names/table")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();

    // Should return 200 OK
    assert_eq!(status, StatusCode::OK);

    // Should have HTML content type
    assert!(
        headers
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/html")
    );
}

#[tokio::test]
async fn names_table_fragment_contains_table_structure() {
    let state = setup().await.expect("Failed to setup test context");
    create_test_names(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/names/table")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Should contain table structure
    assert!(body_text.contains("<table"));
    assert!(body_text.contains("</table>"));

    // Should contain table headers
    assert!(body_text.contains("<th"));

    // Should contain table rows with data
    assert!(body_text.contains("<tr"));

    // Should not contain full HTML document structure (no html, head, body tags)
    assert!(!body_text.contains("<html"));
    assert!(!body_text.contains("<head"));
    assert!(!body_text.contains("<body"));
}

#[tokio::test]
async fn names_table_fragment_sorts_names_by_id() {
    let state = setup().await.expect("Failed to setup test context");

    // Create names in non-sequential order to test sorting
    let name3 = name::ActiveModel {
        id: Set(3),
        discord_id: Set(333444555),
        name: Set("ThirdUser".to_string()),
        server_id: Set("test-server-1".to_string()),
    };

    let name1 = name::ActiveModel {
        id: Set(1),
        discord_id: Set(111222333),
        name: Set("FirstUser".to_string()),
        server_id: Set("test-server-1".to_string()),
    };

    let name2 = name::ActiveModel {
        id: Set(2),
        discord_id: Set(222333444),
        name: Set("SecondUser".to_string()),
        server_id: Set("test-server-1".to_string()),
    };

    let _result3 = name3.insert(&state.db).await.unwrap();
    let _result1 = name1.insert(&state.db).await.unwrap();
    let _result2 = name2.insert(&state.db).await.unwrap();

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/names/table")
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
        "names_table_fragment_sorted_by_id",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn names_table_fragment_handles_large_dataset() {
    let state = setup().await.expect("Failed to setup test context");

    // Create multiple names to test pagination/large dataset handling
    for i in 1..=10 {
        let name = name::ActiveModel {
            id: Set(i),
            discord_id: Set(100000000 + i as i64),
            name: Set(format!("TestUser{}", i)),
            server_id: Set("test-server-1".to_string()),
        };
        let _result = name.insert(&state.db).await.unwrap();
    }

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/names/table")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Should handle large dataset successfully
    assert_eq!(status, StatusCode::OK);

    // Should contain all 10 test users
    for i in 1..=10 {
        assert!(body_text.contains(&format!("TestUser{}", i)));
    }

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "names_table_fragment_large_dataset",
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
        async fn can_get_empty_names_as_json_when_no_names_exist() {
            let state = setup().await.expect("Failed to setup test context");

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
            assert_eq!(json["count"], 0);
            assert_eq!(json["names"].as_array().unwrap().len(), 0);

            let snapshot_data =
                JsonApiResponseSnapshot::new(body_text, status, &headers, "api_v1_names_empty");

            assert_yaml_snapshot!(snapshot_data);
        }

        #[tokio::test]
        async fn api_v1_names_endpoint_returns_correct_json_structure() {
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

            // Parse JSON and validate structure
            let json: Value = serde_json::from_str(body_text).expect("Should be valid JSON");

            // Validate root structure
            assert!(json.is_object());
            assert!(json["names"].is_array());
            assert!(json["count"].is_number());

            // Validate each name object structure
            let names = json["names"].as_array().unwrap();
            for name in names {
                assert!(name["id"].is_number());
                assert!(name["discord_id"].is_number());
                assert!(name["name"].is_string());

                // Ensure IDs are positive
                assert!(name["id"].as_u64().unwrap() > 0);
                assert!(name["discord_id"].as_u64().unwrap() > 0);
                assert!(!name["name"].as_str().unwrap().is_empty());
            }

            let snapshot_data = JsonApiResponseSnapshot::new(
                body_text,
                status,
                &headers,
                "api_v1_names_json_structure",
            );

            assert_yaml_snapshot!(snapshot_data);
        }

        #[tokio::test]
        async fn api_v1_names_endpoint_handles_large_dataset() {
            let state = setup().await.expect("Failed to setup test context");

            // Create 10 test names
            for i in 1..=10 {
                let name = name::ActiveModel {
                    discord_id: Set(100000000 + i),
                    name: Set(format!("TestUser{}", i)),
                    server_id: Set("test-server-1".to_string()),
                    ..Default::default()
                };
                let _result = name.insert(&state.db).await.unwrap();
            }

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

            // Parse and validate JSON structure
            let json: Value = serde_json::from_str(body_text).expect("Should be valid JSON");
            assert_eq!(json["count"], 10);
            assert_eq!(json["names"].as_array().unwrap().len(), 10);

            // Verify all names are present
            let names = json["names"].as_array().unwrap();
            let name_values: Vec<&str> =
                names.iter().map(|n| n["name"].as_str().unwrap()).collect();

            for i in 1..=10 {
                assert!(name_values.contains(&format!("TestUser{}", i).as_str()));
            }

            let snapshot_data = JsonApiResponseSnapshot::new(
                body_text,
                status,
                &headers,
                "api_v1_names_large_dataset",
            );

            assert_yaml_snapshot!(snapshot_data);
        }

        #[tokio::test]
        async fn api_v1_names_endpoint_returns_names_in_consistent_order() {
            let state = setup().await.expect("Failed to setup test context");
            create_test_names(&state.db).await;

            let name_state = create_name_state(state.db);
            let app = create_api_router(name_state.clone());

            // Make two requests to ensure consistent ordering
            let request1 = Request::builder()
                .method(Method::GET)
                .uri("/names")
                .body(Body::empty())
                .unwrap();

            let response1 = app.oneshot(request1).await.unwrap();
            let body1 = axum::body::to_bytes(response1.into_body(), usize::MAX)
                .await
                .unwrap();
            let body_text1 = std::str::from_utf8(&body1).unwrap();

            let app2 = create_api_router(name_state);
            let request2 = Request::builder()
                .method(Method::GET)
                .uri("/names")
                .body(Body::empty())
                .unwrap();

            let response2 = app2.oneshot(request2).await.unwrap();
            let body2 = axum::body::to_bytes(response2.into_body(), usize::MAX)
                .await
                .unwrap();
            let body_text2 = std::str::from_utf8(&body2).unwrap();

            // Both responses should be identical
            assert_eq!(body_text1, body_text2);

            // Parse and verify structure
            let json: Value = serde_json::from_str(body_text1).expect("Should be valid JSON");
            assert!(json["names"].is_array());
            assert_eq!(json["count"], 2);
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

        #[tokio::test]
        async fn can_filter_names_by_different_server_id() {
            let state = setup().await.expect("Failed to setup test context");
            create_test_names_multiple_servers(&state.db).await;

            let name_state = create_name_state(state.db);
            let app = create_api_router(name_state);

            // Request names for server2
            let request = Request::builder()
                .method(Method::GET)
                .uri("/names?server_id=server2")
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

            // Parse and validate JSON structure
            let json: Value = serde_json::from_str(body_text).expect("Should be valid JSON");
            assert!(json["names"].is_array());
            assert_eq!(json["count"], 2);

            // Validate that only server2 names are returned
            let names = json["names"].as_array().unwrap();
            assert_eq!(names.len(), 2);

            let name_values: Vec<&str> =
                names.iter().map(|n| n["name"].as_str().unwrap()).collect();
            assert!(name_values.contains(&"Charlie"));
            assert!(name_values.contains(&"David"));
            assert!(!name_values.contains(&"Alice"));
            assert!(!name_values.contains(&"Bob"));

            // Verify all names have the correct server_id
            for name in names {
                assert_eq!(name["server_id"].as_str().unwrap(), "server2");
            }

            let snapshot_data = JsonApiResponseSnapshot::new(
                body_text,
                status,
                &headers,
                "api_v1_names_filtered_server2",
            );

            assert_yaml_snapshot!(snapshot_data);
        }

        #[tokio::test]
        async fn can_return_empty_list_for_nonexistent_server() {
            let state = setup().await.expect("Failed to setup test context");
            create_test_names_multiple_servers(&state.db).await;

            let name_state = create_name_state(state.db);
            let app = create_api_router(name_state);

            // Request names for non-existent server
            let request = Request::builder()
                .method(Method::GET)
                .uri("/names?server_id=nonexistent-server")
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

            // Parse and validate JSON structure
            let json: Value = serde_json::from_str(body_text).expect("Should be valid JSON");
            assert!(json["names"].is_array());
            assert_eq!(json["count"], 0);
            assert_eq!(json["names"].as_array().unwrap().len(), 0);

            let snapshot_data = JsonApiResponseSnapshot::new(
                body_text,
                status,
                &headers,
                "api_v1_names_filtered_nonexistent",
            );

            assert_yaml_snapshot!(snapshot_data);
        }

        #[tokio::test]
        async fn can_return_all_names_when_no_server_filter() {
            let state = setup().await.expect("Failed to setup test context");
            create_test_names_multiple_servers(&state.db).await;

            let name_state = create_name_state(state.db);
            let app = create_api_router(name_state);

            // Request all names without filter
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

            // Parse and validate JSON structure
            let json: Value = serde_json::from_str(body_text).expect("Should be valid JSON");
            assert!(json["names"].is_array());
            assert_eq!(json["count"], 4); // All names from both servers

            // Validate that all names are returned
            let names = json["names"].as_array().unwrap();
            assert_eq!(names.len(), 4);

            let name_values: Vec<&str> =
                names.iter().map(|n| n["name"].as_str().unwrap()).collect();
            assert!(name_values.contains(&"Alice"));
            assert!(name_values.contains(&"Bob"));
            assert!(name_values.contains(&"Charlie"));
            assert!(name_values.contains(&"David"));

            let snapshot_data = JsonApiResponseSnapshot::new(
                body_text,
                status,
                &headers,
                "api_v1_names_all_servers",
            );

            assert_yaml_snapshot!(snapshot_data);
        }

        #[tokio::test]
        async fn can_handle_empty_server_id_parameter() {
            let state = setup().await.expect("Failed to setup test context");
            create_test_names_multiple_servers(&state.db).await;

            let name_state = create_name_state(state.db);
            let app = create_api_router(name_state);

            // Request names with empty server_id
            let request = Request::builder()
                .method(Method::GET)
                .uri("/names?server_id=")
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

            // Parse and validate JSON structure
            let json: Value = serde_json::from_str(body_text).expect("Should be valid JSON");
            assert!(json["names"].is_array());
            assert_eq!(json["count"], 0); // Empty string should match no servers
            assert_eq!(json["names"].as_array().unwrap().len(), 0);

            let snapshot_data = JsonApiResponseSnapshot::new(
                body_text,
                status,
                &headers,
                "api_v1_names_empty_server_id",
            );

            assert_yaml_snapshot!(snapshot_data);
        }
    }
}

#[tokio::test]
async fn can_serve_bulk_add_form() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .uri("/names/bulk-add")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot_data = HttpResponseSnapshot::new(body_text, status, &headers, "bulk_add_form");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_process_bulk_add_with_valid_yaml() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let yaml_content = "123456789: TestUser1\n987654321: TestUser2\n111222333: TestUser3";
    let form_data = format!("server_id=test-server-1&yaml_content={}", yaml_content);

    let request = Request::builder()
        .method(Method::POST)
        .uri("/names/bulk-add")
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
        "bulk_add_success_with_valid_yaml",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_handle_bulk_add_with_some_duplicate_entries() {
    let state = setup().await.expect("Failed to setup test context");

    // Create some existing entries
    create_test_names(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    // YAML that includes one existing entry (123456789) and two new ones
    let yaml_content = "123456789: TestUser1\n555666777: NewUser1\n888999000: NewUser2";
    let form_data = format!("server_id=test-server-1&yaml_content={}", yaml_content);

    let request = Request::builder()
        .method(Method::POST)
        .uri("/names/bulk-add")
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
        "bulk_add_success_with_duplicates",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_handle_bulk_add_with_invalid_yaml() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    // Invalid YAML content
    let yaml_content = "invalid: yaml: content: [unclosed";
    let form_data = format!("server_id=test-server-1&yaml_content={}", yaml_content);

    let request = Request::builder()
        .method(Method::POST)
        .uri("/names/bulk-add")
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

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "bulk_add_with_invalid_yaml");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_handle_bulk_add_with_empty_yaml() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    // Empty YAML content
    let yaml_content = "";
    let form_data = format!("server_id=test-server-1&yaml_content={}", yaml_content);

    let request = Request::builder()
        .method(Method::POST)
        .uri("/names/bulk-add")
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

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "bulk_add_with_empty_yaml");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_bulk_delete_selected_names() {
    let state = setup().await.expect("Failed to setup test context");
    let test_ids = create_test_names_with_ids(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    // Select first two names for deletion using query parameters
    let selected_ids = [test_ids[0], test_ids[1]];
    let query_params = selected_ids
        .iter()
        .map(|id| format!("selected_ids={}", id))
        .collect::<Vec<_>>()
        .join("&");

    let request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/names/delete?{}", query_params))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Debug print for troubleshooting
    println!("Status: {}", status);
    println!("Body: {}", body_text);

    // Should return updated table with remaining names
    assert!(status.is_success());

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "bulk_delete_selected_names");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_handle_bulk_delete_with_no_selection() {
    let state = setup().await.expect("Failed to setup test context");
    create_test_names(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    // No selected IDs (empty query parameters)
    let request = Request::builder()
        .method(Method::DELETE)
        .uri("/names/delete")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Should return the table with all names still present
    assert!(status.is_success());
    assert!(body_text.contains("TestUser1"));
    assert!(body_text.contains("TestUser2"));

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "bulk_delete_no_selection");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_handle_bulk_delete_with_nonexistent_ids() {
    let state = setup().await.expect("Failed to setup test context");
    let test_ids = create_test_names_with_ids(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    // Include some nonexistent IDs along with valid ones using query parameters
    let valid_id = test_ids[0];
    let invalid_ids = [99999, 88888];
    let mut selected_ids = vec![valid_id];
    selected_ids.extend_from_slice(&invalid_ids);

    let query_params = selected_ids
        .iter()
        .map(|id| format!("selected_ids={}", id))
        .collect::<Vec<_>>()
        .join("&");

    let request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/names/delete?{}", query_params))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Should successfully delete the valid ID and ignore invalid ones
    assert!(status.is_success());

    let snapshot_data = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "bulk_delete_with_nonexistent_ids",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_bulk_delete_all_names() {
    let state = setup().await.expect("Failed to setup test context");
    let test_ids = create_test_names_with_ids(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    // Select all name IDs for deletion using query parameters
    let query_params = test_ids
        .iter()
        .map(|id| format!("selected_ids={}", id))
        .collect::<Vec<_>>()
        .join("&");

    let request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/names/delete?{}", query_params))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Should return empty table message
    assert!(status.is_success());
    assert!(body_text.contains("No names found in the database"));

    let snapshot_data =
        HttpResponseSnapshot::new(body_text, status, &headers, "bulk_delete_all_names");

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn bulk_delete_endpoint_returns_correct_content_type() {
    let state = setup().await.expect("Failed to setup test context");
    let test_ids = create_test_names_with_ids(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let query_params = format!("selected_ids={}", test_ids[0]);

    let request = Request::builder()
        .method(Method::DELETE)
        .uri(format!("/names/delete?{}", query_params))
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
        "bulk_delete_content_type_check",
    );

    assert_yaml_snapshot!(snapshot_data);
}

#[tokio::test]
async fn can_serve_bulk_delete_page() {
    let state = setup().await.expect("Failed to setup test context");

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/names/delete")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    assert_eq!(status, StatusCode::OK);
    assert!(body_text.contains("Bulk Delete Names"));
    assert!(body_text.contains("Select Names to Delete"));
    assert!(
        headers
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/html")
    );
}

#[tokio::test]
async fn can_serve_bulk_delete_table_fragment() {
    let state = setup().await.expect("Failed to setup test context");
    create_test_names(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/names/delete/table")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    assert_eq!(status, StatusCode::OK);
    assert!(body_text.contains("Delete Selected"));
    assert!(body_text.contains("TestUser1"));
    assert!(body_text.contains("TestUser2"));
    // Should not contain individual edit/delete buttons (simplified view)
    assert!(!body_text.contains("Edit"));
    assert!(!body_text.contains("hx-delete=\"/names/1\""));
    assert!(!body_text.contains("hx-delete=\"/names/2\""));
    assert!(
        headers
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/html")
    );
}

#[tokio::test]
async fn bulk_delete_table_fragment_has_correct_form_structure() {
    let state = setup().await.expect("Failed to setup test context");
    create_test_names(&state.db).await;

    let name_state = create_name_state(state.db);
    let app = create_name_router(name_state);

    let request = Request::builder()
        .method(Method::GET)
        .uri("/names/delete/table")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    // Check for proper form structure
    assert!(body_text.contains("id=\"bulk-delete-form\""));
    assert!(body_text.contains("hx-delete=\"/names/delete\""));
    assert!(body_text.contains("hx-target=\"#bulk-delete-table\""));
    assert!(body_text.contains("name=\"selected_ids\""));
    assert!(body_text.contains("id=\"select-all\""));
    assert!(body_text.contains("id=\"delete-selected-btn\""));
    assert!(body_text.contains("toggleAllCheckboxes"));
    assert!(body_text.contains("updateDeleteButton"));
}
