//! Current-handle release with concrete safety observations and durable checkpoints.
use std::{ffi::OsStr, path::Path};

use crate::{
    acquisition::{Assignment, AssignmentState},
    catalog,
    coordination::LockGuard,
    domain::{CatalogProjection, decode_event, management_event, management_event_caused_by},
    error::PoolError,
    git,
    management::{AssignmentHandle, ManagementEvent, OperationId, RepositoryId},
    paths::Paths,
    release::{ReleaseEvent, ReleaseOperation, ReleaseState},
    workflows,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseCheckpoint {
    IntentCommitted,
    PreservationObserved,
    PreservationCommitted,
    ResultCommitted,
}
/// # Errors
/// Rejects unsafe or unknown ownership; never repeats an uncertain preservation effect.
pub fn release(
    paths: &Paths,
    selector: Option<&OsStr>,
    handle: AssignmentHandle,
) -> Result<
    (
        CatalogProjection,
        Assignment,
        Option<ReleaseOperation>,
        bool,
    ),
    PoolError,
> {
    release_observed(paths, selector, handle, |_| {})
}
/// # Errors
/// Keeps ownership across effect failure, interruption and indeterminate commits.
pub fn release_observed(
    paths: &Paths,
    selector: Option<&OsStr>,
    handle: AssignmentHandle,
    mut observe: impl FnMut(ReleaseCheckpoint),
) -> Result<
    (
        CatalogProjection,
        Assignment,
        Option<ReleaseOperation>,
        bool,
    ),
    PoolError,
> {
    let _maintenance = workflows::maintenance(paths)?;
    let state = catalog::open(paths)?.projection;
    let assignment = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle == handle)
        .ok_or(PoolError::UnknownAssignment)?;
    let repository_id = assignment.repository_id;
    if selector.is_some() && workflows::repository(&state, selector)?.repository_id != repository_id
    {
        return Err(PoolError::Selectors);
    }
    let _repository = LockGuard::acquire(
        &paths.state.join(format!("repository-{repository_id}.lock")),
        true,
    )?;
    let state = catalog::open(paths)?.projection;
    let assignment = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle == handle)
        .cloned()
        .ok_or(PoolError::UnknownAssignment)?;
    if assignment.state == AssignmentState::Released {
        return result(state, handle, true);
    }
    if state
        .releases
        .iter()
        .any(|o| o.assignment_handle == handle && o.state.is_pending())
    {
        return result(state, handle, false);
    }
    if assignment.state != AssignmentState::Active || has_pending_work(&state, repository_id) {
        return Err(PoolError::OperationPending);
    }
    let repo = state
        .repositories
        .iter()
        .find(|r| r.repository_id == repository_id)
        .ok_or(PoolError::Corrupt)?;
    let common = repo.common_directory.to_path()?;
    let w = state
        .worktrees
        .iter()
        .find(|w| w.worktree_id == assignment.worktree_id)
        .ok_or(PoolError::Corrupt)?;
    let path = assignment.path.to_path()?;
    let git_directory = w.git_directory.to_path()?;
    validate_binding(&path, &common, &git_directory)?;
    let tip = git::resolve_commit(&path, OsStr::new("HEAD"))?;
    let safety = git::observe_safe(&path, &tip)?;
    let (mut state, op) = begin_release(paths, &assignment, &safety)?;
    let operation_id = op.operation_id;
    observe(ReleaseCheckpoint::IntentCommitted);
    if op.branch.is_none() {
        let successful = git::preserve_tip(&common, &operation_id.to_string(), &op.tip).is_ok();
        observe(ReleaseCheckpoint::PreservationObserved);
        let cause = operation(&state, operation_id)?.intent_event_id.clone();
        state = append(
            paths,
            ReleaseEvent::PreservationFinished {
                operation_id,
                repository_id,
                worktree_id: op.worktree_id,
                successful,
            },
            Some(&cause),
        )?;
        observe(ReleaseCheckpoint::PreservationCommitted);
        if !successful {
            return result(state, handle, false);
        }
    }
    let successful = validate_binding(&path, &common, &git_directory).is_ok()
        && git::observe_safe(&path, &op.tip)
            .is_ok_and(|git::Safety { head, branch }| head == op.tip && branch == op.branch)
        && (op.branch.is_some()
            || git::verify_preserved_tip(&common, &operation_id.to_string(), &op.tip).is_ok());
    let cause = operation(&state, operation_id)?.intent_event_id.clone();
    state = append(
        paths,
        ReleaseEvent::Finished {
            operation_id,
            repository_id,
            worktree_id: op.worktree_id,
            successful,
        },
        Some(&cause),
    )?;
    observe(ReleaseCheckpoint::ResultCommitted);
    result(state, handle, false)
}
fn operation(state: &CatalogProjection, id: OperationId) -> Result<&ReleaseOperation, PoolError> {
    state
        .releases
        .iter()
        .find(|o| o.operation_id == id)
        .ok_or(PoolError::Corrupt)
}
fn result(
    state: CatalogProjection,
    handle: AssignmentHandle,
    already: bool,
) -> Result<
    (
        CatalogProjection,
        Assignment,
        Option<ReleaseOperation>,
        bool,
    ),
    PoolError,
> {
    let a = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle == handle)
        .cloned()
        .ok_or(PoolError::Corrupt)?;
    let o = state
        .releases
        .iter()
        .rev()
        .find(|o| o.assignment_handle == handle)
        .cloned();
    if o.is_none() && !already {
        return Err(PoolError::Corrupt);
    }
    Ok((state, a, o, already))
}
fn append(
    paths: &Paths,
    event: ReleaseEvent,
    cause: Option<&str>,
) -> Result<CatalogProjection, PoolError> {
    let session = catalog::open(paths)?;
    let event = ManagementEvent::Release(event);
    let wire = if let Some(cause) = cause {
        management_event_caused_by(session.projection.catalog_id, &event, cause)?
    } else {
        management_event(session.projection.catalog_id, &event)?
    };
    session.store().append(
        decode_event(&wire, session.projection.catalog_id)?
            .expected_revision(&session.projection)?,
        wire,
    )
}

fn validate_binding(path: &Path, common: &Path, git_directory: &Path) -> Result<(), PoolError> {
    let checkout = git::checkout(path)?;
    if checkout.path != path
        || checkout.common_directory != common
        || checkout.git_directory != git_directory
    {
        return Err(PoolError::Git);
    }
    Ok(())
}

fn begin_release(
    paths: &Paths,
    assignment: &Assignment,
    safety: &git::Safety,
) -> Result<(CatalogProjection, ReleaseOperation), PoolError> {
    let repository_id = assignment.repository_id;
    let handle = assignment.assignment_handle;
    let operation_id = OperationId::new();
    let op = ReleaseOperation {
        operation_id,
        repository_id,
        worktree_id: assignment.worktree_id,
        assignment_handle: handle,
        state: ReleaseState::Intended,
        tip: safety.head.clone(),
        branch: safety.branch.clone(),
        preservation_reference: safety
            .branch
            .is_none()
            .then(|| format!("refs/worktree-pool/{operation_id}")),
        intent_event_id: String::new(),
        last_checkpoint: "release_intended".into(),
    };
    let state = append(paths, ReleaseEvent::Started(op.clone()), None)?;
    Ok((state, op))
}

fn has_pending_work(state: &CatalogProjection, repository_id: RepositoryId) -> bool {
    state.has_pending_recovery(repository_id)
        || state
            .operations
            .iter()
            .any(|o| o.repository_id == repository_id && o.state.is_pending())
        || state
            .acquisitions
            .iter()
            .any(|o| o.repository_id == repository_id && o.state.is_pending())
        || state
            .releases
            .iter()
            .any(|o| o.repository_id == repository_id && o.state.is_pending())
}
