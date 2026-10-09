//! Pure recovery plans and causal facts; replay never invokes external effects.
use serde::{Deserialize, Serialize};

use crate::{
    acquisition::{AcquisitionOperation, AcquisitionState, AssignmentState, WithheldWorktree},
    creation::CreationState,
    domain::CatalogProjection,
    error::PoolError,
    management::{AssignmentHandle, OperationId, RepositoryId, WorktreeId},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryState {
    Intended,
    Preserved,
    Completed,
    NeedsReconciliation,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDisposition {
    #[default]
    Released,
    Active,
    Withheld,
    Unassigned,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryOperation {
    pub operation_id: OperationId,
    pub repository_id: RepositoryId,
    pub worktree_id: WorktreeId,
    pub assignment_handle: Option<AssignmentHandle>,
    pub state: RecoveryState,
    pub tip: Option<String>,
    pub branch: Option<String>,
    pub preservation_reference: Option<String>,
    pub intent_event_id: String,
    pub last_checkpoint: String,
    #[serde(default)]
    pub target_operation_id: Option<OperationId>,
    #[serde(default)]
    pub disposition: RecoveryDisposition,
    #[serde(default)]
    pub protected_reason: Option<String>,
}
impl RecoveryOperation {
    #[must_use]
    pub fn requires_preservation(&self) -> bool {
        self.disposition != RecoveryDisposition::Withheld && self.branch.is_none()
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RecoveryEvent {
    Started(RecoveryOperation),
    Preserved {
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
impl RecoveryEvent {
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Started(_) => "io.lowkeylab.worktreepool.recovery.started.v1",
            Self::Preserved { .. } => "io.lowkeylab.worktreepool.recovery.preserved.v1",
            Self::Finished { .. } => "io.lowkeylab.worktreepool.recovery.finished.v1",
        }
    }
    #[must_use]
    pub const fn operation_id(&self) -> OperationId {
        match self {
            Self::Started(o) => o.operation_id,
            Self::Preserved { operation_id, .. } | Self::Finished { operation_id, .. } => {
                *operation_id
            }
        }
    }
    #[must_use]
    pub const fn repository_id(&self) -> RepositoryId {
        match self {
            Self::Started(o) => o.repository_id,
            Self::Preserved { repository_id, .. } | Self::Finished { repository_id, .. } => {
                *repository_id
            }
        }
    }
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        match self {
            Self::Started(o) => o.worktree_id,
            Self::Preserved { worktree_id, .. } | Self::Finished { worktree_id, .. } => {
                *worktree_id
            }
        }
    }
    /// # Errors
    /// Rejects inconsistent identities, forged causation or unpreserved ownership transitions.
    pub fn apply(
        &self,
        state: &mut CatalogProjection,
        event_id: &str,
        cause: Option<&str>,
    ) -> Result<(), PoolError> {
        if let Self::Started(o) = self {
            return start(state, o, event_id, cause);
        }
        let index = state
            .recoveries
            .iter()
            .position(|o| {
                o.operation_id == self.operation_id()
                    && o.repository_id == self.repository_id()
                    && o.worktree_id == self.worktree_id()
            })
            .ok_or(PoolError::Corrupt)?;
        let mut o = state.recoveries[index].clone();
        if cause != Some(o.intent_event_id.as_str()) {
            return Err(PoolError::Conflict);
        }
        match self {
            Self::Preserved { .. }
                if o.state == RecoveryState::Intended && o.requires_preservation() =>
            {
                o.state = RecoveryState::Preserved;
                o.last_checkpoint = "recovery_preserved".into();
            }
            Self::Finished { successful, .. }
                if o.state == RecoveryState::Preserved
                    || o.state == RecoveryState::NeedsReconciliation
                    || (o.state == RecoveryState::Intended && !o.requires_preservation()) =>
            {
                if *successful {
                    finish(state, &o)?;
                }
                o.state = if *successful {
                    RecoveryState::Completed
                } else {
                    RecoveryState::NeedsReconciliation
                };
                o.last_checkpoint = if *successful {
                    "recovery_committed"
                } else {
                    "recovery_state_uncertain"
                }
                .into();
            }
            _ => return Err(PoolError::Conflict),
        }
        o.intent_event_id = event_id.into();
        state.recoveries[index] = o;
        Ok(())
    }
}
fn start(
    state: &mut CatalogProjection,
    o: &RecoveryOperation,
    event_id: &str,
    cause: Option<&str>,
) -> Result<(), PoolError> {
    if !state
        .worktrees
        .iter()
        .any(|w| w.worktree_id == o.worktree_id && w.repository_id == o.repository_id)
    {
        return Err(PoolError::Conflict);
    }
    let a = state.assignments.iter().find(|a| {
        a.repository_id == o.repository_id
            && a.worktree_id == o.worktree_id
            && a.state != AssignmentState::Released
    });
    if a.map(|a| a.assignment_handle) != o.assignment_handle
        || (a.is_none()
            && !matches!(
                o.disposition,
                RecoveryDisposition::Withheld | RecoveryDisposition::Unassigned
            ))
        || (a.is_some() && o.disposition == RecoveryDisposition::Unassigned)
    {
        return Err(PoolError::Conflict);
    }
    let valid_tip = o.tip.as_ref().is_some_and(|tip| {
        matches!(tip.len(), 40 | 64)
            && tip
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    });
    let protected = o.disposition == RecoveryDisposition::Withheld;
    if o.state != RecoveryState::Intended
        || !o.intent_event_id.is_empty()
        || o.last_checkpoint != "recovery_intended"
        || (!valid_tip && !protected)
        || (o.tip.is_some() && !valid_tip)
        || o.preservation_reference != expected_preservation_reference(state, o)
        || protected != o.protected_reason.is_some()
        || o.protected_reason.as_deref().is_some_and(|reason| {
            !matches!(
                reason,
                "safety_uncertain"
                    | "partial_creation_unproven"
                    | "unfinished_work"
                    | "git_operation_in_progress"
                    | "unsupported_index_state"
                    | "retained_file_collision"
            )
        })
        || state.has_pending_recovery(o.repository_id)
        || state.operation_identity_used(o.operation_id)
    {
        return Err(PoolError::Conflict);
    }
    if o.disposition == RecoveryDisposition::Active
        && a.is_none_or(|a| {
            a.state == AssignmentState::Released
                || (a.state == AssignmentState::Preparing
                    && (o.tip.as_deref() != Some(a.resolved_commit.as_str()) || o.branch.is_some()))
        })
    {
        return Err(PoolError::Conflict);
    }
    if let Some(id) = o.target_operation_id {
        if let Some(target) = state.releases.iter().find(|r| r.operation_id == id) {
            if target.repository_id != o.repository_id
                || target.worktree_id != o.worktree_id
                || Some(target.assignment_handle) != o.assignment_handle
                || !target.state.is_pending()
                || cause != Some(target.intent_event_id.as_str())
                || o.disposition == RecoveryDisposition::Active
                || (o.disposition == RecoveryDisposition::Released
                    && (o.tip.as_deref() != Some(target.tip.as_str()) || o.branch != target.branch))
            {
                return Err(PoolError::Conflict);
            }
        } else {
            let (target, checkpoint) = preparation_target(state, id)?;
            if target.repository_id != o.repository_id
                || target.worktree_id != o.worktree_id
                || Some(target.assignment_handle) != o.assignment_handle
                || !target.state.is_pending()
                || cause != Some(checkpoint)
            {
                return Err(PoolError::Conflict);
            }
        }
    } else if cause.is_some() {
        return Err(PoolError::Conflict);
    }
    let mut o = o.clone();
    o.intent_event_id = event_id.into();
    state.recoveries.push(o);
    Ok(())
}
fn finish(state: &mut CatalogProjection, o: &RecoveryOperation) -> Result<(), PoolError> {
    let a = state.assignments.iter_mut().find(|a| {
        Some(a.assignment_handle) == o.assignment_handle && a.state != AssignmentState::Released
    });
    match o.disposition {
        RecoveryDisposition::Released => {
            a.ok_or(PoolError::Conflict)?.state = AssignmentState::Released;
            state
                .worktrees
                .iter_mut()
                .find(|w| w.worktree_id == o.worktree_id)
                .ok_or(PoolError::Corrupt)?
                .last_release_position =
                Some(state.revision.checked_add(1).ok_or(PoolError::Corrupt)?);
        }
        RecoveryDisposition::Active => {
            a.ok_or(PoolError::Conflict)?.state = AssignmentState::Active;
        }
        RecoveryDisposition::Unassigned => {
            if a.is_some() {
                return Err(PoolError::Conflict);
            }
        }
        RecoveryDisposition::Withheld => {
            if let Some(w) = state
                .withheld_worktrees
                .iter_mut()
                .find(|w| w.worktree_id == o.worktree_id)
            {
                w.reason = o.protected_reason.clone().ok_or(PoolError::Corrupt)?;
            } else {
                state.withheld_worktrees.push(WithheldWorktree {
                    repository_id: o.repository_id,
                    worktree_id: o.worktree_id,
                    reason: o.protected_reason.clone().ok_or(PoolError::Corrupt)?,
                });
            }
        }
    }
    if o.disposition != RecoveryDisposition::Withheld {
        state
            .withheld_worktrees
            .retain(|w| w.worktree_id != o.worktree_id);
    }
    if let Some(id) = o.target_operation_id {
        if let Some(target) = state.releases.iter_mut().find(|r| r.operation_id == id) {
            target.state = if o.disposition == RecoveryDisposition::Released {
                crate::release::ReleaseState::Completed
            } else {
                crate::release::ReleaseState::Reconciled
            };
            target.last_checkpoint = "release_reconciled".into();
        } else {
            let acquisition = preparation_target(state, id)?.0.operation_id;
            let target = state
                .acquisitions
                .iter_mut()
                .find(|a| a.operation_id == acquisition)
                .ok_or(PoolError::Conflict)?;
            target.state = if o.disposition == RecoveryDisposition::Active {
                AcquisitionState::Completed
            } else {
                AcquisitionState::Reconciled
            };
            target.last_checkpoint = "acquisition_reconciled".into();
            if let Some(creation) = state
                .creations
                .iter_mut()
                .find(|c| c.acquisition_operation_id == acquisition)
            {
                creation.state = if o.disposition == RecoveryDisposition::Active {
                    CreationState::Completed
                } else {
                    CreationState::Reconciled
                };
                creation.last_checkpoint = "creation_reconciled".into();
            }
        }
    }
    Ok(())
}
pub(crate) fn preparation_target(
    state: &CatalogProjection,
    id: OperationId,
) -> Result<(&AcquisitionOperation, &str), PoolError> {
    if let Some(acquisition) = state.acquisitions.iter().find(|a| a.operation_id == id) {
        return Ok((acquisition, acquisition.intent_event_id.as_str()));
    }
    let creation = state
        .creations
        .iter()
        .find(|c| c.operation_id == id)
        .ok_or(PoolError::Unregistered)?;
    let acquisition = state
        .acquisitions
        .iter()
        .find(|a| {
            a.operation_id == creation.acquisition_operation_id
                && (a.assignment_handle, a.worktree_id, a.repository_id)
                    == (
                        creation.assignment_handle,
                        creation.worktree_id,
                        creation.repository_id,
                    )
        })
        .ok_or(PoolError::Corrupt)?;
    Ok((acquisition, creation.intent_event_id.as_str()))
}

fn expected_preservation_reference(
    state: &CatalogProjection,
    o: &RecoveryOperation,
) -> Option<String> {
    if !o.requires_preservation() {
        return None;
    }
    if let Some(target) = state
        .releases
        .iter()
        .find(|r| Some(r.operation_id) == o.target_operation_id)
    {
        return target.preservation_reference.clone();
    }
    Some(format!("refs/worktree-pool/{}", o.operation_id))
}
