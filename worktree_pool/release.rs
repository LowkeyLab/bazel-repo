//! Pure release facts; replay changes ownership only after completed preservation.
use serde::{Deserialize, Serialize};

use crate::{
    acquisition::AssignmentState,
    domain::CatalogProjection,
    error::PoolError,
    management::{AssignmentHandle, OperationId, RepositoryId, WorktreeId},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseState {
    Intended,
    Preserved,
    Completed,
    NeedsReconciliation,
    Reconciled,
}
impl ReleaseState {
    #[must_use]
    pub const fn is_pending(self) -> bool {
        !matches!(self, Self::Completed | Self::Reconciled)
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseOperation {
    pub operation_id: OperationId,
    pub repository_id: RepositoryId,
    pub worktree_id: WorktreeId,
    pub assignment_handle: AssignmentHandle,
    pub state: ReleaseState,
    pub tip: String,
    pub branch: Option<String>,
    pub preservation_reference: Option<String>,
    pub intent_event_id: String,
    pub last_checkpoint: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ReleaseEvent {
    Started(ReleaseOperation),
    PreservationFinished {
        operation_id: OperationId,
        repository_id: RepositoryId,
        worktree_id: WorktreeId,
        successful: bool,
    },
    Finished {
        operation_id: OperationId,
        repository_id: RepositoryId,
        worktree_id: WorktreeId,
        successful: bool,
    },
}
impl ReleaseEvent {
    #[must_use]
    pub const fn repository_id(&self) -> RepositoryId {
        match self {
            Self::Started(o) => o.repository_id,
            Self::PreservationFinished { repository_id, .. }
            | Self::Finished { repository_id, .. } => *repository_id,
        }
    }
    #[must_use]
    pub const fn operation_id(&self) -> OperationId {
        match self {
            Self::Started(o) => o.operation_id,
            Self::PreservationFinished { operation_id, .. }
            | Self::Finished { operation_id, .. } => *operation_id,
        }
    }
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        match self {
            Self::Started(o) => o.worktree_id,
            Self::PreservationFinished { worktree_id, .. } | Self::Finished { worktree_id, .. } => {
                *worktree_id
            }
        }
    }
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Started(_) => "io.lowkeylab.worktreepool.assignment.release.started.v1",
            Self::PreservationFinished { .. } => {
                "io.lowkeylab.worktreepool.assignment.release.preservation.finished.v1"
            }
            Self::Finished { .. } => "io.lowkeylab.worktreepool.assignment.release.finished.v1",
        }
    }
    /// # Errors
    /// Rejects stale ownership, mismatched identities and invalid causal checkpoint order.
    pub fn apply(
        &self,
        state: &mut CatalogProjection,
        event_id: &str,
        cause: Option<&str>,
    ) -> Result<(), PoolError> {
        if let Self::Started(o) = self {
            if state.has_pending_recovery(o.repository_id) {
                return Err(PoolError::OperationPending);
            }
            if o.state != ReleaseState::Intended
                || !o.intent_event_id.is_empty()
                || o.last_checkpoint != "release_intended"
                || !matches!(o.tip.len(), 40 | 64)
                || !o
                    .tip
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || o.preservation_reference
                    != o.branch
                        .is_none()
                        .then(|| format!("refs/worktree-pool/{}", o.operation_id))
                || !state.assignments.iter().any(|a| {
                    a.assignment_handle == o.assignment_handle
                        && a.repository_id == o.repository_id
                        && a.worktree_id == o.worktree_id
                        && a.state == AssignmentState::Active
                })
                || state.releases.iter().any(|r| {
                    r.operation_id == o.operation_id
                        || (r.repository_id == o.repository_id && r.state.is_pending())
                })
            {
                return Err(PoolError::Conflict);
            }
            let mut o = o.clone();
            o.intent_event_id = event_id.into();
            state.releases.push(o);
            return Ok(());
        }
        let o = state
            .releases
            .iter_mut()
            .find(|o| {
                o.operation_id == self.operation_id()
                    && o.repository_id == self.repository_id()
                    && o.worktree_id == self.worktree_id()
            })
            .ok_or(PoolError::Corrupt)?;
        if cause != Some(o.intent_event_id.as_str()) {
            return Err(PoolError::Conflict);
        }
        match self {
            Self::PreservationFinished { successful, .. }
                if o.state == ReleaseState::Intended && o.branch.is_none() =>
            {
                o.state = if *successful {
                    ReleaseState::Preserved
                } else {
                    ReleaseState::NeedsReconciliation
                };
                o.last_checkpoint = if *successful {
                    "preservation_committed"
                } else {
                    "preservation_failed"
                }
                .into();
                o.intent_event_id = event_id.into();
            }
            Self::Finished { successful, .. }
                if o.state == ReleaseState::Preserved
                    || (o.state == ReleaseState::Intended && o.branch.is_some()) =>
            {
                o.state = if *successful {
                    ReleaseState::Completed
                } else {
                    ReleaseState::NeedsReconciliation
                };
                o.last_checkpoint = if *successful {
                    "release_committed"
                } else {
                    "release_state_uncertain"
                }
                .into();
                if *successful {
                    let a = state
                        .assignments
                        .iter_mut()
                        .find(|a| {
                            a.assignment_handle == o.assignment_handle
                                && a.state == AssignmentState::Active
                        })
                        .ok_or(PoolError::Conflict)?;
                    a.state = AssignmentState::Released;
                    let w = state
                        .worktrees
                        .iter_mut()
                        .find(|w| w.worktree_id == o.worktree_id)
                        .ok_or(PoolError::Corrupt)?;
                    w.last_release_position =
                        Some(state.revision.checked_add(1).ok_or(PoolError::Corrupt)?);
                }
            }
            _ => return Err(PoolError::Conflict),
        }
        Ok(())
    }
}
