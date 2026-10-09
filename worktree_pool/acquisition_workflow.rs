//! Lock-scoped acquisition orchestration using concrete Git and catalog adapters.
use std::{ffi::OsStr, path::Path};

use crate::{
    acquisition::{AcquisitionEvent, Assignment, AssignmentState},
    catalog,
    coordination::LockGuard,
    domain::{CatalogProjection, decode_event, management_event, management_event_caused_by},
    error::PoolError,
    git,
    management::{AssignmentHandle, ManagementEvent, OperationId, Repository, Worktree},
    paths::Paths,
    workflows,
};

/// Observes committed checkpoints and external-effect boundaries without replacing effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcquisitionCheckpoint {
    RefreshCommitted,
    CreationIntended,
    CreationPathObserved,
    CreationPathCommitted,
    CreationObserved,
    ReservationCommitted,
    PreservationIntended,
    PreservationObserved,
    PreservationCommitted,
    CheckoutIntended,
    CheckoutObserved,
    ResultCommitted,
}
/// # Errors
/// Rejects unsafe candidates and retains exclusive preparation across uncertain effects.
pub fn acquire(
    paths: &Paths,
    selector: Option<&OsStr>,
    reference: Option<&OsStr>,
) -> Result<(CatalogProjection, Assignment), PoolError> {
    acquire_observed(paths, selector, reference, |_| {})
}
/// # Errors
/// Preserves durable pending ownership across effect and reporting interruption.
pub fn acquire_observed(
    paths: &Paths,
    selector: Option<&OsStr>,
    reference: Option<&OsStr>,
    mut observe: impl FnMut(AcquisitionCheckpoint),
) -> Result<(CatalogProjection, Assignment), PoolError> {
    let _maintenance = workflows::maintenance(paths)?;
    let state = catalog::open(paths)?.projection;
    let repo = workflows::repository(&state, selector)?;
    let _repository = LockGuard::acquire(
        &paths
            .state
            .join(format!("repository-{}.lock", repo.repository_id)),
        true,
    )?;
    let state = catalog::open(paths)?.projection;
    if state
        .acquisitions
        .iter()
        .any(|o| o.repository_id == repo.repository_id && o.state.is_pending())
    {
        return Err(PoolError::OperationPending);
    }
    if state
        .releases
        .iter()
        .any(|o| o.repository_id == repo.repository_id && o.state.is_pending())
    {
        return Err(PoolError::OperationPending);
    }
    if state.has_pending_recovery(repo.repository_id) {
        return Err(PoolError::OperationPending);
    }
    if state
        .operations
        .iter()
        .any(|o| o.repository_id == repo.repository_id && o.state.is_pending())
    {
        return Err(PoolError::OperationPending);
    }
    let common = repo.common_directory.to_path()?;
    let (state, target) = if let Some(reference) = reference {
        let target = git::resolve_repository_commit(&common, reference)?;
        (state, target)
    } else {
        let (state, refresh) = workflows::refresh_locked(paths, &repo, |_| {})?;
        if refresh.state != crate::management::RefreshState::Completed {
            return Err(PoolError::OperationPending);
        }
        observe(AcquisitionCheckpoint::RefreshCommitted);
        (state, refresh.resolved_commit.ok_or(PoolError::Corrupt)?)
    };
    let (worktree, safety) = match select_worktree(paths, &repo, &state, &common, &target) {
        Ok(candidate) => candidate,
        Err(PoolError::Unavailable) => {
            return crate::creation_workflow::create(paths, &repo, &target, &mut observe);
        }
        Err(error) => return Err(error),
    };
    let assignment = Assignment {
        assignment_handle: AssignmentHandle::new(),
        operation_id: OperationId::new(),
        repository_id: repo.repository_id,
        worktree_id: worktree.worktree_id,
        path: worktree.path,
        resolved_commit: target,
        branch: None,
        state: AssignmentState::Preparing,
    };
    prepare(paths, &common, assignment, &safety, &mut observe)
}

fn append(
    paths: &Paths,
    event: AcquisitionEvent,
    cause: Option<&str>,
) -> Result<CatalogProjection, PoolError> {
    let session = catalog::open(paths)?;
    let event = ManagementEvent::Acquisition(event);
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

fn select_worktree(
    paths: &Paths,
    repo: &Repository,
    state: &CatalogProjection,
    common: &Path,
    target: &str,
) -> Result<(Worktree, git::Safety), PoolError> {
    let mut candidates: Vec<_> = state
        .worktrees
        .iter()
        .filter(|w| {
            w.repository_id == repo.repository_id
                && !state
                    .assignments
                    .iter()
                    .any(|a| a.worktree_id == w.worktree_id && a.state != AssignmentState::Released)
                && !state
                    .withheld_worktrees
                    .iter()
                    .any(|x| x.worktree_id == w.worktree_id)
        })
        .cloned()
        .collect();
    candidates.sort_by_key(|w| w.worktree_id.to_string());
    let mut safe = Vec::new();
    for w in candidates {
        let path = w.path.to_path()?;
        let safety = git::checkout(&path).and_then(|observed| {
            if observed.path != path
                || observed.git_directory != w.git_directory.to_path()?
                || observed.common_directory.as_path() != common
            {
                return Err(PoolError::Git);
            }
            git::observe_safe(&path, target)
        });
        if let Ok(safety) = safety {
            safe.push((w, safety));
        } else {
            let reason = match &safety {
                Err(PoolError::RetainedCollision) => "retained_file_collision",
                Err(PoolError::UnfinishedWork) => "unfinished_work",
                Err(PoolError::GitOperation) => "git_operation_in_progress",
                Err(PoolError::UnsupportedIndex) => "unsupported_index_state",
                _ => "safety_uncertain",
            };
            let session = catalog::open(paths)?;
            let event = ManagementEvent::WorktreeWithheld(crate::acquisition::WithheldWorktree {
                repository_id: repo.repository_id,
                worktree_id: w.worktree_id,
                reason: reason.into(),
            });
            let wire = management_event(session.projection.catalog_id, &event)?;
            session.store().append(
                decode_event(&wire, session.projection.catalog_id)?
                    .expected_revision(&session.projection)?,
                wire,
            )?;
        }
    }
    safe.sort_by_key(|(w, safety)| crate::acquisition::candidate_order(w, &safety.head, target));
    safe.into_iter().next().ok_or(PoolError::Unavailable)
}

fn prepare(
    paths: &Paths,
    common: &Path,
    assignment: Assignment,
    safety: &git::Safety,
    observe: &mut impl FnMut(AcquisitionCheckpoint),
) -> Result<(CatalogProjection, Assignment), PoolError> {
    let mut state = append(paths, AcquisitionEvent::Reserved(assignment.clone()), None)?;
    observe(AcquisitionCheckpoint::ReservationCommitted);
    if safety.branch.is_none()
        && !preserve(
            paths,
            common,
            &assignment,
            &safety.head,
            &mut state,
            observe,
        )?
    {
        return Ok((state, assignment));
    }
    let op = state
        .acquisitions
        .iter()
        .find(|o| o.operation_id == assignment.operation_id)
        .ok_or(PoolError::Corrupt)?;
    state = append(
        paths,
        AcquisitionEvent::CheckoutIntended {
            operation_id: assignment.operation_id,
            repository_id: assignment.repository_id,
            worktree_id: assignment.worktree_id,
        },
        Some(&op.intent_event_id),
    )?;
    let cause = state
        .acquisitions
        .iter()
        .find(|o| o.operation_id == assignment.operation_id)
        .ok_or(PoolError::Corrupt)?
        .intent_event_id
        .clone();
    observe(AcquisitionCheckpoint::CheckoutIntended);
    let path = assignment.path.to_path()?;
    let successful = git::detach(&path, &assignment.resolved_commit)
        .and_then(|()| git::observe_safe(&path, &assignment.resolved_commit))
        .is_ok_and(|s| s.head == assignment.resolved_commit && s.branch.is_none());
    observe(AcquisitionCheckpoint::CheckoutObserved);
    state = append(
        paths,
        AcquisitionEvent::Finished {
            operation_id: assignment.operation_id,
            repository_id: assignment.repository_id,
            worktree_id: assignment.worktree_id,
            successful,
        },
        Some(&cause),
    )?;
    observe(AcquisitionCheckpoint::ResultCommitted);
    let result = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle == assignment.assignment_handle)
        .cloned()
        .ok_or(PoolError::Corrupt)?;
    Ok((state, result))
}

fn preserve(
    paths: &Paths,
    common: &Path,
    assignment: &Assignment,
    tip: &str,
    state: &mut CatalogProjection,
    observe: &mut impl FnMut(AcquisitionCheckpoint),
) -> Result<bool, PoolError> {
    let cause = state
        .acquisitions
        .iter()
        .find(|o| o.operation_id == assignment.operation_id)
        .ok_or(PoolError::Corrupt)?
        .intent_event_id
        .clone();
    *state = append(
        paths,
        AcquisitionEvent::PreservationIntended {
            operation_id: assignment.operation_id,
            repository_id: assignment.repository_id,
            worktree_id: assignment.worktree_id,
            tip: tip.into(),
        },
        Some(&cause),
    )?;
    let cause = state
        .acquisitions
        .iter()
        .find(|o| o.operation_id == assignment.operation_id)
        .ok_or(PoolError::Corrupt)?
        .intent_event_id
        .clone();
    observe(AcquisitionCheckpoint::PreservationIntended);
    if git::preserve_tip(common, &assignment.operation_id.to_string(), tip).is_err() {
        return Ok(false);
    }
    observe(AcquisitionCheckpoint::PreservationObserved);
    *state = append(
        paths,
        AcquisitionEvent::Preserved {
            operation_id: assignment.operation_id,
            repository_id: assignment.repository_id,
            worktree_id: assignment.worktree_id,
        },
        Some(&cause),
    )?;
    observe(AcquisitionCheckpoint::PreservationCommitted);

    Ok(true)
}
