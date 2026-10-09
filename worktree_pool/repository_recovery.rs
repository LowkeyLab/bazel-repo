//! Explicit reconciliation of interrupted refreshes without repeating fetch.
use std::ffi::OsStr;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    catalog,
    coordination::LockGuard,
    domain::{CatalogProjection, management_event_caused_by},
    error::PoolError,
    git,
    management::{ManagementEvent, OperationId, RefreshOperation, RefreshState, RepositoryId},
    paths::Paths,
    workflows,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryRecoveryState {
    Intended,
    Completed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryRecoveryOperation {
    pub operation_id: OperationId,
    pub repository_id: RepositoryId,
    pub target_operation_id: OperationId,
    pub target_checkpoint: String,
    pub state: RepositoryRecoveryState,
    pub intent_event_id: String,
    pub last_checkpoint: String,
    /// A local observation; never evidence that the interrupted fetch completed.
    pub observed_commit: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RepositoryRecoveryEvent {
    Started(RepositoryRecoveryOperation),
    Finished {
        operation_id: OperationId,
        repository_id: RepositoryId,
        target_operation_id: OperationId,
        observed_commit: Option<String>,
    },
}

impl RepositoryRecoveryEvent {
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Started(_) => "io.lowkeylab.worktreepool.repository.recovery.started.v1",
            Self::Finished { .. } => "io.lowkeylab.worktreepool.repository.recovery.finished.v1",
        }
    }

    #[must_use]
    pub const fn operation_id(&self) -> OperationId {
        match self {
            Self::Started(operation) => operation.operation_id,
            Self::Finished { operation_id, .. } => *operation_id,
        }
    }

    #[must_use]
    pub const fn repository_id(&self) -> RepositoryId {
        match self {
            Self::Started(operation) => operation.repository_id,
            Self::Finished { repository_id, .. } => *repository_id,
        }
    }

    /// Applies typed observations without Git effects or freshness inference.
    /// # Errors
    /// Rejects inconsistent identities, malformed facts, stale checkpoints and forged causation.
    pub fn apply(
        &self,
        state: &mut CatalogProjection,
        event_id: &str,
        cause: Option<&str>,
    ) -> Result<(), PoolError> {
        if !uuid::Uuid::parse_str(event_id)
            .is_ok_and(|id| !id.is_nil() && id.to_string() == event_id)
        {
            return Err(PoolError::Corrupt);
        }
        match self {
            Self::Started(operation) => {
                let target = refresh(state, operation.target_operation_id)?;
                if target.repository_id != operation.repository_id
                    || !target.state.is_pending()
                    || target.last_checkpoint != operation.target_checkpoint
                    || cause != Some(target.intent_event_id.as_str())
                    || operation.state != RepositoryRecoveryState::Intended
                    || !operation.intent_event_id.is_empty()
                    || operation.last_checkpoint != "repository_recovery_intended"
                    || operation.observed_commit.is_some()
                    || state.has_pending_recovery(operation.repository_id)
                    || state.operation_identity_used(operation.operation_id)
                    || state.repository_recoveries.iter().any(|other| {
                        other.target_operation_id == operation.target_operation_id
                            || (other.repository_id == operation.repository_id
                                && other.state != RepositoryRecoveryState::Completed)
                    })
                {
                    return Err(PoolError::Conflict);
                }
                let mut operation = operation.clone();
                operation.intent_event_id = event_id.into();
                state.repository_recoveries.push(operation);
            }
            Self::Finished {
                operation_id,
                repository_id,
                target_operation_id,
                observed_commit,
            } => {
                if observed_commit
                    .as_deref()
                    .is_some_and(|oid| !valid_oid(oid))
                {
                    return Err(PoolError::Corrupt);
                }
                let index = state
                    .repository_recoveries
                    .iter()
                    .position(|operation| {
                        operation.operation_id == *operation_id
                            && operation.repository_id == *repository_id
                            && operation.target_operation_id == *target_operation_id
                    })
                    .ok_or(PoolError::Corrupt)?;
                let operation = &state.repository_recoveries[index];
                let target = refresh(state, *target_operation_id)?;
                if operation.state != RepositoryRecoveryState::Intended
                    || cause != Some(operation.intent_event_id.as_str())
                    || target.repository_id != *repository_id
                    || !target.state.is_pending()
                    || target.last_checkpoint != operation.target_checkpoint
                {
                    return Err(PoolError::Conflict);
                }
                // Validate everything before changing either projection. No RefreshFinished
                // success fact is fabricated from a remote-tracking reference observation.
                let target = state
                    .operations
                    .iter_mut()
                    .find(|target| target.operation_id == *target_operation_id)
                    .ok_or(PoolError::Corrupt)?;
                target.state = RefreshState::Reconciled;
                target.last_checkpoint = "refresh_reconciled".into();
                let operation = &mut state.repository_recoveries[index];
                operation.state = RepositoryRecoveryState::Completed;
                operation.last_checkpoint = "repository_recovery_committed".into();
                operation.observed_commit.clone_from(observed_commit);
            }
        }
        Ok(())
    }
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepositoryRecoveryCheckpoint {
    IntentCommitted,
    ResultCommitted,
}

/// Whether an exact selector names a refresh or its recorded recovery.
#[must_use]
pub fn recognizes(state: &CatalogProjection, id: &str) -> bool {
    let Ok(id) = id.parse::<OperationId>() else {
        return false;
    };
    state
        .operations
        .iter()
        .any(|operation| operation.operation_id == id)
        || state
            .repository_recoveries
            .iter()
            .any(|operation| operation.operation_id == id)
}

/// Observes the selected refresh or recovery without writing storage or fetching.
/// # Errors
/// Rejects unknown identities, conflicting repository selectors and invalid authority.
pub fn preview(
    paths: &Paths,
    selector: Option<&OsStr>,
    id: &str,
) -> Result<(CatalogProjection, Value), PoolError> {
    reconcile(paths, selector, id, false, &mut |_| {})
}

/// Reconciles only the recorded identity; freshness requires a new explicit refresh.
/// # Errors
/// Retains the recorded intent on storage failure or an unobserved indeterminate commit.
pub fn resume(
    paths: &Paths,
    selector: Option<&OsStr>,
    id: &str,
) -> Result<(CatalogProjection, Value), PoolError> {
    resume_observed(paths, selector, id, |_| {})
}

/// # Errors
/// Preserves the same durable boundaries and exact identity selection as production.
pub fn resume_observed(
    paths: &Paths,
    selector: Option<&OsStr>,
    id: &str,
    mut observe: impl FnMut(RepositoryRecoveryCheckpoint),
) -> Result<(CatalogProjection, Value), PoolError> {
    reconcile(paths, selector, id, true, &mut observe)
}

fn reconcile(
    paths: &Paths,
    selector: Option<&OsStr>,
    id: &str,
    apply: bool,
    observe: &mut impl FnMut(RepositoryRecoveryCheckpoint),
) -> Result<(CatalogProjection, Value), PoolError> {
    let id = id
        .parse::<OperationId>()
        .map_err(|_| PoolError::Unregistered)?;
    let _maintenance = workflows::maintenance(paths)?;
    let before = catalog::inspect_projection(paths)?;
    let repository_id = selected_refresh(&before, id)?.repository_id;
    validate_selector(&before, selector, repository_id)?;
    let _repository = LockGuard::acquire(
        &paths.state.join(format!("repository-{repository_id}.lock")),
        true,
    )?;
    let mut state = catalog::inspect_projection(paths)?;
    let target = selected_refresh(&state, id)?.clone();
    if target.repository_id != repository_id {
        return Err(PoolError::Conflict);
    }
    validate_selector(&state, selector, repository_id)?;
    if !apply || target.state == RefreshState::Completed {
        return result(state, id);
    }
    let recovery_id = if let Some(operation) =
        state.repository_recoveries.iter().find(|operation| {
            operation.operation_id == id || operation.target_operation_id == target.operation_id
        }) {
        if operation.state == RepositoryRecoveryState::Completed {
            return result(state, id);
        }
        operation.operation_id
    } else {
        let operation_id = OperationId::new();
        let operation = RepositoryRecoveryOperation {
            operation_id,
            repository_id,
            target_operation_id: target.operation_id,
            target_checkpoint: target.last_checkpoint,
            state: RepositoryRecoveryState::Intended,
            intent_event_id: String::new(),
            last_checkpoint: "repository_recovery_intended".into(),
            observed_commit: None,
        };
        state = append(
            paths,
            RepositoryRecoveryEvent::Started(operation),
            &target.intent_event_id,
        )?;
        observe(RepositoryRecoveryCheckpoint::IntentCommitted);
        operation_id
    };
    let repository = state
        .repositories
        .iter()
        .find(|repository| repository.repository_id == repository_id)
        .ok_or(PoolError::Corrupt)?;
    // Only a local read: storage is closed and the short catalog lock is released.
    let observed_commit = git::resolve_commit(
        &repository.common_directory.to_path()?,
        OsStr::new("refs/remotes/origin/main"),
    )
    .ok();
    let operation = state
        .repository_recoveries
        .iter()
        .find(|operation| operation.operation_id == recovery_id)
        .ok_or(PoolError::Corrupt)?;
    state = append(
        paths,
        RepositoryRecoveryEvent::Finished {
            operation_id: recovery_id,
            repository_id,
            target_operation_id: operation.target_operation_id,
            observed_commit,
        },
        &operation.intent_event_id,
    )?;
    observe(RepositoryRecoveryCheckpoint::ResultCommitted);
    result(state, recovery_id)
}

fn validate_selector(
    state: &CatalogProjection,
    selector: Option<&OsStr>,
    repository_id: RepositoryId,
) -> Result<(), PoolError> {
    if selector.is_some() && workflows::repository(state, selector)?.repository_id != repository_id
    {
        return Err(PoolError::Selectors);
    }
    Ok(())
}

fn refresh(state: &CatalogProjection, id: OperationId) -> Result<&RefreshOperation, PoolError> {
    state
        .operations
        .iter()
        .find(|operation| operation.operation_id == id)
        .ok_or(PoolError::Unregistered)
}

fn selected_refresh(
    state: &CatalogProjection,
    id: OperationId,
) -> Result<&RefreshOperation, PoolError> {
    let target = state
        .repository_recoveries
        .iter()
        .find(|operation| operation.operation_id == id)
        .map_or(id, |operation| operation.target_operation_id);
    refresh(state, target)
}

fn result(
    state: CatalogProjection,
    id: OperationId,
) -> Result<(CatalogProjection, Value), PoolError> {
    let target = selected_refresh(&state, id)?;
    let recovery = state.repository_recoveries.iter().find(|operation| {
        operation.operation_id == id || operation.target_operation_id == target.operation_id
    });
    let fresh = target.state == RefreshState::Completed;
    let data = json!({
        "repository_id": target.repository_id,
        "worktree_id": null,
        "assignment_handle": null,
        "operation_id": recovery.map_or(target.operation_id, |operation| operation.operation_id),
        "operation": recovery.map_or_else(|| json!(target), |operation| json!(operation)),
        "target_operation_id": target.operation_id,
        "target_refresh": target,
        "ownership": null,
        "availability": null,
        "fresh_fetch_proven": fresh,
        "observed_commit": recovery.and_then(|operation| operation.observed_commit.as_deref()),
        "revision": state.revision,
        "next_action": if fresh { "the recorded refresh result remains authoritative" }
            else { "run a new explicit repository refresh or acquisition before using a fresh origin/main" },
    });
    Ok((state, data))
}

fn append(
    paths: &Paths,
    event: RepositoryRecoveryEvent,
    cause: &str,
) -> Result<CatalogProjection, PoolError> {
    let (wire, id, expected, result) = {
        let session = catalog::open(paths)?;
        let wire = management_event_caused_by(
            session.projection.catalog_id,
            &ManagementEvent::RepositoryRecovery(event),
            cause,
        )?;
        let id = wire["id"].as_str().ok_or(PoolError::Corrupt)?.to_owned();
        let result = session
            .store()
            .append_batch(session.projection.revision, vec![wire.clone()]);
        (wire, id, session.projection.revision, result)
    };
    match result {
        Err(PoolError::CommitUnknown) => {
            // Observe the exact event and revision through complete readonly validation.
            let (projection, events) =
                catalog::inspect_events(paths).map_err(|_| PoolError::CommitUnknown)?;
            if events.iter().enumerate().any(|(position, event)| {
                u64::try_from(position) == Ok(expected)
                    && event["id"].as_str() == Some(&id)
                    && event == &wire
            }) {
                Ok(projection)
            } else {
                Err(PoolError::CommitUnknown)
            }
        }
        result => result,
    }
}
