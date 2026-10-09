//! Retirement observes explicit human removal and preserves every recorded reachability root.
use std::{ffi::OsStr, fs, path::Path};

use serde_json::{Value, json};

use crate::{
    catalog,
    coordination::LockGuard,
    domain::{CatalogProjection, decode_event, management_event_caused_by},
    error::PoolError,
    git,
    management::{ManagementEvent, OperationId, Worktree, WorktreeId},
    paths::Paths,
    retirement::{RetirementEvent, RetirementOperation, RetirementState},
    workflows,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetirementCheckpoint {
    IntentCommitted,
    ResultCommitted,
}

/// # Errors
/// Refuses current ownership, uncertain removal or preservation, and stale reconciliation.
pub fn retire(
    paths: &Paths,
    repo: Option<&OsStr>,
    selector: &OsStr,
    proof: OperationId,
    removed: bool,
) -> Result<(CatalogProjection, Value), PoolError> {
    retire_observed(paths, repo, selector, proof, removed, |_| {})
}
/// # Errors
/// Retains registered capacity until the durable retirement result, including after interruption.
pub fn retire_observed(
    paths: &Paths,
    repo: Option<&OsStr>,
    selector: &OsStr,
    proof: OperationId,
    removed: bool,
    mut observe: impl FnMut(RetirementCheckpoint),
) -> Result<(CatalogProjection, Value), PoolError> {
    if !removed {
        return Err(PoolError::RetirementUnsafe);
    }
    let _maintenance = workflows::maintenance(paths)?;
    let state = catalog::inspect_projection(paths)?;
    let retired_id = selector.to_str().and_then(|s| s.parse::<WorktreeId>().ok());
    let existing = state
        .retirements
        .iter()
        .find(|o| Some(o.worktree_id) == retired_id)
        .cloned();
    let tree = if let Some(o) = &existing {
        o.worktree.clone()
    } else {
        crate::recovery_inspection::selected_worktree(&state, repo, selector)?.clone()
    };
    if repo.is_some() && workflows::repository(&state, repo)?.repository_id != tree.repository_id {
        return Err(PoolError::Selectors);
    }
    let _repository = LockGuard::acquire(
        &paths
            .state
            .join(format!("repository-{}.lock", tree.repository_id)),
        true,
    )?;
    let state = catalog::inspect_projection(paths)?;
    if let Some(o) = state
        .retirements
        .iter()
        .find(|o| o.worktree_id == tree.worktree_id)
        .cloned()
    {
        if o.reconciliation_id != proof {
            return Err(PoolError::RetirementUnsafe);
        }
        if o.state == RetirementState::Completed {
            return receipt(state, o.operation_id, true);
        }
        validate_removed(paths, &state, &o.worktree, proof)?;
        return finish(paths, &o, &mut observe);
    }
    if state.has_pending_recovery(tree.repository_id) {
        return Err(PoolError::OperationPending);
    }
    let checkpoint = crate::retirement::eligible(&state, &tree, proof)?;
    validate_removed(paths, &state, &tree, proof)?;
    let o = RetirementOperation {
        operation_id: OperationId::new(),
        repository_id: tree.repository_id,
        worktree_id: tree.worktree_id,
        worktree: tree,
        reconciliation_id: proof,
        reconciliation_checkpoint: checkpoint.clone(),
        state: RetirementState::Intended,
        intent_event_id: String::new(),
        last_checkpoint: "retirement_intended".into(),
    };
    let state = append(
        paths,
        RetirementEvent::Started(Box::new(o.clone())),
        &checkpoint,
    )?;
    observe(RetirementCheckpoint::IntentCommitted);
    let o = state
        .retirements
        .iter()
        .find(|r| r.operation_id == o.operation_id)
        .cloned()
        .ok_or(PoolError::Corrupt)?;
    validate_removed(paths, &state, &o.worktree, proof)?;
    finish(paths, &o, &mut observe)
}
fn finish(
    paths: &Paths,
    o: &RetirementOperation,
    observe: &mut impl FnMut(RetirementCheckpoint),
) -> Result<(CatalogProjection, Value), PoolError> {
    let state = append(
        paths,
        RetirementEvent::Finished {
            operation_id: o.operation_id,
            repository_id: o.repository_id,
            worktree_id: o.worktree_id,
        },
        &o.intent_event_id,
    )?;
    observe(RetirementCheckpoint::ResultCommitted);
    receipt(state, o.operation_id, false)
}
fn receipt(
    state: CatalogProjection,
    id: OperationId,
    already: bool,
) -> Result<(CatalogProjection, Value), PoolError> {
    let o = state
        .retirements
        .iter()
        .find(|o| o.operation_id == id)
        .ok_or(PoolError::Corrupt)?;
    let data = json!({"resources":crate::resources::retired_usage(&o.worktree),"operation":o,"repository_id":o.repository_id,"worktree_id":o.worktree_id,"operation_id":o.operation_id,"retired":true,"already_retired":already,"ownership":"unassigned","availability":"retired","next_action":"registered capacity is free; retained commits, shared caches and external build state were not removed"});
    Ok((state, data))
}
fn append(
    paths: &Paths,
    event: RetirementEvent,
    cause: &str,
) -> Result<CatalogProjection, PoolError> {
    let session = catalog::open(paths)?;
    let wire = management_event_caused_by(
        session.projection.catalog_id,
        &ManagementEvent::Retirement(event),
        cause,
    )?;
    session.store().append(
        decode_event(&wire, session.projection.catalog_id)?
            .expected_revision(&session.projection)?,
        wire,
    )
}
fn absent(path: &Path) -> Result<(), PoolError> {
    for ancestor in path.ancestors().skip(1) {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => return Err(PoolError::RetirementUnsafe),
        }
    }
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(PoolError::RetirementUnsafe),
    }
}
fn validate_removed(
    paths: &Paths,
    state: &CatalogProjection,
    w: &Worktree,
    proof: OperationId,
) -> Result<(), PoolError> {
    crate::retirement::eligible(state, w, proof)?;
    absent(&w.path.to_path()?)?;
    absent(&w.git_directory.to_path()?)?;
    let repository = state
        .repositories
        .iter()
        .find(|r| r.repository_id == w.repository_id)
        .ok_or(PoolError::Corrupt)?;
    let common = repository.common_directory.to_path()?;
    if fs::canonicalize(&common)? != common {
        return Err(PoolError::RetirementUnsafe);
    }
    // Inspect the immutable event stream: a proof predating another worktree change is stale.
    let (_, events) = catalog::inspect_events(paths)?;
    let subject = format!("worktrees/{}", w.worktree_id);
    let proof_position = events
        .iter()
        .rposition(|e| {
            e["operationid"] == proof.to_string()
                && (e["type"] == "io.lowkeylab.worktreepool.recovery.finished.v1"
                    || e["type"] == "io.lowkeylab.worktreepool.assignment.release.finished.v1")
        })
        .ok_or(PoolError::RetirementUnsafe)?;
    if events
        .iter()
        .skip(proof_position + 1)
        .any(|e| e["subject"] == subject && e["data"]["kind"] != "retirement")
    {
        return Err(PoolError::RetirementUnsafe);
    }
    let attached = state
        .releases
        .iter()
        .find(|o| o.operation_id == proof)
        .and_then(|o| o.branch.as_ref().map(|b| (b.as_str(), o.tip.as_str())))
        .or_else(|| {
            state
                .recoveries
                .iter()
                .find(|o| o.operation_id == proof)
                .and_then(|o| {
                    o.branch
                        .as_ref()
                        .zip(o.tip.as_ref())
                        .map(|(b, t)| (b.as_str(), t.as_str()))
                })
        });
    if let Some((branch, tip)) = attached {
        git::verify_branch_tip(&common, branch, tip)?;
    }
    for (operation, tip) in required_references(state, w)? {
        git::verify_preserved_tip(&common, &operation.to_string(), &tip)?;
    }
    Ok(())
}
fn required_references(
    state: &CatalogProjection,
    w: &Worktree,
) -> Result<Vec<(OperationId, String)>, PoolError> {
    let mut roots: Vec<_> = state
        .acquisitions
        .iter()
        .filter(|o| o.worktree_id == w.worktree_id)
        .filter_map(|o| {
            o.preservation_tip
                .as_ref()
                .map(|tip| (o.operation_id, tip.clone()))
        })
        .chain(
            state
                .releases
                .iter()
                .filter(|o| o.worktree_id == w.worktree_id && o.preservation_reference.is_some())
                .map(|o| (o.operation_id, o.tip.clone())),
        )
        .collect();
    for recovery in state
        .recoveries
        .iter()
        .filter(|o| o.worktree_id == w.worktree_id)
    {
        if let Some(reference) = &recovery.preservation_reference {
            let operation = reference
                .strip_prefix("refs/worktree-pool/")
                .ok_or(PoolError::Corrupt)?
                .parse()?;
            roots.push((operation, recovery.tip.clone().ok_or(PoolError::Corrupt)?));
        }
    }
    Ok(roots)
}

/// Selects only a recorded retirement ID; preview observes without any mutation.
/// # Errors
/// Rejects unknown identities, selector mismatch, or unsafe resumed preconditions.
pub fn recover(
    paths: &Paths,
    repo: Option<&OsStr>,
    id: OperationId,
    apply: bool,
) -> Result<(CatalogProjection, Value), PoolError> {
    let _maintenance = workflows::maintenance(paths)?;
    let (state, o) = {
        let state = catalog::inspect_projection(paths)?;
        let o = state
            .retirements
            .iter()
            .find(|o| o.operation_id == id)
            .cloned()
            .ok_or(PoolError::Unregistered)?;
        if repo.is_some() && workflows::repository(&state, repo)?.repository_id != o.repository_id {
            return Err(PoolError::Selectors);
        }
        let _repository = LockGuard::acquire(
            &paths
                .state
                .join(format!("repository-{}.lock", o.repository_id)),
            true,
        )?;
        let state = catalog::inspect_projection(paths)?;
        let o = state
            .retirements
            .iter()
            .find(|o| o.operation_id == id)
            .cloned()
            .ok_or(PoolError::Unregistered)?;
        (state, o)
    };
    if apply {
        return retire(
            paths,
            repo,
            OsStr::new(&o.worktree_id.to_string()),
            o.reconciliation_id,
            true,
        );
    }
    let data = json!({"repository_id":o.repository_id,"worktree_id":o.worktree_id,"operation_id":o.operation_id,"operation":o,"retired":o.state == RetirementState::Completed,"ownership":"unassigned","availability":if o.state == RetirementState::Completed {"retired"} else {"withheld"},"next_action":"apply this exact operation to revalidate human removal and preservation; no cleanup is performed"});
    Ok((state, data))
}
