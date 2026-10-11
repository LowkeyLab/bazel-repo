#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("unsafe package path: {0:?}")]
    PackagePath(String),
    #[error("invalid lowercase SHA256 digest: {0:?}")]
    Sha256(String),
    #[error("{0}")]
    Policy(String),
    #[error("cannot access {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("invalid JSON contract {path}: {source}")]
    Json {
        path: std::path::PathBuf,
        source: serde_json::Error,
    },
}
