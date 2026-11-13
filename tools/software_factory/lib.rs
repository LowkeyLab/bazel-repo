//! Inactive Software Factory intake and authenticated recovery boundary.
pub mod controller;
pub mod github;
pub mod history;
pub mod model;
pub mod policy;
pub mod store;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("safety stop: {0}")]
    Stop(String),
    #[error("private receipt I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid structured evidence: {0}")]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
///
/// # Errors
///
/// Returns a safety-stop error with the supplied reason when the condition is false.
pub fn require(condition: bool, reason: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Stop(reason.into()))
    }
}
