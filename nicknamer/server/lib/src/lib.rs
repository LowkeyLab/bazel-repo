pub mod config {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    pub struct Config {
        pub db_url: String,
        #[serde(default = "default_port")]
        pub port: u16,
        pub admin_username: String,
        pub admin_password: String,
        pub jwt_secret: String,
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
}
pub mod entities;
pub mod name;

pub mod auth;
pub mod web;
