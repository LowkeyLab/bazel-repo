//! Explicit retirement facts retain registration history and never perform cleanup.
use serde::{Deserialize, Serialize};

use crate::{
    acquisition::AssignmentState,
    domain::CatalogProjection,
    error::PoolError,
    management::{OperationId, RepositoryId, Worktree, WorktreeId},
    recovery::{RecoveryDisposition, RecoveryState},
    release::ReleaseState,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetirementState {
    Intended,
    Completed,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetirementOperation {
    pub operation_id: OperationId,
    pub repository_id: RepositoryId,
    pub worktree_id: WorktreeId,
    pub worktree: Worktree,
    pub reconciliation_id: OperationId,
    pub reconciliation_checkpoint: String,
    pub state: RetirementState,
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
pub enum RetirementEvent {
    Started(Box<RetirementOperation>),
    Finished {
        operation_id: OperationId,
        repository_id: RepositoryId,
        worktree_id: WorktreeId,
    },
}
impl RetirementEvent {
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Started(_) => "io.lowkeylab.worktreepool.worktree.retirement.started.v1",
            Self::Finished { .. } => "io.lowkeylab.worktreepool.worktree.retirement.finished.v1",
        }
    }
    #[must_use]
    pub const fn operation_id(&self) -> OperationId {
        match self {
            Self::Started(o) => o.operation_id,
            Self::Finished { operation_id, .. } => *operation_id,
        }
    }
    #[must_use]
    pub const fn repository_id(&self) -> Option<RepositoryId> {
        match self {
            Self::Started(o) => Some(o.repository_id),
            Self::Finished { .. } => None,
        }
    }
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        match self {
            Self::Started(o) => o.worktree_id,
            Self::Finished { worktree_id, .. } => *worktree_id,
        }
    }
    /// # Errors
    /// Rejects unsafe owners, stale proof, false binding and duplicate retirement facts.
    pub fn apply(
        &self,
        state: &mut CatalogProjection,
        event_id: &str,
        cause: Option<&str>,
    ) -> Result<(), PoolError> {
        match self {
            Self::Started(o) => {
                let w = state
                    .worktrees
                    .iter()
                    .find(|w| w.worktree_id == o.worktree_id)
                    .ok_or(PoolError::Unregistered)?;
                let checkpoint = eligible(state, w, o.reconciliation_id)?;
                if state.has_pending_recovery(o.repository_id)
                    || state.operation_identity_used(o.operation_id)
                    || &o.worktree != w
                    || o.repository_id != w.repository_id
                    || o.state != RetirementState::Intended
                    || !o.intent_event_id.is_empty()
                    || o.last_checkpoint != "retirement_intended"
                    || checkpoint != o.reconciliation_checkpoint
                    || cause != Some(checkpoint.as_str())
                {
                    return Err(PoolError::Conflict);
                }
                let mut o = o.as_ref().clone();
                o.intent_event_id = event_id.into();
                state.retirements.push(o);
            }
            Self::Finished {
                operation_id,
                repository_id,
                worktree_id,
            } => {
                let o = state
                    .retirements
                    .iter()
                    .find(|o| o.operation_id == *operation_id)
                    .cloned()
                    .ok_or(PoolError::Unregistered)?;
                if o.repository_id != *repository_id
                    || o.worktree_id != *worktree_id
                    || o.state != RetirementState::Intended
                    || cause != Some(o.intent_event_id.as_str())
                {
                    return Err(PoolError::Conflict);
                }
                let w = state
                    .worktrees
                    .iter()
                    .find(|w| w.worktree_id == *worktree_id)
                    .ok_or(PoolError::Unregistered)?;
                if w != &o.worktree
                    || eligible(state, w, o.reconciliation_id)? != o.reconciliation_checkpoint
                {
                    return Err(PoolError::Conflict);
                }
                state.worktrees.retain(|w| w.worktree_id != *worktree_id);
                state
                    .withheld_worktrees
                    .retain(|w| w.worktree_id != *worktree_id);
                let o = state
                    .retirements
                    .iter_mut()
                    .find(|o| o.operation_id == *operation_id)
                    .ok_or(PoolError::Corrupt)?;
                o.state = RetirementState::Completed;
                o.intent_event_id = event_id.into();
                o.last_checkpoint = "retirement_committed".into();
            }
        }
        Ok(())
    }
}

/// # Errors
/// Requires exact successful preservation/ownership reconciliation for the latest owner generation.
pub fn eligible(
    state: &CatalogProjection,
    w: &Worktree,
    proof: OperationId,
) -> Result<String, PoolError> {
    let latest = state
        .assignments
        .iter()
        .rev()
        .find(|a| a.worktree_id == w.worktree_id);
    if state
        .assignments
        .iter()
        .any(|a| a.worktree_id == w.worktree_id && a.state != AssignmentState::Released)
        || state
            .withheld_worktrees
            .iter()
            .any(|x| x.worktree_id == w.worktree_id)
    {
        return Err(PoolError::RetirementUnsafe);
    }
    if state
        .operations
        .iter()
        .any(|o| o.repository_id == w.repository_id && o.state.is_pending())
        || state
            .acquisitions
            .iter()
            .any(|o| o.repository_id == w.repository_id && o.state.is_pending())
        || state
            .releases
            .iter()
            .any(|o| o.repository_id == w.repository_id && o.state.is_pending())
        || state
            .creations
            .iter()
            .any(|o| o.repository_id == w.repository_id && o.state.is_pending())
        || state
            .recoveries
            .iter()
            .any(|o| o.repository_id == w.repository_id && o.state != RecoveryState::Completed)
        || state.repository_recoveries.iter().any(|o| {
            o.repository_id == w.repository_id
                && o.state != crate::repository_recovery::RepositoryRecoveryState::Completed
        })
    {
        return Err(PoolError::OperationPending);
    }
    if let Some(o) = state.releases.iter().find(|o| o.operation_id == proof)
        && o.repository_id == w.repository_id
        && o.worktree_id == w.worktree_id
        && o.state == ReleaseState::Completed
        && latest.is_some_and(|a| a.assignment_handle == o.assignment_handle)
    {
        return Ok(o.intent_event_id.clone());
    }
    if let Some(o) = state.recoveries.iter().find(|o| o.operation_id == proof) {
        let generation_matches = match o.disposition {
            RecoveryDisposition::Released => {
                latest.is_some_and(|a| Some(a.assignment_handle) == o.assignment_handle)
            }
            RecoveryDisposition::Unassigned => o.assignment_handle.is_none(),
            RecoveryDisposition::Active | RecoveryDisposition::Withheld => false,
        };
        if o.repository_id == w.repository_id
            && o.worktree_id == w.worktree_id
            && o.state == RecoveryState::Completed
            && generation_matches
        {
            return Ok(o.intent_event_id.clone());
        }
    }
    Err(PoolError::RetirementUnsafe)
}
