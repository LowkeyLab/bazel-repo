use axum::Router;
use axum::body::Body;
use axum::http::Request;
use insta::assert_yaml_snapshot;
use nicknamer_server::web::{call_to_action_handler, health_check_handler, welcome_handler};
use tower::ServiceExt;

mod common;

use common::HttpResponseSnapshot;

/// Create a router for testing web endpoints.
/// This function creates a minimal router with just the public routes needed for testing.
fn create_test_router() -> Router {
    Router::new()
        .route("/health", axum::routing::get(health_check_handler))
        .route("/", axum::routing::get(welcome_handler))
        .route(
            "/call-to-action",
            axum::routing::get(call_to_action_handler),
        )
}

#[tokio::test]
async fn can_render_welcome_page() {
    let app = create_test_router();

    let request = Request::builder().uri("/").body(Body::empty()).unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot = HttpResponseSnapshot::new(body_text, status, &headers, "welcome_page");
    assert_yaml_snapshot!(snapshot);
}

#[tokio::test]
async fn can_render_call_to_action_for_unauthenticated_user() {
    let app = create_test_router();

    let request = Request::builder()
        .uri("/call-to-action")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot = HttpResponseSnapshot::new(
        body_text,
        status,
        &headers,
        "call_to_action_unauthenticated",
    );
    assert_yaml_snapshot!(snapshot);
}

#[tokio::test]
async fn can_check_health_endpoint() {
    let app = create_test_router();

    let request = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    let snapshot = HttpResponseSnapshot::new(body_text, status, &headers, "health_check");
    assert_yaml_snapshot!(snapshot);
}
