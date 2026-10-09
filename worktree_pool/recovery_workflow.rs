//! Explicit recovery observations and lock-scoped reconciliation over real adapters.
use std::ffi::OsStr;

use serde_json::{Value, json};

use crate::{
    acquisition::AssignmentState, catalog, domain::CatalogProjection, error::PoolError,
    paths::Paths, workflows,
};

/// Read-only observation of one recorded handle; never select a newer assignment.
/// # Errors
/// Rejects unknown handles, conflicting selectors and invalid catalog state.
pub fn preview_assignment(
    paths: &Paths,
    selector: Option<&OsStr>,
    handle: &str,
) -> Result<(CatalogProjection, Value), PoolError> {
    let _maintenance = workflows::maintenance(paths)?;
    let state = catalog::inspect_projection(paths)?;
    let repository_id = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle.to_string() == handle)
        .ok_or(PoolError::UnknownAssignment)?
        .repository_id;
    let _repository = crate::coordination::LockGuard::acquire(
        &paths.state.join(format!("repository-{repository_id}.lock")),
        true,
    )?;
    assignment_preview(catalog::inspect_projection(paths)?, selector, handle)
}
fn assignment_preview(
    state: CatalogProjection,
    selector: Option<&OsStr>,
    handle: &str,
) -> Result<(CatalogProjection, Value), PoolError> {
    let assignment = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle.to_string() == handle)
        .ok_or(PoolError::UnknownAssignment)?;
    if selector.is_some()
        && workflows::repository(&state, selector)?.repository_id != assignment.repository_id
    {
        return Err(PoolError::Selectors);
    }
    let worktree = crate::recovery_inspection::recorded_worktree(&state, assignment.worktree_id)?;
    let observed = crate::recovery_inspection::worktree_data(&state, worktree)?;
    let data = json!({
        "observation":observed["observation"],
        "repository_id":assignment.repository_id,
        "worktree_id":assignment.worktree_id,
        "assignment_handle":assignment.assignment_handle,
        "ownership":assignment.state,
        "availability":if assignment.state == AssignmentState::Released { Value::Null } else { json!("withheld") },
        "assignment":assignment,
        "next_action":if assignment.state == AssignmentState::Released {"this historical handle grants no current ownership"} else {"inspect protected work before explicit reconciliation"},
        "revision":state.revision,
    });
    Ok((state, data))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryCheckpoint {
    IntentCommitted,
    PreservationObserved,
    PreservationCommitted,
    ResultCommitted,
}
/// # Errors
/// Refuses unfinished or uncertain work and preserves the actual detached tip before release.
pub fn abandon_assignment(
    paths: &Paths,
    selector: Option<&OsStr>,
    handle: &str,
) -> Result<(CatalogProjection, Value), PoolError> {
    abandon_assignment_observed(paths, selector, handle, |_| {})
}
/// # Errors
/// Retains ownership on interruption or failed preservation and inspects indeterminate commits.
pub fn abandon_assignment_observed(
    paths: &Paths,
    selector: Option<&OsStr>,
    handle: &str,
    mut observe: impl FnMut(RecoveryCheckpoint),
) -> Result<(CatalogProjection, Value), PoolError> {
    use crate::{
        coordination::LockGuard,
        git,
        recovery::{RecoveryEvent, RecoveryState},
    };
    let _maintenance = workflows::maintenance(paths)?;
    let (state, _) = preview_assignment(paths, selector, handle)?;
    let assignment = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle.to_string() == handle)
        .ok_or(PoolError::UnknownAssignment)?;
    let repository_id = assignment.repository_id;
    let _repository = LockGuard::acquire(
        &paths.state.join(format!("repository-{repository_id}.lock")),
        true,
    )?;
    let state = catalog::inspect_projection(paths)?;
    let assignment = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle.to_string() == handle)
        .ok_or(PoolError::UnknownAssignment)?;
    if assignment.state == AssignmentState::Released {
        return assignment_preview(state, selector, handle);
    }
    let worktree = state
        .worktrees
        .iter()
        .find(|w| w.worktree_id == assignment.worktree_id)
        .ok_or(PoolError::Corrupt)?;
    let repository = state
        .repositories
        .iter()
        .find(|r| r.repository_id == repository_id)
        .ok_or(PoolError::Corrupt)?;
    let path = worktree.path.to_path()?;
    let common = repository.common_directory.to_path()?;
    let admin = worktree.git_directory.to_path()?;
    validate_binding(&path, &common, &admin)?;
    let tip = git::resolve_commit(&path, OsStr::new("HEAD"))?;
    let safety = git::observe_safe(&path, &tip)?;
    if let Some(op) = state
        .recoveries
        .iter()
        .find(|o| {
            o.assignment_handle == Some(assignment.assignment_handle)
                && o.state != RecoveryState::Completed
        })
        .cloned()
    {
        return continue_recovery(paths, state, &op, &mut observe);
    }
    let (op, cause) = abandonment_intent(&state, assignment, safety)?;
    let state = append(paths, RecoveryEvent::Started(op.clone()), cause.as_deref())?;
    observe(RecoveryCheckpoint::IntentCommitted);
    continue_recovery(paths, state, &op, &mut observe)
}
/// Read-only known recovery identity inspection; never guess another operation.
/// # Errors
/// Rejects unknown identities or conflicting repository selectors.
pub fn preview_recovery(
    paths: &Paths,
    selector: Option<&OsStr>,
    id: &str,
) -> Result<(CatalogProjection, Value), PoolError> {
    let _maintenance = workflows::maintenance(paths)?;
    let state = catalog::inspect_projection(paths)?;
    if crate::repository_recovery::recognizes(&state, id) {
        return crate::repository_recovery::preview(paths, selector, id);
    }
    let operation_id = id.parse().map_err(|_| PoolError::Unregistered)?;
    let repository_id = if let Some(op) = state
        .recoveries
        .iter()
        .find(|o| o.operation_id == operation_id || o.target_operation_id == Some(operation_id))
    {
        op.repository_id
    } else if let Some(op) = state
        .releases
        .iter()
        .find(|o| o.operation_id == operation_id)
    {
        op.repository_id
    } else {
        crate::recovery::preparation_target(&state, operation_id)?
            .0
            .repository_id
    };
    if selector.is_some() && workflows::repository(&state, selector)?.repository_id != repository_id
    {
        return Err(PoolError::Selectors);
    }
    let _repository = crate::coordination::LockGuard::acquire(
        &paths.state.join(format!("repository-{repository_id}.lock")),
        true,
    )?;
    operation_preview(catalog::inspect_projection(paths)?, selector, id)
}
fn operation_preview(
    state: CatalogProjection,
    selector: Option<&OsStr>,
    id: &str,
) -> Result<(CatalogProjection, Value), PoolError> {
    let operation_id = id.parse().map_err(|_| PoolError::Unregistered)?;
    if !state
        .recoveries
        .iter()
        .any(|o| o.operation_id == operation_id)
    {
        if let Some(id) = state
            .recoveries
            .iter()
            .find(|o| {
                o.target_operation_id == Some(operation_id)
                    && o.state == crate::recovery::RecoveryState::Completed
            })
            .map(|o| o.operation_id)
        {
            return recovery_result(state, id);
        }
        if state
            .releases
            .iter()
            .any(|r| r.operation_id == operation_id)
        {
            return release_preview(state, selector, operation_id);
        }
        return acquisition_preview(state, selector, operation_id);
    }
    let op = recovery(&state, operation_id)?;
    if selector.is_some()
        && workflows::repository(&state, selector)?.repository_id != op.repository_id
    {
        return Err(PoolError::Selectors);
    }
    recovery_result(state, operation_id)
}
/// # Errors
/// Resumes only the named durable intent, observing preservation before any retry.
pub fn resume_recovery(
    paths: &Paths,
    selector: Option<&OsStr>,
    id: &str,
) -> Result<(CatalogProjection, Value), PoolError> {
    use crate::coordination::LockGuard;
    let _maintenance = workflows::maintenance(paths)?;
    let (state, _) = preview_recovery(paths, selector, id)?;
    if crate::repository_recovery::recognizes(&state, id) {
        return crate::repository_recovery::resume(paths, selector, id);
    }
    let operation_id = id.parse().map_err(|_| PoolError::Unregistered)?;
    if !state
        .recoveries
        .iter()
        .any(|o| o.operation_id == operation_id)
    {
        if state
            .releases
            .iter()
            .any(|r| r.operation_id == operation_id)
        {
            return reconcile_release(paths, selector, operation_id);
        }
        return reconcile_acquisition(paths, selector, operation_id, false);
    }
    let repository_id = recovery(&state, operation_id)?.repository_id;
    let _repository = LockGuard::acquire(
        &paths.state.join(format!("repository-{repository_id}.lock")),
        true,
    )?;
    let state = catalog::inspect_projection(paths)?;
    let op = recovery(&state, operation_id)?.clone();
    continue_recovery(paths, state, &op, &mut |_| {})
}
fn acquisition_preview(
    state: CatalogProjection,
    selector: Option<&OsStr>,
    id: crate::management::OperationId,
) -> Result<(CatalogProjection, Value), PoolError> {
    let (target, _) = crate::recovery::preparation_target(&state, id)?;
    if selector.is_some()
        && workflows::repository(&state, selector)?.repository_id != target.repository_id
    {
        return Err(PoolError::Selectors);
    }
    let a = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle == target.assignment_handle)
        .ok_or(PoolError::Corrupt)?;
    let observed = crate::recovery_inspection::worktree_data(
        &state,
        crate::recovery_inspection::recorded_worktree(&state, a.worktree_id)?,
    )?;
    let creation = state.creations.iter().find(|c| c.operation_id == id);
    let operation = creation.map_or_else(|| json!(target), |c| json!(c));
    let data = json!({"repository_id":a.repository_id,"worktree_id":a.worktree_id,"assignment_handle":a.assignment_handle,"operation_id":id,"operation":operation,"assignment":a,"observation":observed["observation"],"ownership":a.state,"availability":if a.state==AssignmentState::Released {Value::Null}else{json!("withheld")},"revision":state.revision,"next_action":"explicit apply observes the recorded checkout; it does not repeat checkout or creation"});
    Ok((state, data))
}
fn reconcile_acquisition(
    paths: &Paths,
    selector: Option<&OsStr>,
    id: crate::management::OperationId,
    fresh: bool,
) -> Result<(CatalogProjection, Value), PoolError> {
    use crate::{
        coordination::LockGuard,
        management::OperationId,
        recovery::{RecoveryDisposition, RecoveryEvent, RecoveryOperation, RecoveryState},
    };
    let state = catalog::inspect_projection(paths)?;
    let (target, _) = crate::recovery::preparation_target(&state, id)?;
    let repository_id = target.repository_id;
    if selector.is_some() && workflows::repository(&state, selector)?.repository_id != repository_id
    {
        return Err(PoolError::Selectors);
    }
    let _repository = LockGuard::acquire(
        &paths.state.join(format!("repository-{repository_id}.lock")),
        true,
    )?;
    let state = catalog::inspect_projection(paths)?;
    let (target, cause) = crate::recovery::preparation_target(&state, id)?;
    if target.state == crate::acquisition::AcquisitionState::Completed {
        return acquisition_preview(state, selector, id);
    }
    if !fresh && target.state == crate::acquisition::AcquisitionState::Reconciled {
        let recorded = state
            .recoveries
            .iter()
            .find(|o| o.target_operation_id == Some(id) && o.state == RecoveryState::Completed)
            .ok_or(PoolError::Corrupt)?
            .operation_id;
        return recovery_result(state, recorded);
    }
    let original_pending = target.state.is_pending();
    if let Some(op) = state
        .recoveries
        .iter()
        .find(|o| o.target_operation_id == Some(id) && o.state != RecoveryState::Completed)
        .cloned()
    {
        return continue_recovery(paths, state, &op, &mut |_| {});
    }
    let a = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle == target.assignment_handle)
        .ok_or(PoolError::Corrupt)?;
    let (disposition, safety, reason) = observe_preparation(&state, a, target)?;
    let operation_id = OperationId::new();
    let op = RecoveryOperation {
        operation_id,
        repository_id,
        worktree_id: a.worktree_id,
        assignment_handle: Some(a.assignment_handle),
        state: RecoveryState::Intended,
        tip: safety.as_ref().map(|s| s.head.clone()),
        branch: safety.and_then(|s| s.branch),
        preservation_reference: (disposition == RecoveryDisposition::Active)
            .then(|| format!("refs/worktree-pool/{operation_id}")),
        intent_event_id: String::new(),
        last_checkpoint: "recovery_intended".into(),
        target_operation_id: original_pending.then_some(id),
        disposition,
        protected_reason: reason.map(str::to_owned),
    };
    let state = append(
        paths,
        RecoveryEvent::Started(op.clone()),
        original_pending.then_some(cause),
    )?;
    continue_recovery(paths, state, &op, &mut |_| {})
}
fn release_preview(
    state: CatalogProjection,
    selector: Option<&OsStr>,
    id: crate::management::OperationId,
) -> Result<(CatalogProjection, Value), PoolError> {
    let target = state
        .releases
        .iter()
        .find(|r| r.operation_id == id)
        .ok_or(PoolError::Unregistered)?;
    if selector.is_some()
        && workflows::repository(&state, selector)?.repository_id != target.repository_id
    {
        return Err(PoolError::Selectors);
    }
    let a = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle == target.assignment_handle)
        .ok_or(PoolError::Corrupt)?;
    let observed = crate::recovery_inspection::worktree_data(
        &state,
        crate::recovery_inspection::recorded_worktree(&state, a.worktree_id)?,
    )?;
    let data = json!({"observation":observed["observation"],"repository_id":a.repository_id,"worktree_id":a.worktree_id,"assignment_handle":a.assignment_handle,"operation_id":id,"operation":target,"assignment":a,"ownership":a.state,"availability":if a.state==AssignmentState::Released {Value::Null}else{json!("withheld")},"revision":state.revision,"next_action":"explicit apply observes the recorded release and required preservation"});
    Ok((state, data))
}
fn reconcile_release(
    paths: &Paths,
    selector: Option<&OsStr>,
    id: crate::management::OperationId,
) -> Result<(CatalogProjection, Value), PoolError> {
    use crate::{
        coordination::LockGuard,
        git,
        management::OperationId,
        recovery::{RecoveryDisposition, RecoveryEvent, RecoveryOperation, RecoveryState},
    };
    let state = catalog::inspect_projection(paths)?;
    let target = state
        .releases
        .iter()
        .find(|r| r.operation_id == id)
        .ok_or(PoolError::Unregistered)?;
    let repository_id = target.repository_id;
    if selector.is_some() && workflows::repository(&state, selector)?.repository_id != repository_id
    {
        return Err(PoolError::Selectors);
    }
    let _repository = LockGuard::acquire(
        &paths.state.join(format!("repository-{repository_id}.lock")),
        true,
    )?;
    let state = catalog::inspect_projection(paths)?;
    let target = state
        .releases
        .iter()
        .find(|r| r.operation_id == id)
        .ok_or(PoolError::Unregistered)?;
    if target.state == crate::release::ReleaseState::Completed {
        return release_preview(state, selector, id);
    }
    if target.state == crate::release::ReleaseState::Reconciled {
        let recorded = state
            .recoveries
            .iter()
            .find(|o| o.target_operation_id == Some(id) && o.state == RecoveryState::Completed)
            .ok_or(PoolError::Corrupt)?
            .operation_id;
        return recovery_result(state, recorded);
    }
    if let Some(op) = state
        .recoveries
        .iter()
        .find(|o| o.target_operation_id == Some(id) && o.state != RecoveryState::Completed)
        .cloned()
    {
        return continue_recovery(paths, state, &op, &mut |_| {});
    }
    let w = state
        .worktrees
        .iter()
        .find(|w| w.worktree_id == target.worktree_id)
        .ok_or(PoolError::Corrupt)?;
    let repository = state
        .repositories
        .iter()
        .find(|r| r.repository_id == repository_id)
        .ok_or(PoolError::Corrupt)?;
    let path = w.path.to_path()?;
    let common = repository.common_directory.to_path()?;
    let safety = validate_binding(&path, &common, &w.git_directory.to_path()?)
        .and_then(|()| git::observe_safe(&path, &target.tip));
    let safe = safety
        .as_ref()
        .is_ok_and(|s| (&s.head, &s.branch) == (&target.tip, &target.branch));
    let operation_id = OperationId::new();
    let op = RecoveryOperation {
        operation_id,
        repository_id,
        worktree_id: target.worktree_id,
        assignment_handle: Some(target.assignment_handle),
        state: RecoveryState::Intended,
        tip: safety.as_ref().ok().map(|s| s.head.clone()),
        branch: safety.ok().and_then(|s| s.branch),
        preservation_reference: if safe {
            target.preservation_reference.clone()
        } else {
            None
        },
        intent_event_id: String::new(),
        last_checkpoint: "recovery_intended".into(),
        target_operation_id: Some(id),
        disposition: if safe {
            RecoveryDisposition::Released
        } else {
            RecoveryDisposition::Withheld
        },
        protected_reason: (!safe).then(|| "safety_uncertain".into()),
    };
    let state = append(
        paths,
        RecoveryEvent::Started(op.clone()),
        Some(&target.intent_event_id),
    )?;
    continue_recovery(paths, state, &op, &mut |_| {})
}
fn continue_recovery(
    paths: &Paths,
    mut state: CatalogProjection,
    op: &crate::recovery::RecoveryOperation,
    observe: &mut impl FnMut(RecoveryCheckpoint),
) -> Result<(CatalogProjection, Value), PoolError> {
    use crate::{
        git,
        recovery::{RecoveryEvent, RecoveryState},
    };
    let operation_id = op.operation_id;
    let repository_id = op.repository_id;
    if op.state == RecoveryState::Completed {
        return recovery_result(state, operation_id);
    }
    if op.disposition == crate::recovery::RecoveryDisposition::Withheld {
        state = append(
            paths,
            RecoveryEvent::Finished {
                operation_id,
                repository_id,
                worktree_id: op.worktree_id,
                successful: true,
            },
            Some(&recovery(&state, operation_id)?.intent_event_id),
        )?;
        observe(RecoveryCheckpoint::ResultCommitted);
        return recovery_result(state, operation_id);
    }
    let preservation_id = op
        .preservation_reference
        .as_deref()
        .map(|reference| {
            reference
                .strip_prefix("refs/worktree-pool/")
                .ok_or(PoolError::Corrupt)
        })
        .transpose()?;
    let tip = op.tip.as_deref().ok_or(PoolError::Corrupt)?;
    let repository = state
        .repositories
        .iter()
        .find(|r| r.repository_id == repository_id)
        .ok_or(PoolError::Corrupt)?;
    let worktree = state
        .worktrees
        .iter()
        .find(|w| w.worktree_id == op.worktree_id)
        .ok_or(PoolError::Corrupt)?;
    let path = worktree.path.to_path()?;
    let common = repository.common_directory.to_path()?;
    let admin = worktree.git_directory.to_path()?;
    let required = required_references(&state, op);
    let unchanged = || {
        required
            .iter()
            .all(|(id, tip)| git::verify_preserved_tip(&common, id, tip).is_ok())
            && validate_binding(&path, &common, &admin).is_ok()
            && git::observe_safe(&path, tip).is_ok_and(|s| s.head == tip && s.branch == op.branch)
    };
    if !unchanged() {
        return recovery_result(state, operation_id);
    }
    if op.state == RecoveryState::Intended && op.branch.is_none() {
        // Reopen/observe an ambiguous ref effect before attempting an expected-empty write.
        if git::verify_preserved_tip(&common, preservation_id.ok_or(PoolError::Corrupt)?, tip)
            .is_err()
            && git::preserve_tip(&common, preservation_id.ok_or(PoolError::Corrupt)?, tip).is_err()
        {
            return recovery_result(state, operation_id);
        }
        observe(RecoveryCheckpoint::PreservationObserved);
        state = append(
            paths,
            RecoveryEvent::Preserved {
                operation_id,
                repository_id,
                worktree_id: op.worktree_id,
            },
            Some(&recovery(&state, operation_id)?.intent_event_id),
        )?;
        observe(RecoveryCheckpoint::PreservationCommitted);
    }
    let successful = unchanged()
        && (op.branch.is_some()
            || git::verify_preserved_tip(&common, preservation_id.ok_or(PoolError::Corrupt)?, tip)
                .is_ok());
    state = append(
        paths,
        RecoveryEvent::Finished {
            operation_id,
            repository_id,
            worktree_id: op.worktree_id,
            successful,
        },
        Some(&recovery(&state, operation_id)?.intent_event_id),
    )?;
    observe(RecoveryCheckpoint::ResultCommitted);
    recovery_result(state, operation_id)
}

fn validate_binding(
    path: &std::path::Path,
    common: &std::path::Path,
    admin: &std::path::Path,
) -> Result<(), PoolError> {
    let checkout = crate::git::checkout(path)?;
    if checkout.path != path
        || checkout.common_directory != common
        || checkout.git_directory != admin
    {
        return Err(PoolError::Git);
    }
    Ok(())
}
fn recovery(
    state: &CatalogProjection,
    id: crate::management::OperationId,
) -> Result<&crate::recovery::RecoveryOperation, PoolError> {
    state
        .recoveries
        .iter()
        .find(|o| o.operation_id == id)
        .ok_or(PoolError::Corrupt)
}
fn recovery_result(
    state: CatalogProjection,
    id: crate::management::OperationId,
) -> Result<(CatalogProjection, Value), PoolError> {
    let op = recovery(&state, id)?;
    let assignment = state
        .assignments
        .iter()
        .find(|a| Some(a.assignment_handle) == op.assignment_handle);
    let ownership = assignment.map_or_else(|| json!("unassigned"), |a| json!(a.state));
    let availability = if state.retirements.iter().any(|r| {
        r.worktree_id == op.worktree_id && r.state == crate::retirement::RetirementState::Completed
    }) {
        json!("retired")
    } else if assignment.is_some_and(|a| a.state == AssignmentState::Released) {
        Value::Null
    } else if assignment.is_none()
        && op.state == crate::recovery::RecoveryState::Completed
        && !state
            .withheld_worktrees
            .iter()
            .any(|w| w.worktree_id == op.worktree_id)
    {
        json!("unverified")
    } else {
        json!("withheld")
    };
    let observed = crate::recovery_inspection::worktree_data(
        &state,
        crate::recovery_inspection::recorded_worktree(&state, op.worktree_id)?,
    )?;
    let data = json!({"observation":observed["observation"],"observed_assignment_handle":observed["assignment_handle"],"repository_id":op.repository_id,"worktree_id":op.worktree_id,"assignment_handle":op.assignment_handle,"operation_id":op.operation_id,"operation":op,"ownership":ownership,"availability":availability,"preservation_reference":op.preservation_reference,"preservation_tip":op.tip,"revision":state.revision,"next_action":"inspect the registered worktree before another acquisition"});
    Ok((state, data))
}
fn append(
    paths: &Paths,
    event: crate::recovery::RecoveryEvent,
    cause: Option<&str>,
) -> Result<CatalogProjection, PoolError> {
    use crate::{
        domain::{management_event, management_event_caused_by},
        management::ManagementEvent,
    };
    let (wire, id, expected, result) = {
        let session = catalog::open(paths)?;
        let event = ManagementEvent::Recovery(event);
        let wire = if let Some(cause) = cause {
            management_event_caused_by(session.projection.catalog_id, &event, cause)?
        } else {
            management_event(session.projection.catalog_id, &event)?
        };
        let id = wire["id"].as_str().ok_or(PoolError::Corrupt)?.to_owned();
        let result = session
            .store()
            .append_batch(session.projection.revision, vec![wire.clone()]);
        (wire, id, session.projection.revision, result)
    };
    match result {
        Err(PoolError::CommitUnknown) => {
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

/// # Errors
/// Reconciles only the selected current owner, retaining active ownership unless explicitly abandoned.
pub fn reconcile_assignment(
    paths: &Paths,
    selector: Option<&OsStr>,
    handle: &str,
) -> Result<(CatalogProjection, Value), PoolError> {
    use crate::{
        coordination::LockGuard,
        git,
        management::OperationId,
        recovery::{RecoveryDisposition, RecoveryEvent, RecoveryOperation, RecoveryState},
    };
    let _maintenance = workflows::maintenance(paths)?;
    let (state, _) = preview_assignment(paths, selector, handle)?;
    let a = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle.to_string() == handle)
        .ok_or(PoolError::UnknownAssignment)?;
    if a.state == AssignmentState::Released {
        return preview_assignment(paths, selector, handle);
    }
    if a.state == AssignmentState::Preparing {
        let target = state
            .acquisitions
            .iter()
            .find(|o| o.assignment_handle == a.assignment_handle)
            .ok_or(PoolError::Corrupt)?
            .operation_id;
        return reconcile_acquisition(paths, selector, target, true);
    }
    let repository_id = a.repository_id;
    let _repository = LockGuard::acquire(
        &paths.state.join(format!("repository-{repository_id}.lock")),
        true,
    )?;
    let state = catalog::inspect_projection(paths)?;
    let a = state
        .assignments
        .iter()
        .find(|a| a.assignment_handle.to_string() == handle && a.state == AssignmentState::Active)
        .ok_or(PoolError::Conflict)?;
    if let Some(op) = state
        .recoveries
        .iter()
        .find(|o| {
            o.assignment_handle == Some(a.assignment_handle) && o.state != RecoveryState::Completed
        })
        .cloned()
    {
        return continue_recovery(paths, state, &op, &mut |_| {});
    }
    let w = state
        .worktrees
        .iter()
        .find(|w| w.worktree_id == a.worktree_id)
        .ok_or(PoolError::Corrupt)?;
    let repo = state
        .repositories
        .iter()
        .find(|r| r.repository_id == repository_id)
        .ok_or(PoolError::Corrupt)?;
    let path = w.path.to_path()?;
    let safety = validate_binding(
        &path,
        &repo.common_directory.to_path()?,
        &w.git_directory.to_path()?,
    )
    .and_then(|()| git::resolve_commit(&path, OsStr::new("HEAD")))
    .and_then(|tip| git::observe_safe(&path, &tip));
    let operation_id = OperationId::new();
    let safe = safety.is_ok();
    let op = RecoveryOperation {
        operation_id,
        repository_id,
        worktree_id: w.worktree_id,
        assignment_handle: Some(a.assignment_handle),
        state: RecoveryState::Intended,
        tip: safety.as_ref().ok().map(|s| s.head.clone()),
        branch: safety.as_ref().ok().and_then(|s| s.branch.clone()),
        preservation_reference: safety
            .as_ref()
            .ok()
            .filter(|s| s.branch.is_none())
            .map(|_| format!("refs/worktree-pool/{operation_id}")),
        intent_event_id: String::new(),
        last_checkpoint: "recovery_intended".into(),
        target_operation_id: None,
        disposition: if safe {
            RecoveryDisposition::Active
        } else {
            RecoveryDisposition::Withheld
        },
        protected_reason: (!safe).then(|| "safety_uncertain".into()),
    };
    let state = append(paths, RecoveryEvent::Started(op.clone()), None)?;
    continue_recovery(paths, state, &op, &mut |_| {})
}
/// # Errors
/// Selects an exact registered worktree and its current owner; never chooses a historical or newest handle.
pub fn recover_worktree(
    paths: &Paths,
    repo: Option<&OsStr>,
    selector: &OsStr,
    apply: bool,
    abandon: bool,
) -> Result<(CatalogProjection, Value), PoolError> {
    let _maintenance = workflows::maintenance(paths)?;
    if !apply {
        let state = catalog::inspect_projection(paths)?;
        let repository_id =
            crate::recovery_inspection::selected_worktree(&state, repo, selector)?.repository_id;
        let _repository = crate::coordination::LockGuard::acquire(
            &paths.state.join(format!("repository-{repository_id}.lock")),
            true,
        )?;
        let state = catalog::inspect_projection(paths)?;
        let w = crate::recovery_inspection::selected_worktree(&state, repo, selector)?;
        if let Some(a) = state
            .assignments
            .iter()
            .find(|a| a.worktree_id == w.worktree_id && a.state != AssignmentState::Released)
        {
            let handle = a.assignment_handle.to_string();
            return assignment_preview(state, repo, &handle);
        }
        let data = crate::recovery_inspection::worktree_data(&state, w)?;
        return Ok((state, data));
    }
    let state = catalog::inspect_projection(paths)?;
    let w = crate::recovery_inspection::selected_worktree(&state, repo, selector)?;
    let a = state
        .assignments
        .iter()
        .find(|a| a.worktree_id == w.worktree_id && a.state != AssignmentState::Released);
    let Some(a) = a else {
        if abandon {
            return Err(PoolError::Selectors);
        }
        if !apply {
            let data = crate::recovery_inspection::worktree_data(&state, w)?;
            return Ok((state, data));
        }
        return reconcile_unassigned(paths, repo, selector);
    };
    let handle = a.assignment_handle.to_string();
    if !apply {
        return preview_assignment(paths, repo, &handle);
    }
    if abandon {
        abandon_assignment(paths, repo, &handle)
    } else {
        reconcile_assignment(paths, repo, &handle)
    }
}

fn reconcile_unassigned(
    paths: &Paths,
    repo: Option<&OsStr>,
    selector: &OsStr,
) -> Result<(CatalogProjection, Value), PoolError> {
    use crate::{
        coordination::LockGuard,
        git,
        management::OperationId,
        recovery::{RecoveryDisposition, RecoveryEvent, RecoveryOperation, RecoveryState},
    };
    let _maintenance = workflows::maintenance(paths)?;
    let state = catalog::inspect_projection(paths)?;
    let repository_id =
        crate::recovery_inspection::selected_worktree(&state, repo, selector)?.repository_id;
    let _repository = LockGuard::acquire(
        &paths.state.join(format!("repository-{repository_id}.lock")),
        true,
    )?;
    let state = catalog::inspect_projection(paths)?;
    let w = crate::recovery_inspection::selected_worktree(&state, repo, selector)?;
    if state
        .assignments
        .iter()
        .any(|a| a.worktree_id == w.worktree_id && a.state != AssignmentState::Released)
    {
        return Err(PoolError::Conflict);
    }
    if let Some(op) = state
        .recoveries
        .iter()
        .find(|o| o.worktree_id == w.worktree_id && o.state != RecoveryState::Completed)
        .cloned()
    {
        return continue_recovery(paths, state, &op, &mut |_| {});
    }
    let repository = state
        .repositories
        .iter()
        .find(|r| r.repository_id == repository_id)
        .ok_or(PoolError::Corrupt)?;
    let path = w.path.to_path()?;
    let safety = validate_binding(
        &path,
        &repository.common_directory.to_path()?,
        &w.git_directory.to_path()?,
    )
    .and_then(|()| git::resolve_commit(&path, OsStr::new("HEAD")))
    .and_then(|tip| git::observe_safe(&path, &tip));
    let safe = safety.is_ok();
    let operation_id = OperationId::new();
    let op = RecoveryOperation {
        operation_id,
        repository_id,
        worktree_id: w.worktree_id,
        assignment_handle: None,
        state: RecoveryState::Intended,
        tip: safety.as_ref().ok().map(|s| s.head.clone()),
        branch: safety.as_ref().ok().and_then(|s| s.branch.clone()),
        preservation_reference: safety
            .as_ref()
            .ok()
            .filter(|s| s.branch.is_none())
            .map(|_| format!("refs/worktree-pool/{operation_id}")),
        intent_event_id: String::new(),
        last_checkpoint: "recovery_intended".into(),
        target_operation_id: None,
        disposition: if safe {
            RecoveryDisposition::Unassigned
        } else {
            RecoveryDisposition::Withheld
        },
        protected_reason: (!safe).then(|| "safety_uncertain".into()),
    };
    let state = append(paths, RecoveryEvent::Started(op.clone()), None)?;
    continue_recovery(paths, state, &op, &mut |_| {})
}

fn abandonment_intent(
    state: &CatalogProjection,
    assignment: &crate::acquisition::Assignment,
    safety: crate::git::Safety,
) -> Result<(crate::recovery::RecoveryOperation, Option<String>), PoolError> {
    use crate::{
        management::OperationId,
        recovery::{RecoveryOperation, RecoveryState},
    };
    let repository_id = assignment.repository_id;
    let target_operation_id = if assignment.state == AssignmentState::Preparing {
        state
            .acquisitions
            .iter()
            .find(|o| o.operation_id == assignment.operation_id && o.state.is_pending())
            .map(|o| o.operation_id)
    } else {
        state
            .releases
            .iter()
            .find(|r| r.assignment_handle == assignment.assignment_handle && r.state.is_pending())
            .map(|r| r.operation_id)
    };
    let cause = if let Some(id) = target_operation_id {
        if let Some(release) = state.releases.iter().find(|r| r.operation_id == id) {
            if (&release.tip, &release.branch) != (&safety.head, &safety.branch) {
                return Err(PoolError::OperationPending);
            }
            Some(release.intent_event_id.clone())
        } else {
            Some(crate::recovery::preparation_target(state, id)?.1.to_owned())
        }
    } else {
        None
    };
    let operation_id = OperationId::new();
    let preservation_reference = target_operation_id
        .and_then(|id| state.releases.iter().find(|r| r.operation_id == id))
        .map_or_else(
            || {
                safety
                    .branch
                    .is_none()
                    .then(|| format!("refs/worktree-pool/{operation_id}"))
            },
            |r| r.preservation_reference.clone(),
        );
    let op = RecoveryOperation {
        operation_id,
        repository_id,
        worktree_id: assignment.worktree_id,
        assignment_handle: Some(assignment.assignment_handle),
        state: RecoveryState::Intended,
        tip: Some(safety.head),
        branch: safety.branch,
        preservation_reference,
        intent_event_id: String::new(),
        last_checkpoint: "recovery_intended".into(),
        target_operation_id,
        disposition: crate::recovery::RecoveryDisposition::Released,
        protected_reason: None,
    };
    Ok((op, cause))
}

fn observe_preparation(
    state: &CatalogProjection,
    a: &crate::acquisition::Assignment,
    target: &crate::acquisition::AcquisitionOperation,
) -> Result<
    (
        crate::recovery::RecoveryDisposition,
        Option<crate::git::Safety>,
        Option<&'static str>,
    ),
    PoolError,
> {
    use crate::{git, recovery::RecoveryDisposition};
    let repository_id = a.repository_id;
    let w = state
        .worktrees
        .iter()
        .find(|w| w.worktree_id == a.worktree_id)
        .ok_or(PoolError::Corrupt)?;
    let repository = state
        .repositories
        .iter()
        .find(|r| r.repository_id == repository_id)
        .ok_or(PoolError::Corrupt)?;
    let path = w.path.to_path()?;
    let common = repository.common_directory.to_path()?;
    let observed = validate_binding(&path, &common, &w.git_directory.to_path()?)
        .and_then(|()| git::observe_safe(&path, &a.resolved_commit));
    let (disposition, safety, reason) = match observed {
        Ok(safety)
            if safety.head == a.resolved_commit
                && safety.branch.is_none()
                && target.preservation_tip.as_ref().is_none_or(|tip| {
                    git::verify_preserved_tip(&common, &target.operation_id.to_string(), tip)
                        .is_ok()
                }) =>
        {
            (RecoveryDisposition::Active, Some(safety), None)
        }
        Ok(safety) => (
            RecoveryDisposition::Withheld,
            Some(safety),
            Some("safety_uncertain"),
        ),
        Err(error) => {
            let reason = if state
                .creations
                .iter()
                .any(|c| c.acquisition_operation_id == target.operation_id)
            {
                "partial_creation_unproven"
            } else {
                match error {
                    PoolError::UnfinishedWork => "unfinished_work",
                    PoolError::GitOperation => "git_operation_in_progress",
                    PoolError::UnsupportedIndex => "unsupported_index_state",
                    PoolError::RetainedCollision => "retained_file_collision",
                    _ => "safety_uncertain",
                }
            };
            (RecoveryDisposition::Withheld, None, Some(reason))
        }
    };
    Ok((disposition, safety, reason))
}

fn required_references(
    state: &CatalogProjection,
    op: &crate::recovery::RecoveryOperation,
) -> Vec<(String, String)> {
    state
        .acquisitions
        .iter()
        .filter(|a| Some(a.assignment_handle) == op.assignment_handle)
        .filter_map(|a| {
            a.preservation_tip
                .as_ref()
                .map(|tip| (a.operation_id.to_string(), tip.clone()))
        })
        .chain(
            state
                .releases
                .iter()
                .filter(|r| {
                    Some(r.assignment_handle) == op.assignment_handle
                        && r.preservation_reference.is_some()
                        && Some(r.operation_id) != op.target_operation_id
                })
                .map(|r| (r.operation_id.to_string(), r.tip.clone())),
        )
        .collect()
}
