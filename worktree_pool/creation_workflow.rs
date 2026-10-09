//! On-demand creation with counted ownership before any filesystem/Git effects.
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::Path,
};

use crate::{
    acquisition::{AcquisitionEvent, Assignment, AssignmentState},
    acquisition_workflow::AcquisitionCheckpoint,
    catalog,
    coordination::private_directory,
    creation::CreationEvent,
    domain::{CatalogProjection, management_event, management_event_caused_by},
    error::PoolError,
    git,
    management::{
        AssignmentHandle, ManagementEvent, OperationId, Repository, Worktree, WorktreeId,
    },
    paths::{EncodedPath, Paths},
};

/// Caller owns maintenance and repository locks; every short session closes before effects.
/// # Errors
/// Retains counted exclusive preparation whenever creation cannot be certified.
pub(crate) fn create(
    paths: &Paths,
    repo: &Repository,
    target: &str,
    observe: &mut impl FnMut(AcquisitionCheckpoint),
) -> Result<(CatalogProjection, Assignment), PoolError> {
    let common = repo.common_directory.to_path()?;
    let ReservedCreation {
        worktree,
        assignment,
        creation_id,
        creation_cause,
    } = reserve_creation(paths, repo, target)?;
    let path = worktree.path.to_path()?;
    let worktree_id = worktree.worktree_id;
    observe(AcquisitionCheckpoint::ReservationCommitted);
    observe(AcquisitionCheckpoint::CreationIntended);
    let prepared = prepare_path(&path, &worktree.git_directory.to_path()?);
    observe(AcquisitionCheckpoint::CreationPathObserved);
    let Ok(identity) = prepared else {
        return finish(
            paths,
            &assignment,
            creation_id,
            &creation_cause,
            None,
            false,
            observe,
        );
    };
    {
        let session = catalog::open(paths)?;
        let event = management_event_caused_by(
            session.projection.catalog_id,
            &ManagementEvent::Creation(CreationEvent::PathPrepared {
                operation_id: creation_id,
                repository_id: repo.repository_id,
                worktree_id,
            }),
            &creation_cause,
        )?;
        session
            .store()
            .append_batch(session.projection.revision, vec![event])?;
    }
    observe(AcquisitionCheckpoint::CreationPathCommitted);
    let checkout_cause;
    {
        let session = catalog::open(paths)?;
        let cause = &session
            .projection
            .acquisitions
            .iter()
            .find(|o| o.operation_id == assignment.operation_id)
            .ok_or(PoolError::Corrupt)?
            .intent_event_id;
        let checkout = management_event_caused_by(
            session.projection.catalog_id,
            &ManagementEvent::Acquisition(AcquisitionEvent::CheckoutIntended {
                operation_id: assignment.operation_id,
                repository_id: repo.repository_id,
                worktree_id,
            }),
            cause,
        )?;
        checkout_cause = checkout["id"]
            .as_str()
            .ok_or(PoolError::Corrupt)?
            .to_owned();
        session
            .store()
            .append_batch(session.projection.revision, vec![checkout])?;
    }
    observe(AcquisitionCheckpoint::CheckoutIntended);
    let successful = validate_prepared_path(&path, &worktree.git_directory.to_path()?, identity)
        .and_then(|()| git::add_worktree(&common, &path, target))
        .and_then(|()| {
            let observed = git::checkout(&path)?;
            if observed.path != path
                || observed.common_directory != common
                || observed.git_directory != worktree.git_directory.to_path()?
            {
                return Err(PoolError::Git);
            }
            let safety = git::observe_safe(&path, target)?;
            if safety.head != target || safety.branch.is_some() {
                return Err(PoolError::Git);
            }
            Ok(())
        })
        .is_ok();
    observe(AcquisitionCheckpoint::CreationObserved);
    observe(AcquisitionCheckpoint::CheckoutObserved);
    finish(
        paths,
        &assignment,
        creation_id,
        &creation_cause,
        Some(&checkout_cause),
        successful,
        observe,
    )
}
struct ReservedCreation {
    worktree: Worktree,
    assignment: Assignment,
    creation_id: OperationId,
    creation_cause: String,
}
fn reserve_creation(
    paths: &Paths,
    repo: &Repository,
    target: &str,
) -> Result<ReservedCreation, PoolError> {
    {
        let state = catalog::open(paths)?.projection;
        let current = state
            .repositories
            .iter()
            .find(|r| r.repository_id == repo.repository_id)
            .ok_or(PoolError::Unregistered)?;
        let count = state
            .worktrees
            .iter()
            .filter(|w| w.repository_id == repo.repository_id)
            .count();
        if count >= current.capacity as usize {
            let assigned_count = state
                .assignments
                .iter()
                .filter(|a| {
                    a.repository_id == repo.repository_id && a.state != AssignmentState::Released
                })
                .count();
            return Err(PoolError::PoolExhausted {
                repository_id: repo.repository_id,
                maximum: current.capacity,
                registered_count: count,
                assigned_count,
            });
        }
    }
    let common = repo.common_directory.to_path()?;
    let worktree_id = WorktreeId::new();
    let path = fs::canonicalize(paths.catalog.parent().ok_or(PoolError::Configuration)?)?
        .join("worktrees")
        .join(repo.repository_id.to_string())
        .join(worktree_id.to_string());
    let worktree = Worktree {
        worktree_id,
        repository_id: repo.repository_id,
        path: EncodedPath::from_path(&path),
        git_directory: EncodedPath::from_path(
            &common.join("worktrees").join(worktree_id.to_string()),
        ),
        last_release_position: None,
    };
    let assignment = Assignment {
        assignment_handle: AssignmentHandle::new(),
        operation_id: OperationId::new(),
        repository_id: repo.repository_id,
        worktree_id,
        path: worktree.path.clone(),
        resolved_commit: target.into(),
        branch: None,
        state: AssignmentState::Preparing,
    };
    let creation_id = OperationId::new();
    let creation_cause;
    {
        let session = catalog::open(paths)?;
        let registered = management_event(
            session.projection.catalog_id,
            &ManagementEvent::Creation(CreationEvent::Registered {
                operation_id: creation_id,
                worktree: Box::new(worktree.clone()),
                assignment: Box::new(assignment.clone()),
            }),
        )?;
        creation_cause = registered["id"]
            .as_str()
            .ok_or(PoolError::Corrupt)?
            .to_owned();
        let reserved = management_event(
            session.projection.catalog_id,
            &ManagementEvent::Acquisition(AcquisitionEvent::Reserved(assignment.clone())),
        )?;
        session
            .store()
            .append_batch(session.projection.revision, vec![registered, reserved])?;
    }
    Ok(ReservedCreation {
        worktree,
        assignment,
        creation_id,
        creation_cause,
    })
}

fn prepare_path(path: &Path, git_directory: &Path) -> Result<(u64, u64), PoolError> {
    let repository_root = path.parent().ok_or(PoolError::Configuration)?;
    let root = repository_root.parent().ok_or(PoolError::Configuration)?;
    private_directory(root)?;
    fs::File::open(root.parent().ok_or(PoolError::Configuration)?)?.sync_all()?;
    private_directory(repository_root)?;
    fs::File::open(root)?.sync_all()?;
    // Existing destination or administrative metadata is never permission to force/replace.
    for candidate in [path, git_directory] {
        match fs::symlink_metadata(candidate) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
            Ok(_) => return Err(PoolError::Conflict),
        }
    }
    fs::DirBuilder::new().mode(0o700).create(path)?;
    fs::File::open(repository_root)?.sync_all()?;
    let metadata = fs::symlink_metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
}
fn validate_prepared_path(
    path: &Path,
    git_directory: &Path,
    identity: (u64, u64),
) -> Result<(), PoolError> {
    let metadata = fs::symlink_metadata(path)?;
    if (metadata.dev(), metadata.ino()) != identity
        || !metadata.is_dir()
        || fs::canonicalize(path)? != path
        || fs::read_dir(path)?.next().is_some()
    {
        return Err(PoolError::Conflict);
    }
    private_directory(path)?;
    match fs::symlink_metadata(git_directory) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
        Ok(_) => Err(PoolError::Conflict),
    }
}

fn finish(
    paths: &Paths,
    assignment: &Assignment,
    creation_id: OperationId,
    creation_cause: &str,
    checkout_cause: Option<&str>,
    successful: bool,
    observe: &mut impl FnMut(AcquisitionCheckpoint),
) -> Result<(CatalogProjection, Assignment), PoolError> {
    let state = {
        let session = catalog::open(paths)?;
        let mut events = vec![management_event_caused_by(
            session.projection.catalog_id,
            &ManagementEvent::Creation(CreationEvent::Finished {
                operation_id: creation_id,
                repository_id: assignment.repository_id,
                worktree_id: assignment.worktree_id,
                successful,
            }),
            creation_cause,
        )?];
        if let Some(cause) = checkout_cause {
            events.push(management_event_caused_by(
                session.projection.catalog_id,
                &ManagementEvent::Acquisition(AcquisitionEvent::Finished {
                    operation_id: assignment.operation_id,
                    repository_id: assignment.repository_id,
                    worktree_id: assignment.worktree_id,
                    successful,
                }),
                cause,
            )?);
        }
        session
            .store()
            .append_batch(session.projection.revision, events)?
    };
    observe(AcquisitionCheckpoint::ResultCommitted);
    let assignment = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle == assignment.assignment_handle)
        .cloned()
        .ok_or(PoolError::Corrupt)?;
    Ok((state, assignment))
}
