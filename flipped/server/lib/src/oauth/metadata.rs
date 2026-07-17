use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use super::OAuthState;

#[derive(Serialize)]
struct Metadata {
    issuer: String,
    token_endpoint: String,
    jwks_uri: String,
    grant_types_supported: [&'static str; 1],
    token_endpoint_auth_methods_supported: [&'static str; 1],
    scopes_supported: [&'static str; 1],
}

pub async fn authorization_server_metadata(State(state): State<OAuthState>) -> Response {
    let issuer = state.issuer.trim_end_matches('/');
    let metadata = Metadata {
        issuer: state.issuer.clone(),
        token_endpoint: format!("{issuer}/oauth/token"),
        jwks_uri: format!("{issuer}/oauth/jwks.json"),
        grant_types_supported: ["urn:ietf:params:oauth:grant-type:token-exchange"],
        token_endpoint_auth_methods_supported: ["client_secret_basic"],
        scopes_supported: ["session:examine"],
    };
    let mut response = axum::Json(metadata).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public,max-age=60"),
    );
    response
}
