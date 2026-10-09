//! Exact identity selection and read-only observations of registered recovery candidates.
use crate::{
    acquisition::AssignmentState,
    catalog,
    domain::CatalogProjection,
    error::PoolError,
    management::{Worktree, WorktreeId},
    paths::{EncodedPath, Paths},
    workflows,
};
use serde_json::{Value, json};
use std::{ffi::OsStr, path::PathBuf};

/// # Errors
/// Rejects unregistered paths and conflicting repository selectors without selecting another row.
pub fn selected_worktree<'a>(
    state: &'a CatalogProjection,
    repo: Option<&OsStr>,
    selector: &OsStr,
) -> Result<&'a Worktree, PoolError> {
    let id = selector.to_str().and_then(|s| s.parse::<WorktreeId>().ok());
    let path = PathBuf::from(selector);
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    let tree = state
        .worktrees
        .iter()
        .find(|w| {
            Some(w.worktree_id) == id
                || (id.is_none() && w.path.bytes == EncodedPath::from_path(&path).bytes)
        })
        .ok_or(PoolError::Unregistered)?;
    if repo.is_some() && workflows::repository(state, repo)?.repository_id != tree.repository_id {
        return Err(PoolError::Selectors);
    }
    Ok(tree)
}
/// # Errors
/// Rejects invalid authority and selector conflicts; never alters protected state.
pub fn preview_candidates(
    paths: &Paths,
    repo: Option<&OsStr>,
) -> Result<(CatalogProjection, Value), PoolError> {
    let _maintenance = workflows::maintenance(paths)?;
    let state = catalog::inspect_projection(paths)?;
    let selected = workflows::repository(&state, repo)?;
    let _repository = crate::coordination::LockGuard::acquire(
        &paths
            .state
            .join(format!("repository-{}.lock", selected.repository_id)),
        true,
    )?;
    let state = catalog::inspect_projection(paths)?;
    let candidates: Vec<_> = state
        .worktrees
        .iter()
        .filter(|w| w.repository_id == selected.repository_id)
        .map(|w| worktree_data(&state, w))
        .collect::<Result<_, _>>()?;
    let data = json!({"repository_id":selected.repository_id,"candidates":candidates,"revision":state.revision,"next_action":"select an exact operation, assignment handle, or registered worktree before explicit apply; no candidate was selected"});
    Ok((state, data))
}
/// Reconstruct an exact historical result without selecting a newer registration at its path.
pub(crate) fn recorded_worktree(
    state: &CatalogProjection,
    id: WorktreeId,
) -> Result<&Worktree, PoolError> {
    state
        .worktrees
        .iter()
        .find(|w| w.worktree_id == id)
        .or_else(|| {
            state
                .retirements
                .iter()
                .find(|r| {
                    r.worktree_id == id && r.state == crate::retirement::RetirementState::Completed
                })
                .map(|r| &r.worktree)
        })
        .ok_or(PoolError::Corrupt)
}
pub(crate) fn worktree_data(state: &CatalogProjection, w: &Worktree) -> Result<Value, PoolError> {
    if state.retirements.iter().any(|r| {
        r.worktree_id == w.worktree_id && r.state == crate::retirement::RetirementState::Completed
    }) {
        return Ok(
            json!({"repository_id":w.repository_id,"worktree_id":w.worktree_id,"path":w.path,"assignment_handle":null,"assignment":null,"ownership":"unassigned","availability":"retired","observation":{"status":"historical","safe":false,"reason_code":"registration_retired"},"next_action":"this retired registration grants no current ownership"}),
        );
    }

    let owner = state
        .assignments
        .iter()
        .find(|a| a.worktree_id == w.worktree_id && a.state != AssignmentState::Released);
    let repo = state
        .repositories
        .iter()
        .find(|r| r.repository_id == w.repository_id)
        .ok_or(PoolError::Corrupt)?;
    let observation = (|| {
        let path = w.path.to_path()?;
        let checkout = crate::git::checkout(&path)?;
        if checkout.path != path
            || checkout.common_directory != repo.common_directory.to_path()?
            || checkout.git_directory != w.git_directory.to_path()?
        {
            return Err(PoolError::Git);
        }
        let head = crate::git::resolve_commit(&path, OsStr::new("HEAD"))?;
        crate::git::observe_safe(&path, &head)
    })();
    let required: Vec<_> = state
        .acquisitions
        .iter()
        .filter(|a| owner.is_some_and(|owner| owner.assignment_handle == a.assignment_handle))
        .filter_map(|a| {
            a.preservation_tip
                .as_ref()
                .map(|tip| (a.operation_id, tip.as_str()))
        })
        .chain(
            state
                .releases
                .iter()
                .filter(|r| {
                    owner.is_some_and(|owner| owner.assignment_handle == r.assignment_handle)
                        && r.preservation_reference.is_some()
                })
                .map(|r| (r.operation_id, r.tip.as_str())),
        )
        .collect();
    let common = repo.common_directory.to_path()?;
    let preservation:Vec<_>=required.iter().map(|(id,tip)|json!({"reference":format!("refs/worktree-pool/{id}"),"tip":tip,"verified":crate::git::verify_preserved_tip(&common,&id.to_string(),tip).is_ok()})).collect();
    let preserved = preservation
        .iter()
        .all(|root| root["verified"].as_bool() == Some(true));
    let mut observed = match observation {
        Ok(s) => {
            json!({"safe":preserved,"tip":s.head,"branch":s.branch,"reason_code":if preserved {Value::Null}else{json!("preservation_unverified")}})
        }
        Err(e) => json!({"safe":false,"reason_code":e.reason_code()}),
    };
    observed["preservation"] = json!(preservation);
    Ok(
        json!({"repository_id":w.repository_id,"worktree_id":w.worktree_id,"path":w.path,"assignment_handle":owner.map(|a|a.assignment_handle),"assignment":owner,"ownership":owner.map_or_else(||json!("unassigned"),|a|json!(a.state)),"availability":"withheld","observation":observed,"next_action":"explicit reconciliation preserves ownership unless abandonment is requested and preservation succeeds"}),
    )
}
