use axum::Router;
use axum::body::Body;
use axum::http::Request;
use googletest::prelude::*;
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

#[googletest::test]
#[tokio::test]
async fn can_handle_template_error_with_internal_server_error() {
    use axum::http::StatusCode;
    use nicknamer_server::web::WebError;

    // Simulate a template rendering error using askama::Error::Custom
    let custom_error_message = "Simulated template rendering failure".to_string();
    let template_error = askama::Error::Custom(custom_error_message.into());

    let web_error = WebError::Template(template_error);
    let response = axum::response::IntoResponse::into_response(web_error);

    assert_that!(response.status(), eq(StatusCode::INTERNAL_SERVER_ERROR));

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_text = std::str::from_utf8(&body).unwrap();

    assert_that!(
        body_text,
        eq(
            "<h1>Internal Server Error</h1><p>An unexpected error occurred while processing your request. Please try again later.</p>"
        )
    );
}
