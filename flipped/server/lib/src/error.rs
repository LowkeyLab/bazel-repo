use flipped_anki::ImportErrorCode;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionErrorCode {
    NotFound,
    Unauthenticated,
    RoleForbidden,
    SessionExpired,
    InvalidState,
    InvalidCommandId,
    InvalidCursor,
    CommandCapacityExceeded,
    ResourceExhausted,
    CredentialVersionMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionApplicationError {
    pub code: SessionErrorCode,
    pub current_revision: u64,
}

impl SessionApplicationError {
    pub const fn new(code: SessionErrorCode, current_revision: u64) -> Self {
        Self {
            code,
            current_revision,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamErrorCode {
    InvalidCursor,
    ResyncRequired,
    SessionExpired,
    CredentialVersionMismatch,
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("configuration rejected: {0}")]
    Configuration(String),
    #[error("session operation rejected: {0:?}")]
    Session(SessionErrorCode),
    #[error("import rejected: {0:?}")]
    Import(ImportErrorCode),
    #[error("credential rejected")]
    Credential,
    #[error("service capacity exhausted")]
    ResourceExhausted,
    #[error("internal service failure")]
    Internal,
}

pub type Result<T> = std::result::Result<T, ServerError>;
