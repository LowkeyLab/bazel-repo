mod jwks;
mod metadata;
mod token;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Router;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::application::Application;
use crate::observability::{TraceContext, scope_trace};

#[derive(Clone)]
pub struct OAuthState {
    pub application: Arc<Application>,
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub readiness: Arc<AtomicBool>,
}

pub fn router(state: OAuthState) -> Router {
    Router::new()
        .route("/oauth/token", post(token::exchange))
        .route(
            "/.well-known/oauth-authorization-server",
            get(metadata::authorization_server_metadata),
        )
        .route("/oauth/jwks.json", get(jwks::jwks))
        .route("/health/live", get(|| async { "ok" }))
        .route("/health/ready", get(readiness))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(middleware::from_fn_with_state(
            Arc::new(Semaphore::new(128)),
            network_controls,
        ))
        .with_state(state)
}

async fn readiness(State(state): State<OAuthState>) -> (StatusCode, &'static str) {
    if state.readiness.load(Ordering::Acquire) {
        (StatusCode::OK, "ok")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready")
    }
}

async fn network_controls(
    State(concurrency): State<Arc<Semaphore>>,
    request: Request,
    next: Next,
) -> Response {
    let trace_context = TraceContext::from_headers(request.headers());
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::now_v7().to_string());
    let permit = match tokio::time::timeout(
        std::time::Duration::from_secs(1),
        concurrency.acquire_owned(),
    )
    .await
    {
        Ok(Ok(permit)) => permit,
        _ => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        scope_trace(trace_context, next.run(request)),
    )
    .await;
    drop(permit);
    let mut response = match result {
        Ok(response) => response,
        Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
    };
    if let Ok(request_id) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", request_id);
    }
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}
