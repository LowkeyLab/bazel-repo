use std::future::Future;
use std::pin::Pin;
use std::sync::RwLock;

use anyhow::Context as _;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};

use auth_claims::{AuthError, AuthService, Claims};

struct JwkEntry {
    kid: String,
    decoding_key: DecodingKey,
}

/// Validates JWTs against a cached JWKS keyset from an OIDC provider.
pub struct JwksValidator {
    client: reqwest::Client,
    jwks_url: String,
    keys: RwLock<Vec<JwkEntry>>,
    validation: Validation,
}

impl JwksValidator {
    /// Creates a new validator and fetches the initial JWKS keyset.
    pub async fn new(issuer_url: &str, client_id: &str) -> anyhow::Result<Self> {
        let jwks_url = format!("{}/.well-known/jwks", issuer_url.trim_end_matches('/'));
        let client = reqwest::Client::new();

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[client_id]);
        validation.set_issuer(&[issuer_url]);

        let validator = Self {
            client,
            jwks_url,
            keys: RwLock::new(Vec::new()),
            validation,
        };

        validator.refresh_keys().await?;
        Ok(validator)
    }

    /// Fetches the JWKS keyset from the OIDC provider and caches the keys.
    pub async fn refresh_keys(&self) -> anyhow::Result<()> {
        let jwks: jsonwebtoken::jwk::JwkSet = self
            .client
            .get(&self.jwks_url)
            .send()
            .await
            .context("Failed to fetch JWKS")?
            .json()
            .await
            .context("Failed to parse JWKS")?;

        let entries: Vec<JwkEntry> = jwks
            .keys
            .iter()
            .filter_map(|jwk| {
                let kid = jwk.common.key_id.clone()?;
                let key = DecodingKey::from_jwk(jwk).ok()?;
                Some(JwkEntry {
                    kid,
                    decoding_key: key,
                })
            })
            .collect();

        tracing::info!("Loaded {} JWKS keys", entries.len());
        *self.keys.write().unwrap() = entries;
        Ok(())
    }

    /// Validates a JWT token and returns the decoded claims.
    pub fn validate_token(&self, token: &str) -> Result<Claims, AuthError> {
        let header = decode_header(token).map_err(|e| {
            tracing::debug!("JWT decode_header failed: {e}");
            AuthError::InvalidToken
        })?;
        let kid = header.kid.as_deref().ok_or(AuthError::InvalidToken)?;

        let keys = self.keys.read().unwrap();
        let entry = keys
            .iter()
            .find(|e| e.kid == kid)
            .ok_or(AuthError::InvalidToken)?;

        let token_data =
            decode::<Claims>(token, &entry.decoding_key, &self.validation).map_err(|e| {
                tracing::debug!("JWT decode failed: {e}");
                AuthError::InvalidToken
            })?;

        Ok(token_data.claims)
    }

    /// Validates a JWT token, refreshing the JWKS keyset once on a key-not-found miss.
    ///
    /// This handles OIDC key rotation gracefully: if the `kid` in the token header is not
    /// found in the local cache, the cache is refreshed from the OIDC provider and the
    /// validation is retried once. Other validation failures (bad signature, expired, etc.)
    /// do **not** trigger a refresh to avoid unnecessary outbound requests.
    pub async fn validate_token_with_refresh(&self, token: &str) -> Result<Claims, AuthError> {
        // Pre-check: does the token's kid exist in our cache?
        let kid_missing = match decode_header(token) {
            Ok(header) => match header.kid.as_deref() {
                Some(kid) => {
                    let keys = self.keys.read().unwrap();
                    !keys.iter().any(|e| e.kid == kid)
                }
                None => false, // No kid in token — let validate_token handle the error
            },
            Err(_) => false, // Malformed header — let validate_token handle the error
        };

        if kid_missing {
            // Unknown kid — refresh keys once in case the OIDC provider rotated.
            tracing::debug!("Unknown kid in token, refreshing JWKS keys");
            if self.refresh_keys().await.is_ok() {
                return self.validate_token(token);
            }
            return Err(AuthError::InvalidToken);
        }

        self.validate_token(token)
    }

    /// Creates a validator with an empty keyset that rejects **all** tokens.
    ///
    /// Used when no Casdoor client ID is configured — the server still starts,
    /// but any authenticated mutation will fail with `InvalidToken`.
    pub fn new_noop_rejecting() -> Self {
        Self {
            client: reqwest::Client::new(),
            jwks_url: String::new(),
            keys: RwLock::new(Vec::new()),
            validation: Validation::new(Algorithm::RS256),
        }
    }
}

impl AuthService for JwksValidator {
    fn validate_auth_header<'a>(
        &'a self,
        header_value: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Claims, AuthError>> + Send + 'a>> {
        Box::pin(async move {
            let token =
                if header_value.len() > 7 && header_value[..7].eq_ignore_ascii_case("bearer ") {
                    &header_value[7..]
                } else {
                    return Err(AuthError::InvalidToken);
                };
            self.validate_token_with_refresh(token).await
        })
    }
}
