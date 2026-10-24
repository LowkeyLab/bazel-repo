//! Native coverage reports and shared build-event interpretation.
pub mod build_event;
#[path = "javascript_adapter.rs"]
pub mod javascript_report;
#[path = "jvm_adapter.rs"]
pub mod jvm_report;
pub mod native_report;

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ReportError>;

pub(crate) fn invalid(message: impl Into<String>) -> ReportError {
    ReportError::Invalid(message.into())
}
