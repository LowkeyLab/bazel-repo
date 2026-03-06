use serde::Deserialize;

/// JWT claims from a Casdoor-issued token.
#[derive(Debug, Deserialize)]
pub struct Claims {
    /// Subject (Casdoor user ID).
    pub sub: String,
    /// Issuer URL.
    pub iss: String,
    /// Token expiration (unix timestamp).
    pub exp: u64,
    /// Token issued-at (unix timestamp).
    pub iat: u64,
    /// User display name (optional).
    pub name: Option<String>,
    /// User email (optional).
    pub email: Option<String>,
}

/// Errors during authentication.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Authentication required")]
    MissingToken,
    #[error("Invalid authentication token")]
    InvalidToken,
}

/// Trait abstracting authentication validation.
///
/// Implementors validate a raw HTTP `Authorization` header value and
/// return decoded [`Claims`] on success.
#[trait_variant::make(Send)]
pub trait AuthService: Send + Sync {
    /// Validates an `Authorization` header value (e.g. `"Bearer <token>"`)
    /// and returns the decoded claims.
    async fn validate_auth_header(&self, header_value: &str) -> Result<Claims, AuthError>;
}

/// Test double that accepts any token and returns fixed claims.
pub struct AlwaysAllow;

impl AuthService for AlwaysAllow {
    async fn validate_auth_header(&self, _header_value: &str) -> Result<Claims, AuthError> {
        Ok(Claims {
            sub: "test-user".to_string(),
            iss: "test-issuer".to_string(),
            exp: 0,
            iat: 0,
            name: None,
            email: None,
        })
    }
}

/// Test double that rejects every token.
pub struct AlwaysDeny;

impl AuthService for AlwaysDeny {
    async fn validate_auth_header(&self, _header_value: &str) -> Result<Claims, AuthError> {
        Err(AuthError::InvalidToken)
    }
}
