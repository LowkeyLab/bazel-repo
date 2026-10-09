#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error(
        "registered capacity is exhausted; inspect ownership and withheld records or increase durable capacity"
    )]
    PoolExhausted {
        repository_id: crate::management::RepositoryId,
        maximum: u32,
        registered_count: usize,
        assigned_count: usize,
    },
    #[error(
        "maximum cannot be lower than current registered count; retire records explicitly first"
    )]
    CapacityBelowCount,
    #[error("assignment handle is unknown; inspect recorded identities")]
    UnknownAssignment,
    #[error("unfinished tracked, indexed, or untracked work is present")]
    UnfinishedWork,
    #[error("Git operation or required lock state is present")]
    GitOperation,
    #[error("index state cannot be certified for ordinary worktree reuse")]
    UnsupportedIndex,
    #[error("retained work would collide with the selected checkout")]
    RetainedCollision,
    #[error("no safe registered worktree is available")]
    Unavailable,
    #[error("Git observation failed; verify the selected repository")]
    Git,
    #[error("repository operation is pending; inspect before explicit reconciliation")]
    OperationPending,
    #[error("resource is not explicitly registered")]
    Unregistered,
    #[error("repository selectors conflict")]
    Selectors,
    #[error("registered worktree capacity is exhausted")]
    Capacity,
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
            Self::PoolExhausted { maximum: 0, .. } => "capacity_zero",
            Self::PoolExhausted {
                registered_count,
                assigned_count,
                ..
            } if *registered_count == *assigned_count => "capacity_all_assigned",
            Self::PoolExhausted { .. } => "capacity_no_safe_worktree",
            Self::CapacityBelowCount => "capacity_below_count",
            Self::UnknownAssignment => "assignment_unknown",
            Self::UnfinishedWork => "unfinished_work",
            Self::GitOperation => "git_operation_in_progress",
            Self::UnsupportedIndex => "unsupported_index_state",
            Self::RetainedCollision => "retained_file_collision",
            Self::Unavailable => "worktree_unavailable",
            Self::Git => "git_failed",
            Self::OperationPending => "operation_pending",
            Self::Unregistered => "resource_unregistered",
            Self::Selectors => "selector_conflict",
            Self::Capacity => "capacity_exhausted",
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
