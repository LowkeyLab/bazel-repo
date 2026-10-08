#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("filesystem access failed")]
    Io(#[from] std::io::Error),
    #[error("catalog storage failed")]
    Storage,
    #[error("catalog history or locator is corrupt")]
    Corrupt,
    #[error("catalog version or event type is unsupported")]
    Unsupported,
    #[error("catalog authority conflicts with the selected location")]
    Conflict,
    #[error("commit outcome requires reopen and identity inspection")]
    CommitUnknown,
    #[error("catalog is not initialized; run catalog init")]
    Missing,
    #[error("catalog already initialized; use catalog info")]
    AlreadyInitialized,
    #[error("initialization is pending; inspect catalog before reconciliation")]
    Pending,
    #[error("configuration is invalid")]
    Configuration,
    #[error("tool-owned storage must have user-only permissions")]
    Permissions,
}
impl PoolError {
    #[must_use]
    pub const fn reason_code(&self) -> &'static str {
        match self {
            Self::Io(_) => "filesystem_error",
            Self::Storage => "storage_error",
            Self::Corrupt => "catalog_corrupt",
            Self::Unsupported => "unsupported_version",
            Self::Conflict => "catalog_conflict",
            Self::CommitUnknown => "commit_unknown",
            Self::Missing => "catalog_missing",
            Self::AlreadyInitialized => "catalog_exists",
            Self::Pending => "initialization_pending",
            Self::Configuration => "invalid_configuration",
            Self::Permissions => "unsafe_permissions",
        }
    }
}
