use serde::Deserialize;

/// Application configuration loaded from environment variables.
#[derive(Deserialize, Debug)]
pub struct Config {
    /// PostgreSQL connection URL.
    pub db_url: String,
    /// Port to bind the HTTP server to.
    #[serde(default = "default_port")]
    pub port: u16,
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
