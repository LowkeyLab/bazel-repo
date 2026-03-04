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
