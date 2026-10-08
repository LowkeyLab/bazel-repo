//! Pure assignment reservation and acquisition checkpoints. No Git or filesystem effects.
use serde::{Deserialize, Serialize};

use crate::{
    domain::CatalogProjection,
    error::PoolError,
    management::{AssignmentHandle, OperationId, RepositoryId, WorktreeId},
    paths::EncodedPath,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub assignment_handle: AssignmentHandle,
    pub operation_id: OperationId,
    pub repository_id: RepositoryId,
    pub worktree_id: WorktreeId,
    pub path: EncodedPath,
    pub resolved_commit: String,
    pub branch: Option<String>,
    pub state: AssignmentState,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentState {
    Preparing,
    Active,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionState {
    Reserved,
    PreservationIntended,
    Preserved,
    CheckoutIntended,
    Completed,
    NeedsReconciliation,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcquisitionOperation {
    pub operation_id: OperationId,
    pub repository_id: RepositoryId,
    pub worktree_id: WorktreeId,
    pub assignment_handle: AssignmentHandle,
    pub state: AcquisitionState,
    pub intent_event_id: String,
    pub last_checkpoint: String,
    pub preservation_tip: Option<String>,
    pub preservation_reference: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AcquisitionEvent {
    Reserved(Assignment),
    PreservationIntended {
        operation_id: OperationId,
        repository_id: RepositoryId,
        worktree_id: WorktreeId,
        tip: String,
    },
    Preserved {
        operation_id: OperationId,
        repository_id: RepositoryId,
        worktree_id: WorktreeId,
    },
    CheckoutIntended {
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
impl AcquisitionEvent {
    #[must_use]
    pub const fn repository_id(&self) -> RepositoryId {
        match self {
            Self::Reserved(a) => a.repository_id,
            Self::PreservationIntended { repository_id, .. }
            | Self::Preserved { repository_id, .. }
            | Self::CheckoutIntended { repository_id, .. }
            | Self::Finished { repository_id, .. } => *repository_id,
        }
    }
    #[must_use]
    pub const fn operation_id(&self) -> OperationId {
        match self {
            Self::Reserved(a) => a.operation_id,
            Self::PreservationIntended { operation_id, .. }
            | Self::Preserved { operation_id, .. }
            | Self::CheckoutIntended { operation_id, .. }
            | Self::Finished { operation_id, .. } => *operation_id,
        }
    }
    #[must_use]
    pub const fn worktree_id(&self) -> WorktreeId {
        match self {
            Self::Reserved(a) => a.worktree_id,
            Self::PreservationIntended { worktree_id, .. }
            | Self::Preserved { worktree_id, .. }
            | Self::CheckoutIntended { worktree_id, .. }
            | Self::Finished { worktree_id, .. } => *worktree_id,
        }
    }
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Reserved(_) => "io.lowkeylab.worktreepool.assignment.reserved.v1",
            Self::PreservationIntended { .. } => {
                "io.lowkeylab.worktreepool.worktree.preservation.intended.v1"
            }
            Self::Preserved { .. } => "io.lowkeylab.worktreepool.worktree.preserved.v1",
            Self::CheckoutIntended { .. } => {
                "io.lowkeylab.worktreepool.worktree.checkout.intended.v1"
            }
            Self::Finished { .. } => "io.lowkeylab.worktreepool.assignment.acquisition.finished.v1",
        }
    }
    /// # Errors
    /// Rejects inconsistent identities, duplicate reservations and invalid checkpoint order.
    pub fn apply(
        &self,
        state: &mut CatalogProjection,
        event_id: &str,
        causation: Option<&str>,
    ) -> Result<(), PoolError> {
        match self {
            Self::Reserved(a) => reserve(state, a, event_id)?,
            Self::PreservationIntended { .. }
            | Self::Preserved { .. }
            | Self::CheckoutIntended { .. }
            | Self::Finished { .. } => {
                let op = state
                    .acquisitions
                    .iter_mut()
                    .find(|o| {
                        o.operation_id == self.operation_id()
                            && o.repository_id == self.repository_id()
                            && o.worktree_id == self.worktree_id()
                    })
                    .ok_or(PoolError::Corrupt)?;
                if causation != Some(op.intent_event_id.as_str()) {
                    return Err(PoolError::Conflict);
                }
                match self {
                    Self::PreservationIntended { tip, .. }
                        if op.state == AcquisitionState::Reserved && valid_commit(tip) =>
                    {
                        op.state = AcquisitionState::PreservationIntended;
                        op.preservation_tip = Some(tip.clone());
                        op.preservation_reference =
                            Some(format!("refs/worktree-pool/{}", op.operation_id));
                        op.intent_event_id = event_id.into();
                        op.last_checkpoint = "preservation_intended".into();
                    }
                    Self::Preserved { .. }
                        if op.state == AcquisitionState::PreservationIntended =>
                    {
                        op.state = AcquisitionState::Preserved;
                        op.intent_event_id = event_id.into();
                        op.last_checkpoint = "preservation_committed".into();
                    }
                    Self::CheckoutIntended { .. }
                        if matches!(
                            op.state,
                            AcquisitionState::Reserved | AcquisitionState::Preserved
                        ) =>
                    {
                        op.state = AcquisitionState::CheckoutIntended;
                        op.intent_event_id = event_id.into();
                        op.last_checkpoint = "checkout_intended".into();
                    }
                    Self::Finished { successful, .. }
                        if op.state == AcquisitionState::CheckoutIntended =>
                    {
                        op.state = if *successful {
                            AcquisitionState::Completed
                        } else {
                            AcquisitionState::NeedsReconciliation
                        };
                        op.last_checkpoint = if *successful {
                            "acquisition_committed"
                        } else {
                            "checkout_uncertain"
                        }
                        .into();
                        if *successful {
                            state
                                .assignments
                                .iter_mut()
                                .find(|a| a.assignment_handle == op.assignment_handle)
                                .ok_or(PoolError::Corrupt)?
                                .state = AssignmentState::Active;
                        }
                    }
                    _ => return Err(PoolError::Conflict),
                }
            }
        }
        Ok(())
    }
}
fn valid_commit(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WithheldWorktree {
    pub repository_id: RepositoryId,
    pub worktree_id: WorktreeId,
    pub reason: String,
}

/// Deterministic reuse priority: matching commit, latest committed release, then stable ID.
/// Release positions are absent until an actual release fact supplies them.
#[must_use]
pub fn candidate_order(
    worktree: &crate::management::Worktree,
    head: &str,
    target: &str,
) -> (bool, std::cmp::Reverse<Option<u64>>, String) {
    (
        head != target,
        std::cmp::Reverse(worktree.last_release_position),
        worktree.worktree_id.to_string(),
    )
}

fn reserve(state: &mut CatalogProjection, a: &Assignment, event_id: &str) -> Result<(), PoolError> {
    a.path.to_path()?;
    if a.state != AssignmentState::Preparing
        || a.branch.is_some()
        || !valid_commit(&a.resolved_commit)
        || !state.worktrees.iter().any(|w| {
            w.worktree_id == a.worktree_id && w.repository_id == a.repository_id && w.path == a.path
        })
        || state
            .withheld_worktrees
            .iter()
            .any(|w| w.worktree_id == a.worktree_id)
        || state
            .assignments
            .iter()
            .any(|x| x.assignment_handle == a.assignment_handle || x.worktree_id == a.worktree_id)
        || state.acquisitions.iter().any(|o| {
            o.operation_id == a.operation_id
                || (o.repository_id == a.repository_id && o.state != AcquisitionState::Completed)
        })
    {
        return Err(PoolError::Conflict);
    }
    state.assignments.push(a.clone());
    state.acquisitions.push(AcquisitionOperation {
        operation_id: a.operation_id,
        repository_id: a.repository_id,
        worktree_id: a.worktree_id,
        assignment_handle: a.assignment_handle,
        state: AcquisitionState::Reserved,
        intent_event_id: event_id.into(),
        last_checkpoint: "reservation_committed".into(),
        preservation_tip: None,
        preservation_reference: None,
    });

    Ok(())
}
