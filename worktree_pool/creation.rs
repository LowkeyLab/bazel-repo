//! Pure creation facts. Registration and preparation remain durable across interruption.
use serde::{Deserialize, Serialize};

use crate::{
    acquisition::Assignment,
    domain::CatalogProjection,
    error::PoolError,
    management::{OperationId, RepositoryId, Worktree, WorktreeId},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreationState {
    Reserved,
    PathPrepared,
    Completed,
    NeedsReconciliation,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationOperation {
    pub operation_id: OperationId,
    pub repository_id: RepositoryId,
    pub worktree_id: WorktreeId,
    pub assignment_handle: crate::management::AssignmentHandle,
    pub acquisition_operation_id: OperationId,
    pub state: CreationState,
    pub last_checkpoint: String,
    pub intent_event_id: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CreationEvent {
    Registered {
        operation_id: OperationId,
        worktree: Box<Worktree>,
        assignment: Box<Assignment>,
    },
    PathPrepared {
        operation_id: OperationId,
        repository_id: RepositoryId,
        worktree_id: WorktreeId,
    },
    Finished {
        operation_id: OperationId,
        repository_id: RepositoryId,
        worktree_id: WorktreeId,
        successful: bool,
    },
}
impl CreationEvent {
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Registered { .. } => "io.lowkeylab.worktreepool.worktree.creation.registered.v1",
            Self::PathPrepared { .. } => {
                "io.lowkeylab.worktreepool.worktree.creation.path.prepared.v1"
            }
            Self::Finished { .. } => "io.lowkeylab.worktreepool.worktree.creation.finished.v1",
        }
    }
    #[must_use]
    pub const fn operation_id(&self) -> OperationId {
        match self {
            Self::Registered { operation_id, .. }
            | Self::PathPrepared { operation_id, .. }
            | Self::Finished { operation_id, .. } => *operation_id,
        }
    }
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        match self {
            Self::Registered { worktree, .. } => worktree.worktree_id,
            Self::PathPrepared { worktree_id, .. } | Self::Finished { worktree_id, .. } => {
                *worktree_id
            }
        }
    }
    #[must_use]
    pub const fn repository_id(&self) -> Option<RepositoryId> {
        match self {
            Self::Registered { .. } => None,
            Self::PathPrepared { repository_id, .. } | Self::Finished { repository_id, .. } => {
                Some(*repository_id)
            }
        }
    }
    /// # Errors
    /// Rejects inconsistent registration, identity, causation or preparation order.
    pub fn apply(
        &self,
        state: &mut CatalogProjection,
        event_id: &str,
        causation: Option<&str>,
    ) -> Result<(), PoolError> {
        if let Self::Registered {
            operation_id,
            worktree,
            assignment,
        } = self
        {
            if worktree.worktree_id != assignment.worktree_id
                || worktree.repository_id != assignment.repository_id
                || worktree.path != assignment.path
                || *operation_id == assignment.operation_id
                || state.creations.iter().any(|o| {
                    o.operation_id == *operation_id || o.worktree_id == worktree.worktree_id
                })
            {
                return Err(PoolError::Conflict);
            }
            crate::management::enroll_worktree(state, worktree)?;
            state.creations.push(CreationOperation {
                operation_id: *operation_id,
                repository_id: worktree.repository_id,
                worktree_id: worktree.worktree_id,
                assignment_handle: assignment.assignment_handle,
                acquisition_operation_id: assignment.operation_id,
                state: CreationState::Reserved,
                last_checkpoint: "creation_registered".into(),
                intent_event_id: event_id.into(),
            });
            return Ok(());
        }
        let operation = state
            .creations
            .iter_mut()
            .find(|o| {
                o.operation_id == self.operation_id()
                    && Some(o.repository_id) == self.repository_id()
                    && o.worktree_id == self.worktree_id()
            })
            .ok_or(PoolError::Corrupt)?;
        if causation != Some(operation.intent_event_id.as_str()) {
            return Err(PoolError::Conflict);
        }
        match self {
            Self::PathPrepared { .. } if operation.state == CreationState::Reserved => {
                operation.state = CreationState::PathPrepared;
                operation.last_checkpoint = "creation_path_prepared".into();
            }
            Self::Finished { successful, .. }
                if matches!(
                    operation.state,
                    CreationState::Reserved | CreationState::PathPrepared
                ) =>
            {
                if *successful && operation.state != CreationState::PathPrepared {
                    return Err(PoolError::Conflict);
                }
                operation.state = if *successful {
                    CreationState::Completed
                } else {
                    CreationState::NeedsReconciliation
                };
                operation.last_checkpoint = if *successful {
                    "creation_committed"
                } else {
                    "creation_uncertain"
                }
                .into();
            }
            _ => return Err(PoolError::Conflict),
        }
        Ok(())
    }
}
