use serde::Deserialize;

/// Application configuration loaded from environment variables.
#[derive(Deserialize, Debug)]
pub struct Config {
    /// PostgreSQL connection URL.
    pub db_url: String,
    /// Port to bind the HTTP server to.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Casdoor issuer URL used for JWT `iss` claim validation
    /// (e.g., "http://localhost:8000").
    #[serde(default = "default_casdoor_issuer_url")]
    pub casdoor_issuer_url: String,
    /// Optional override for the JWKS endpoint URL. When set, JWKS keys are
    /// fetched from this URL instead of `{casdoor_issuer_url}/.well-known/jwks`.
    /// Useful when the backend reaches the OIDC provider via an internal network
    /// address that differs from the public issuer URL in tokens.
    #[serde(default)]
    pub casdoor_jwks_url: Option<String>,
    /// Casdoor application client ID. When absent, the server starts without
    /// OIDC validation — mutations will reject all requests as unauthenticated.
    #[serde(default)]
    pub casdoor_client_id: Option<String>,
    /// Directory containing the Angular frontend build output.
    /// When set, the server serves these static files and falls back to
    /// index.html for client-side routing.
    #[serde(default)]
    pub static_dir: Option<String>,
}

impl Config {
    /// Loads configuration from environment variables.
    pub fn from_env() -> anyhow::Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::Environment::default())
            .build()?;

        let config: Config = settings.try_deserialize()?;
        Ok(config)
    }
}

fn default_port() -> u16 {
    8080
}

fn default_casdoor_issuer_url() -> String {
    "http://localhost:8000".to_string()
}
