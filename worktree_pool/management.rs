//! Explicitly enrolled resources and their pure catalog transitions.
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{domain::CatalogProjection, error::PoolError, paths::EncodedPath};

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(Uuid);
        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.to_string()
            }
        }
        impl TryFrom<String> for $name {
            type Error = PoolError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                value.parse()
            }
        }
        impl FromStr for $name {
            type Err = PoolError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let id = Uuid::parse_str(value).map_err(|_| PoolError::Corrupt)?;
                if id.is_nil() || id.to_string() != value {
                    return Err(PoolError::Corrupt);
                }
                Ok(Self(id))
            }
        }
    };
}
identity!(RepositoryId);
identity!(WorktreeId);
identity!(OperationId);
identity!(AssignmentHandle);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repository {
    pub repository_id: RepositoryId,
    pub common_directory: EncodedPath,
    pub context_path: EncodedPath,
    pub capacity: u32,
    #[serde(default)]
    pub revision: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Worktree {
    pub worktree_id: WorktreeId,
    pub repository_id: RepositoryId,
    pub path: EncodedPath,
    pub git_directory: EncodedPath,
    #[serde(default)]
    pub last_release_position: Option<u64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshState {
    Pending,
    Completed,
    NeedsReconciliation,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshOperation {
    pub operation_id: OperationId,
    pub repository_id: RepositoryId,
    pub state: RefreshState,
    pub last_checkpoint: String,
    pub intent_event_id: String,
    pub resolved_commit: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshStarted {
    pub operation_id: OperationId,
    pub repository_id: RepositoryId,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshFinished {
    pub operation_id: OperationId,
    pub repository_id: RepositoryId,
    pub resolved_commit: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ManagementEvent {
    Creation(crate::creation::CreationEvent),
    CapacityConfigured {
        repository_id: RepositoryId,
        maximum: u32,
    },
    Release(crate::release::ReleaseEvent),
    WorktreeWithheld(crate::acquisition::WithheldWorktree),
    Acquisition(crate::acquisition::AcquisitionEvent),
    RepositoryRegistered(Repository),
    WorktreeRegistered(Worktree),
    WorktreeEnrolled(Worktree),
    RefreshStarted(RefreshStarted),
    RefreshFinished(RefreshFinished),
}
impl ManagementEvent {
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Creation(c) => c.event_type(),
            Self::CapacityConfigured { .. } => {
                "io.lowkeylab.worktreepool.repository.capacity.configured.v1"
            }
            Self::WorktreeWithheld(_) => "io.lowkeylab.worktreepool.worktree.withheld.v1",
            Self::Release(r) => r.event_type(),
            Self::Acquisition(a) => a.event_type(),
            Self::RepositoryRegistered(_) => "io.lowkeylab.worktreepool.repository.registered.v1",
            Self::WorktreeRegistered(_) => "io.lowkeylab.worktreepool.worktree.registered.v1",
            Self::WorktreeEnrolled(_) => "io.lowkeylab.worktreepool.worktree.registered.v2",
            Self::RefreshStarted(_) => "io.lowkeylab.worktreepool.repository.refresh.started.v1",
            Self::RefreshFinished(_) => "io.lowkeylab.worktreepool.repository.refresh.finished.v1",
        }
    }
    #[must_use]
    pub const fn repository_id(&self) -> Option<RepositoryId> {
        match self {
            Self::Creation(c) => c.repository_id(),
            Self::CapacityConfigured { repository_id, .. } => Some(*repository_id),
            Self::WorktreeWithheld(w) => Some(w.repository_id),
            Self::Release(r) => Some(r.repository_id()),
            Self::Acquisition(a) => Some(a.repository_id()),
            Self::RepositoryRegistered(_) | Self::WorktreeEnrolled(_) => None,
            Self::WorktreeRegistered(w) => Some(w.repository_id),
            Self::RefreshStarted(r) => Some(r.repository_id),
            Self::RefreshFinished(r) => Some(r.repository_id),
        }
    }
    #[must_use]
    pub const fn operation_id(&self) -> Option<OperationId> {
        match self {
            Self::Creation(c) => Some(c.operation_id()),
            Self::Release(r) => Some(r.operation_id()),
            Self::Acquisition(a) => Some(a.operation_id()),
            Self::RefreshStarted(r) => Some(r.operation_id),
            Self::RefreshFinished(r) => Some(r.operation_id),
            _ => None,
        }
    }
    #[must_use]
    pub fn subject(&self) -> String {
        match self {
            Self::Creation(c) => format!("worktrees/{}", c.worktree_id()),
            Self::CapacityConfigured { repository_id, .. } => {
                format!("repositories/{repository_id}")
            }
            Self::WorktreeWithheld(w) => format!("worktrees/{}", w.worktree_id),
            Self::Release(r) => format!("worktrees/{}", r.worktree_id()),
            Self::Acquisition(a) => format!("worktrees/{}", a.worktree_id()),
            Self::RepositoryRegistered(r) => format!("repositories/{}", r.repository_id),
            Self::WorktreeRegistered(w) | Self::WorktreeEnrolled(w) => {
                format!("worktrees/{}", w.worktree_id)
            }
            Self::RefreshStarted(r) => format!("repositories/{}", r.repository_id),
            Self::RefreshFinished(r) => format!("repositories/{}", r.repository_id),
        }
    }
    /// Apply an enrollment fact without filesystem or Git effects.
    /// # Errors
    /// Rejects duplicate identities, invalid paths, unknown repositories and capacity overflow.
    pub fn apply(
        &self,
        state: &mut CatalogProjection,
        event_id: &str,
        causation_id: Option<&str>,
    ) -> Result<(), PoolError> {
        match self {
            Self::Creation(c) => c.apply(state, event_id, causation_id)?,
            Self::CapacityConfigured {
                repository_id,
                maximum,
            } => configure_capacity(state, *repository_id, *maximum)?,
            Self::WorktreeWithheld(w) => withhold(state, w)?,
            Self::Release(r) => r.apply(state, event_id, causation_id)?,
            Self::Acquisition(a) => a.apply(state, event_id, causation_id)?,
            Self::RepositoryRegistered(repository) => {
                repository.common_directory.to_path()?;
                repository.context_path.to_path()?;
                if repository.capacity != 4
                    || repository.revision != 0
                    || state.repositories.iter().any(|r| {
                        r.repository_id == repository.repository_id
                            || r.common_directory.bytes == repository.common_directory.bytes
                    })
                {
                    return Err(PoolError::Conflict);
                }
                state.repositories.push(repository.clone());
            }
            Self::WorktreeRegistered(worktree) | Self::WorktreeEnrolled(worktree) => {
                enroll_worktree(state, worktree)?;
            }
            Self::RefreshStarted(started) => {
                if state.releases.iter().any(|o| {
                    o.repository_id == started.repository_id
                        && o.state != crate::release::ReleaseState::Completed
                }) || state.acquisitions.iter().any(|o| {
                    o.repository_id == started.repository_id
                        && o.state != crate::acquisition::AcquisitionState::Completed
                }) || state.operations.iter().any(|o| {
                    o.operation_id == started.operation_id
                        || (o.repository_id == started.repository_id
                            && o.state != RefreshState::Completed)
                }) {
                    return Err(PoolError::OperationPending);
                }
                state.operations.push(RefreshOperation {
                    operation_id: started.operation_id,
                    repository_id: started.repository_id,
                    state: RefreshState::Pending,
                    last_checkpoint: "intent_recorded".into(),
                    intent_event_id: event_id.into(),
                    resolved_commit: None,
                });
            }
            Self::RefreshFinished(finished) => {
                if finished.resolved_commit.as_ref().is_some_and(|oid| {
                    !matches!(oid.len(), 40 | 64)
                        || !oid
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                }) {
                    return Err(PoolError::Corrupt);
                }
                let operation = state
                    .operations
                    .iter_mut()
                    .find(|o| {
                        o.operation_id == finished.operation_id
                            && o.repository_id == finished.repository_id
                    })
                    .ok_or(PoolError::Corrupt)?;
                if operation.state != RefreshState::Pending
                    || causation_id != Some(operation.intent_event_id.as_str())
                {
                    return Err(PoolError::Conflict);
                }
                operation.state = if finished.resolved_commit.is_some() {
                    RefreshState::Completed
                } else {
                    RefreshState::NeedsReconciliation
                };
                operation.last_checkpoint = if finished.resolved_commit.is_some() {
                    "result_committed"
                } else {
                    "fetch_failed"
                }
                .into();
                operation
                    .resolved_commit
                    .clone_from(&finished.resolved_commit);
            }
        }
        if let Some(id) = self.repository_id() {
            let repo = state
                .repositories
                .iter_mut()
                .find(|r| r.repository_id == id)
                .ok_or(PoolError::Unregistered)?;
            repo.revision = repo.revision.checked_add(1).ok_or(PoolError::Corrupt)?;
        }
        Ok(())
    }
}

pub(crate) fn enroll_worktree(
    state: &mut CatalogProjection,
    worktree: &Worktree,
) -> Result<(), PoolError> {
    if worktree.last_release_position.is_some() {
        return Err(PoolError::Corrupt);
    }
    worktree.path.to_path()?;
    worktree.git_directory.to_path()?;
    let repo = state
        .repositories
        .iter()
        .find(|r| r.repository_id == worktree.repository_id)
        .ok_or(PoolError::Unregistered)?;
    if state.worktrees.iter().any(|w| {
        w.worktree_id == worktree.worktree_id
            || w.path.bytes == worktree.path.bytes
            || w.git_directory.bytes == worktree.git_directory.bytes
    }) {
        return Err(PoolError::Conflict);
    }
    if state
        .worktrees
        .iter()
        .filter(|w| w.repository_id == repo.repository_id)
        .count()
        >= repo.capacity as usize
    {
        return Err(PoolError::Capacity);
    }
    state.worktrees.push(worktree.clone());
    Ok(())
}

fn withhold(
    state: &mut CatalogProjection,
    w: &crate::acquisition::WithheldWorktree,
) -> Result<(), PoolError> {
    if !matches!(
        w.reason.as_str(),
        "safety_uncertain"
            | "retained_file_collision"
            | "unfinished_work"
            | "git_operation_in_progress"
            | "unsupported_index_state"
    ) || !state
        .worktrees
        .iter()
        .any(|x| x.repository_id == w.repository_id && x.worktree_id == w.worktree_id)
        || state
            .withheld_worktrees
            .iter()
            .any(|x| x.worktree_id == w.worktree_id)
    {
        return Err(PoolError::Conflict);
    }
    state.withheld_worktrees.push(w.clone());

    Ok(())
}

fn configure_capacity(
    state: &mut CatalogProjection,
    repository_id: RepositoryId,
    maximum: u32,
) -> Result<(), PoolError> {
    let count = state
        .worktrees
        .iter()
        .filter(|w| w.repository_id == repository_id)
        .count();
    if (maximum as usize) < count {
        return Err(PoolError::CapacityBelowCount);
    }
    state
        .repositories
        .iter_mut()
        .find(|r| r.repository_id == repository_id)
        .ok_or(PoolError::Unregistered)?
        .capacity = maximum;
    Ok(())
}
