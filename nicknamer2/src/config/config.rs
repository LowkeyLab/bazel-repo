use serde::Deserialize;

/// Application configuration loaded from environment variables.
#[derive(Deserialize, Debug)]
pub struct Config {
    /// PostgreSQL connection URL.
    pub db_url: String,
    /// Port to bind the HTTP server to.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Casdoor issuer URL (e.g., "http://localhost:8000").
    #[serde(default = "default_casdoor_issuer_url")]
    pub casdoor_issuer_url: String,
    /// Casdoor application client ID.
    pub casdoor_client_id: String,
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
