//! Synchronous enrollment orchestration over real Git, locks and event storage.
use std::{ffi::OsStr, path::Path};

use crate::{
    catalog,
    coordination::LockGuard,
    domain::{CatalogProjection, decode_event, management_event, management_event_caused_by},
    error::PoolError,
    git,
    management::{ManagementEvent, Repository, RepositoryId, Worktree, WorktreeId},
    paths::{EncodedPath, Paths},
};

/// # Errors
/// Rejects unsafe catalog state or unavailable stable coordination files.
pub fn maintenance(paths: &Paths) -> Result<LockGuard, PoolError> {
    if !paths.state.join("active.json").exists() {
        return Err(PoolError::Missing);
    }
    LockGuard::acquire(&paths.state.join("maintenance.lock"), false)
}
/// # Errors
/// Rejects unregistered repositories or conflicting path/identity selectors.
pub fn repository(
    state: &CatalogProjection,
    selector: Option<&OsStr>,
) -> Result<Repository, PoolError> {
    if let Some(id) = selector
        .and_then(OsStr::to_str)
        .and_then(|s| s.parse::<RepositoryId>().ok())
    {
        return state
            .repositories
            .iter()
            .find(|r| r.repository_id == id)
            .cloned()
            .ok_or(PoolError::Unregistered);
    }
    let cwd = std::env::current_dir()?;
    let path = selector.map_or(cwd.as_path(), Path::new);
    let observed = git::checkout(path)?;
    state
        .repositories
        .iter()
        .find(|r| {
            r.common_directory.bytes == EncodedPath::from_path(&observed.common_directory).bytes
        })
        .cloned()
        .ok_or(PoolError::Unregistered)
}
/// # Errors
/// Rejects Git/catalog failures without changing existing checkout state.
pub fn register_repository(
    paths: &Paths,
    path: &Path,
    selector: Option<&OsStr>,
) -> Result<(CatalogProjection, Repository), PoolError> {
    let _maintenance = maintenance(paths)?;
    let observed = git::checkout(path)?;
    if let Some(selector) = selector {
        let common = if selector
            .to_str()
            .is_some_and(|s| s.parse::<RepositoryId>().is_ok())
        {
            let state = catalog::open(paths)?.projection;
            repository(&state, Some(selector))?.common_directory
        } else {
            EncodedPath::from_path(&git::checkout(Path::new(selector))?.common_directory)
        };
        if common.bytes != EncodedPath::from_path(&observed.common_directory).bytes {
            return Err(PoolError::Selectors);
        }
    }
    let _enrollment = LockGuard::acquire(&paths.state.join("repository-enrollment.lock"), true)?;
    let session = catalog::open(paths)?;
    if let Some(existing) = session.projection.repositories.iter().find(|r| {
        r.common_directory.bytes == EncodedPath::from_path(&observed.common_directory).bytes
    }) {
        let existing = existing.clone();
        return Ok((session.projection, existing));
    }
    let repository = Repository {
        repository_id: RepositoryId::new(),
        common_directory: EncodedPath::from_path(&observed.common_directory),
        context_path: EncodedPath::from_path(&observed.path),
        capacity: 4,
        revision: 0,
    };
    let event = management_event(
        session.projection.catalog_id,
        &ManagementEvent::RepositoryRegistered(repository.clone()),
    )?;
    let state = session.store().append(
        decode_event(&event, session.projection.catalog_id)?
            .expected_revision(&session.projection)?,
        event,
    )?;
    Ok((state, repository))
}
/// # Errors
/// Rejects mismatches and capacity overflow before durable enrollment.
pub fn register_worktree(
    paths: &Paths,
    path: &Path,
    selector: Option<&OsStr>,
) -> Result<(CatalogProjection, Worktree), PoolError> {
    let _maintenance = maintenance(paths)?;
    let observed = git::checkout(path)?;
    let state = {
        let session = catalog::open(paths)?;
        session.projection
    };
    let repo = repository(&state, selector.or(Some(path.as_os_str())))?;
    if repo.common_directory.bytes != EncodedPath::from_path(&observed.common_directory).bytes {
        return Err(PoolError::Selectors);
    }
    let _repo = LockGuard::acquire(
        &paths
            .state
            .join(format!("repository-{}.lock", repo.repository_id)),
        true,
    )?;
    let session = catalog::open(paths)?;
    if let Some(existing) =
        session.projection.worktrees.iter().find(|w| {
            w.git_directory.bytes == EncodedPath::from_path(&observed.git_directory).bytes
        })
    {
        if existing.path.bytes != EncodedPath::from_path(&observed.path).bytes {
            return Err(PoolError::Selectors);
        }
        let existing = existing.clone();
        return Ok((session.projection, existing));
    }
    let worktree = Worktree {
        worktree_id: WorktreeId::new(),
        repository_id: repo.repository_id,
        path: EncodedPath::from_path(&observed.path),
        git_directory: EncodedPath::from_path(&observed.git_directory),
        last_release_position: None,
    };
    let event = management_event(
        session.projection.catalog_id,
        &ManagementEvent::WorktreeEnrolled(worktree.clone()),
    )?;
    let state = session.store().append(
        decode_event(&event, session.projection.catalog_id)?
            .expected_revision(&session.projection)?,
        event,
    )?;
    Ok((state, worktree))
}

/// A checkpoint callback observes the same committed boundaries as the production workflow.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefreshCheckpoint {
    IntentRecorded,
    FetchObserved,
    ResultCommitted,
}

/// # Errors
/// Rejects selector/catalog failures before effects; failed commit is indeterminate.
pub fn refresh(
    paths: &Paths,
    selector: Option<&OsStr>,
) -> Result<(CatalogProjection, crate::management::RefreshOperation), PoolError> {
    refresh_observed(paths, selector, |_| {})
}
/// # Errors
/// Preserves durable pending barriers across effect failure or interruption.
pub fn refresh_observed(
    paths: &Paths,
    selector: Option<&OsStr>,
    observe: impl FnMut(RefreshCheckpoint),
) -> Result<(CatalogProjection, crate::management::RefreshOperation), PoolError> {
    let _maintenance = maintenance(paths)?;
    let state = {
        let session = catalog::open(paths)?;
        session.projection
    };
    let repo = repository(&state, selector)?;
    let _repo = LockGuard::acquire(
        &paths
            .state
            .join(format!("repository-{}.lock", repo.repository_id)),
        true,
    )?;
    refresh_locked(paths, &repo, observe)
}
/// Executes refresh while the caller owns maintenance and repository coordination.
/// # Errors
/// Retains durable pending work on failed or interrupted effects.
pub(crate) fn refresh_locked(
    paths: &Paths,
    repo: &Repository,
    mut observe: impl FnMut(RefreshCheckpoint),
) -> Result<(CatalogProjection, crate::management::RefreshOperation), PoolError> {
    use crate::management::{OperationId, RefreshFinished, RefreshStarted, RefreshState};
    let operation_id = OperationId::new();
    let intent_event_id;
    {
        let session = catalog::open(paths)?;
        if let Some(pending) =
            session.projection.operations.iter().find(|o| {
                o.repository_id == repo.repository_id && o.state != RefreshState::Completed
            })
        {
            let pending = pending.clone();
            return Ok((session.projection, pending));
        }
        let event = management_event(
            session.projection.catalog_id,
            &ManagementEvent::RefreshStarted(RefreshStarted {
                operation_id,
                repository_id: repo.repository_id,
            }),
        )?;
        intent_event_id = event["id"].as_str().ok_or(PoolError::Corrupt)?.to_owned();
        session.store().append(
            decode_event(&event, session.projection.catalog_id)?
                .expected_revision(&session.projection)?,
            event,
        )?;
    }
    observe(RefreshCheckpoint::IntentRecorded);
    // No catalog handle/lock remains across a Git subprocess. std's lock FDs are close-on-exec.
    let path = repo.common_directory.to_path()?;
    let resolved_commit = git::refresh_origin_main(&path).ok();
    observe(RefreshCheckpoint::FetchObserved);
    let state = {
        let session = catalog::open(paths)?;
        let event = management_event_caused_by(
            session.projection.catalog_id,
            &ManagementEvent::RefreshFinished(RefreshFinished {
                operation_id,
                repository_id: repo.repository_id,
                resolved_commit,
            }),
            &intent_event_id,
        )?;
        session.store().append(
            decode_event(&event, session.projection.catalog_id)?
                .expected_revision(&session.projection)?,
            event,
        )?
    };
    observe(RefreshCheckpoint::ResultCommitted);
    let operation = state
        .operations
        .iter()
        .find(|o| o.operation_id == operation_id)
        .cloned()
        .ok_or(PoolError::Corrupt)?;
    Ok((state, operation))
}

/// Changes durable capacity without removing or hiding registered records.
/// # Errors
/// Rejects a maximum below the current count or invalid catalog/selector state.
pub fn configure_capacity(
    paths: &Paths,
    selector: Option<&OsStr>,
    maximum: u32,
) -> Result<(CatalogProjection, Repository), PoolError> {
    let _maintenance = maintenance(paths)?;
    let state = catalog::open(paths)?.projection;
    let repo = repository(&state, selector)?;
    let _repo = LockGuard::acquire(
        &paths
            .state
            .join(format!("repository-{}.lock", repo.repository_id)),
        true,
    )?;
    let session = catalog::open(paths)?;
    let fact = ManagementEvent::CapacityConfigured {
        repository_id: repo.repository_id,
        maximum,
    };
    let event = management_event(session.projection.catalog_id, &fact)?;
    let state = session.store().append(
        decode_event(&event, session.projection.catalog_id)?
            .expected_revision(&session.projection)?,
        event,
    )?;
    let repo = repository(&state, Some(OsStr::new(&repo.repository_id.to_string())))?;
    Ok((state, repo))
}
