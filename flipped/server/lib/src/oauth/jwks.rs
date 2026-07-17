use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};

use super::OAuthState;

pub async fn jwks(State(state): State<OAuthState>) -> Response {
    let mut response = axum::Json(state.application.credentials().jwks()).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public,max-age=60"),
    );
    response
}
