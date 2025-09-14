use askama::Template;
use axum::Router;
use axum::extract::{Extension, Form, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use axum_extra::extract::CookieJar;
use jsonwebtoken::encode;
use std::sync::Arc;

use crate::config::Config;

/// Represents the currently authenticated user.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub username: String,
}

impl CurrentUser {
    /// Creates a new CurrentUser instance.
    pub fn new(username: String) -> Self {
        Self { username }
    }
}

/// Authentication state containing admin credentials and JWT secret.
#[derive(Clone)]
pub struct AuthState {
    pub admin_username: String,
    pub admin_password: String,
    pub jwt_secret: String,
}

impl AuthState {
    /// Creates a new AuthState from the application config.
    pub fn from_config(config: &Config) -> Self {
        Self {
            admin_username: config.admin_username.clone(),
            admin_password: config.admin_password.clone(),
            jwt_secret: config.jwt_secret.clone(),
        }
    }
}

/// Creates a login router with authentication routes.
pub fn create_login_router(state: Arc<AuthState>) -> Router<()> {
    Router::new()
        .route("/login", axum::routing::post(login_handler))
        .route("/login", axum::routing::get(login_page_handler))
        .with_state(state)
}

/// Authentication middleware that checks for valid JWT tokens and sets CurrentUser extension.
/// This middleware only populates the CurrentUser extension and does not perform redirects.
pub async fn auth_user_middleware(
    State(state): State<Arc<AuthState>>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(token_cookie) = jar.get("auth_token") {
        if let Ok(claims) = decode_jwt(token_cookie.value(), &state.jwt_secret).await {
            let current_user = CurrentUser::new(claims.username);
            request.extensions_mut().insert(current_user);
        }
    }

    next.run(request).await
}

/// Login redirect middleware that redirects unauthenticated users to the login page.
/// This middleware should be applied after auth_user_middleware to check for CurrentUser extension.
pub async fn login_redirect_middleware(request: Request, next: Next) -> Response {
    // Check if user is authenticated by looking for CurrentUser extension
    let is_authenticated = request.extensions().get::<CurrentUser>().is_some();

    // If no valid authentication and accessing a protected route, redirect to login
    if !is_authenticated {
        return axum::response::Redirect::to("/login").into_response();
    }

    next.run(request).await
}

/// Represents the login request payload.
#[derive(serde::Deserialize, Debug)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Claims {
    pub exp: usize,       // Expiry time of the token
    pub iat: usize,       // Issued at time of the token
    pub username: String, // Username of the authenticated user
}
/// Custom error type for authentication operations.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    /// Represents an error during template rendering.
    /// The specific `askama::Error` is captured as the source of this error.
    #[error("Template rendering failed")]
    Template(#[from] askama::Error),
    /// Represents an error during JWT operations.
    /// The specific `jsonwebtoken::errors::Error` is captured as the source of this error.
    #[error("JWT operation failed")]
    JwtError,
}

impl axum::response::IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        let user_facing_error_message =
            "An unexpected error occurred while processing your request. Please try again later.";
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!(
                "<h1>Internal Server Error</h1><p>{}</p>",
                user_facing_error_message
            )),
        )
            .into_response()
    }
}

/// Handles the login request.
/// Checks submitted username and password against admin credentials.
/// If a user is already logged in, returns a success message.
pub async fn login_handler(
    State(state): State<Arc<AuthState>>,
    jar: CookieJar,
    current_user: Option<Extension<CurrentUser>>,
    Form(payload): Form<LoginRequest>,
) -> Result<(CookieJar, Response), AuthError> {
    // Check if user is already logged in
    if let Some(Extension(user)) = current_user {
        return handle_already_logged_in_user(jar, &user).await;
    }

    handle_login_attempt(state, jar, payload).await
}

/// Handles the case when a user is already logged in.
/// Returns a success response with the current user's information.
#[tracing::instrument(skip(jar))]
async fn handle_already_logged_in_user(
    jar: CookieJar,
    user: &CurrentUser,
) -> Result<(CookieJar, Response), AuthError> {
    let html = LoginSuccessTemplate {
        name: &user.username,
    }
    .render()
    .map_err(AuthError::from)?;

    Ok((jar, Html(html).into_response()))
}

/// Handles a login attempt when the user is not logged in.
/// Validates credentials and either returns success with JWT token or error response.
#[tracing::instrument(skip(state, jar, payload))]
async fn handle_login_attempt(
    state: Arc<AuthState>,
    jar: CookieJar,
    payload: LoginRequest,
) -> Result<(CookieJar, Response), AuthError> {
    if payload.username == state.admin_username && payload.password == state.admin_password {
        // Generate JWT token
        let jwt_token = encode_jwt(payload.username.clone(), &state.jwt_secret)
            .await
            .map_err(|_| AuthError::JwtError)?;

        // Create cookie with JWT token
        let cookie = axum_extra::extract::cookie::Cookie::build(("auth_token", jwt_token))
            .http_only(true)
            .secure(false) // Set to true in production with HTTPS
            .same_site(axum_extra::extract::cookie::SameSite::Lax)
            .max_age(time::Duration::hours(24))
            .path("/")
            .build();

        let updated_jar = jar.add(cookie);

        let html = LoginSuccessTemplate {
            name: &payload.username,
        }
        .render()
        .map_err(AuthError::from)?;

        Ok((updated_jar, Html(html).into_response()))
    } else {
        let error_message = LoginErrorMessageTemplate
            .render()
            .map_err(AuthError::from)?;

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("hx-retarget"),
            HeaderValue::from_static("#login-message"),
        );
        headers.insert(
            HeaderName::from_static("hx-reswap"),
            HeaderValue::from_static("outerHTML"),
        );

        let mut response = Html(error_message).into_response();
        response.headers_mut().extend(headers);
        Ok((jar, response))
    }
}

pub async fn encode_jwt(username: String, jwt_secret: &str) -> anyhow::Result<String> {
    let now = chrono::Utc::now();
    let expire = chrono::Duration::hours(24);
    let exp = (now + expire).timestamp() as usize;
    let iat = now.timestamp() as usize;
    let claims = Claims { exp, iat, username };
    let jwt = encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(jwt_secret.as_bytes()),
    )?;
    Ok(jwt)
}

#[allow(dead_code)]
pub async fn decode_jwt(token: &str, jwt_secret: &str) -> anyhow::Result<Claims> {
    let token_data = jsonwebtoken::decode(
        token,
        &jsonwebtoken::DecodingKey::from_secret(jwt_secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )?;
    Ok(token_data.claims)
}

#[derive(Template)]
#[template(path = "login/login_success.html")]
pub struct LoginSuccessTemplate<'a> {
    pub name: &'a str,
}

#[derive(Template)]
#[template(path = "login/login_error_message.html")]
pub struct LoginErrorMessageTemplate;

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub username: Option<String>,
}

/// Handles GET requests to display the login page.
#[tracing::instrument]
pub async fn login_page_handler(
    current_user: Option<Extension<CurrentUser>>,
) -> Result<Html<String>, AuthError> {
    let username = current_user.map(|Extension(user)| user.username);

    let template = LoginTemplate { username };
    template.render().map(Html).map_err(AuthError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[tokio::test]
    async fn auth_middlewares_work_together() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use axum::middleware::from_fn_with_state;
        use tower::ServiceExt;

        let config = Config {
            db_url: "".to_string(),
            port: 8080,
            admin_username: "admin".to_string(),
            admin_password: "password".to_string(),
            jwt_secret: "test_secret".to_string(),
        };

        let auth_state = Arc::new(AuthState::from_config(&config));

        // Create a test app with both middlewares in the correct order
        // Note: Layers are applied in reverse order (bottom to top)
        let app = axum::Router::new()
            .route(
                "/protected",
                axum::routing::get(|| async { "Protected content" }),
            )
            .layer(axum::middleware::from_fn(login_redirect_middleware))
            .layer(from_fn_with_state(auth_state.clone(), auth_user_middleware));

        // Test 1: Unauthenticated request should redirect to login
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/protected")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response.headers().get("location").unwrap();
        assert_eq!(location, "/login");

        // Test 2: Authenticated request should allow access
        let jwt_token = encode_jwt("admin".to_string(), &config.jwt_secret)
            .await
            .unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/protected")
                    .header("cookie", format!("auth_token={}", jwt_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "Protected content");
    }
}

pub mod api;
