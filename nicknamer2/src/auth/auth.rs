use std::sync::RwLock;

use anyhow::Context as _;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};

use auth_claims::{AuthError, Claims};

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
    /// validation is retried once.
    pub async fn validate_token_with_refresh(&self, token: &str) -> Result<Claims, AuthError> {
        match self.validate_token(token) {
            Ok(claims) => Ok(claims),
            Err(AuthError::InvalidToken) => {
                // Attempt a key refresh in case the OIDC provider rotated its keys.
                if self.refresh_keys().await.is_ok() {
                    self.validate_token(token)
                } else {
                    Err(AuthError::InvalidToken)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Extracts and validates a Bearer token from an auth header value.
    ///
    /// Per RFC 7235 the authentication scheme is case-insensitive, so both
    /// `Bearer` and `bearer` (and any other casing) are accepted.
    pub async fn validate_auth_header(&self, header_value: &str) -> Result<Claims, AuthError> {
        let token = if header_value.len() > 7 && header_value[..7].eq_ignore_ascii_case("bearer ") {
            &header_value[7..]
        } else {
            return Err(AuthError::InvalidToken);
        };
        self.validate_token_with_refresh(token).await
    }

    /// Secret used for test-only HMAC-based JWT signing and validation.
    const TEST_SECRET: &[u8] = b"nicknamer2-test-secret-do-not-use-in-production";
    const TEST_KID: &str = "test-kid";

    /// Creates a validator that accepts tokens minted by [`Self::mint_test_token`].
    ///
    /// # WARNING
    /// This bypasses real OIDC validation and must **never** be used in production.
    pub fn new_noop_for_testing() -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_aud = false;
        validation.validate_exp = false;
        validation.set_required_spec_claims::<&str>(&[]);
        Self {
            client: reqwest::Client::new(),
            jwks_url: String::new(),
            keys: RwLock::new(vec![JwkEntry {
                kid: Self::TEST_KID.to_string(),
                decoding_key: DecodingKey::from_secret(Self::TEST_SECRET),
            }]),
            validation,
        }
    }

    /// Mints a test JWT token that the noop validator will accept.
    pub fn mint_test_token() -> String {
        use jsonwebtoken::{EncodingKey, Header};

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(Self::TEST_KID.to_string());

        #[derive(serde::Serialize)]
        struct TestClaims {
            sub: &'static str,
            iss: &'static str,
            exp: u64,
            iat: u64,
        }

        let claims = TestClaims {
            sub: "test-user",
            iss: "test-issuer",
            // Far-future expiry; exp validation is disabled in the noop validator.
            exp: 9_999_999_999,
            iat: 0,
        };
        jsonwebtoken::encode(
            &header,
            &claims,
            &EncodingKey::from_secret(Self::TEST_SECRET),
        )
        .expect("test token encoding should never fail")
    }
}
