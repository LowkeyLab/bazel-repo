//! CLI composition for the stable locator's explicit catalog recovery journal.
use std::ffi::OsStr;

use serde_json::{Value, json};

use crate::{
    catalog::{self, AuthorityObservation, AuthorityRecoveryFact},
    cli::Envelope,
    error::PoolError,
    management::OperationId,
    paths::{EncodedPath, Paths},
};

/// Handles an explicitly selected catalog scope without guessing a repository operation.
#[must_use]
pub fn recover_catalog(
    paths: &Paths,
    repository: Option<&OsStr>,
    apply: bool,
    operation: Option<&str>,
) -> Envelope {
    let command = recovery_command(apply);
    if repository.is_some() {
        return failure(paths, command, &PoolError::Selectors, None, None);
    }
    if let Some(operation) = operation {
        return recover_known_operation(paths, None, apply, operation)
            .unwrap_or_else(|| failure(paths, command, &PoolError::Unregistered, None, None));
    }
    let before = match catalog::inspect_authority(paths) {
        Ok(authority) => authority,
        Err(error) => return failure(paths, command, &error, None, None),
    };
    if !apply {
        let operation = current_operation(&before);
        return observed(command, &before, operation.as_ref(), false, false);
    }
    let already_completed = mutations_available(&before);
    match catalog::reconcile_authority(paths) {
        Ok(authority) => {
            let operation = current_operation(&authority);
            observed(
                command,
                &authority,
                operation.as_ref(),
                true,
                already_completed,
            )
        }
        Err(error) => failure(paths, command, &error, Some(before), None),
    }
}

/// Returns None only when the selector cannot identify a known catalog operation.
/// The caller can then perform an exact repository-operation lookup.
#[must_use]
pub fn recover_known_operation(
    paths: &Paths,
    repository: Option<&OsStr>,
    apply: bool,
    operation: &str,
) -> Option<Envelope> {
    let command = recovery_command(apply);
    let KnownCatalogOperation {
        authority: before,
        operation,
    } = match known_operation(paths, operation) {
        Ok(Some(known)) => known,
        Ok(None) => return None,
        Err(error) => return Some(failure(paths, command, &error, None, None)),
    };
    let operation_id = operation.operation_id;
    if repository.is_some() {
        return Some(failure(
            paths,
            command,
            &PoolError::Selectors,
            Some(before),
            Some(operation_id),
        ));
    }
    if !apply {
        return Some(observed(command, &before, Some(&operation), false, false));
    }
    let already_completed = operation.checkpoint == "completed";
    Some(
        match catalog::reconcile_authority_operation(paths, operation_id) {
            Ok(operation) => {
                let authority = catalog::inspect_authority(paths).unwrap_or(before);
                observed(
                    command,
                    &authority,
                    Some(&operation),
                    true,
                    already_completed,
                )
            }
            Err(error) => failure(paths, command, &error, Some(before), Some(operation_id)),
        },
    )
}

/// Inspects one exact catalog result, including while repository storage is unavailable.
#[must_use]
pub fn inspect_known_operation(
    paths: &Paths,
    repository: Option<&OsStr>,
    operation: &str,
) -> Option<Envelope> {
    let command = "operation inspect";
    let KnownCatalogOperation {
        authority,
        operation,
    } = match known_operation(paths, operation) {
        Ok(Some(known)) => known,
        Ok(None) => return None,
        Err(error) => return Some(failure(paths, command, &error, None, None)),
    };
    if repository.is_some() {
        return Some(failure(
            paths,
            command,
            &PoolError::Selectors,
            Some(authority),
            Some(operation.operation_id),
        ));
    }
    Some(observed(
        command,
        &authority,
        Some(&operation),
        false,
        false,
    ))
}

/// Lists the latest checkpoint of each catalog operation from the stable locator.
/// # Errors
/// Rejects missing, corrupt, conflicting or unsupported locator authority.
pub fn lifecycle_operations(paths: &Paths) -> Result<Vec<Value>, PoolError> {
    catalog::list_authority_operations(paths)
        .map(|operations| operations.iter().map(operation_value).collect())
}

struct KnownCatalogOperation {
    authority: AuthorityObservation,
    operation: AuthorityRecoveryFact,
}

fn known_operation(
    paths: &Paths,
    selector: &str,
) -> Result<Option<KnownCatalogOperation>, PoolError> {
    let Ok(operation_id) = selector.parse::<OperationId>() else {
        return Ok(None);
    };
    let authority = match catalog::inspect_authority(paths) {
        Ok(authority) => authority,
        Err(PoolError::Missing | PoolError::Unregistered) => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(
        exact_operation(&authority, operation_id).map(|operation| KnownCatalogOperation {
            authority,
            operation,
        }),
    )
}

fn exact_operation(
    authority: &AuthorityObservation,
    operation_id: OperationId,
) -> Option<AuthorityRecoveryFact> {
    authority
        .recovery_history
        .iter()
        .rev()
        .find(|fact| fact.operation_id == operation_id)
        .cloned()
}

fn current_operation(authority: &AuthorityObservation) -> Option<AuthorityRecoveryFact> {
    authority
        .recovery_operation_id
        .and_then(|id| exact_operation(authority, id))
}

const fn recovery_command(apply: bool) -> &'static str {
    if apply {
        "recover apply"
    } else {
        "recover preview"
    }
}

fn mutations_available(authority: &AuthorityObservation) -> bool {
    authority.phase == "active"
        && authority.store_state == "validated"
        && authority.recovery_checkpoint.as_deref() != Some("intent_recorded")
}

fn next_action(authority: &AuthorityObservation) -> &'static str {
    if authority.recovery_checkpoint.as_deref() == Some("intent_recorded") {
        "inspect the recorded catalog operation and explicitly apply its recovery"
    } else if authority.phase == "initializing" {
        "run recover apply --catalog to reconcile the recorded initialization"
    } else if authority.store_state == "rebuild_required" {
        "run catalog rebuild to reconstruct derived state from validated immutable history"
    } else if authority.store_state != "validated" {
        "inspect the selected catalog path and recorded catalog identity before explicit recovery"
    } else {
        "inspect repository operations and registered worktrees before acquisition"
    }
}

fn operation_value(operation: &AuthorityRecoveryFact) -> Value {
    let mut value = json!(operation);
    value["scope"] = json!("catalog");
    value["state"] = json!(if operation.checkpoint == "completed" {
        "completed"
    } else {
        "pending"
    });
    value["last_checkpoint"] = json!(operation.checkpoint);
    value["journal"] = json!("active_catalog_locator");
    value
}

fn context(
    paths: &Paths,
    authority: Option<&AuthorityObservation>,
    operation_id: Option<OperationId>,
) -> Value {
    json!({
        "catalog_id": authority.map(|authority| authority.catalog_id),
        "catalog_path": EncodedPath::from_path(&paths.catalog),
        "repository_id": null,
        "worktree_id": null,
        "assignment_handle": null,
        "operation_id": operation_id,
    })
}

fn observed(
    command: &'static str,
    authority: &AuthorityObservation,
    operation: Option<&AuthorityRecoveryFact>,
    apply: bool,
    already_completed: bool,
) -> Envelope {
    let pending = apply && operation.is_some_and(|operation| operation.checkpoint != "completed");
    let reason_code = if pending {
        "operation_pending"
    } else if !apply {
        "catalog_recovery_observed"
    } else if already_completed {
        "catalog_recovery_already_completed"
    } else {
        "catalog_recovery_completed"
    };
    Envelope {
        schema_version: 1,
        command: command.into(),
        outcome: if pending { "pending" } else { "completed" },
        reason_code,
        context: json!({
            "catalog_id": authority.catalog_id,
            "catalog_path": authority.catalog_path,
            "repository_id": null,
            "worktree_id": null,
            "assignment_handle": null,
            "operation_id": operation.map(|operation| operation.operation_id),
        }),
        data: json!({
            "scope": "catalog",
            "authority": authority,
            "operation": operation.map(operation_value),
            "ownership": null,
            "availability": null,
            "catalog_mutations_available": mutations_available(authority),
            "already_completed": already_completed,
            "next_action": next_action(authority),
        }),
        warnings: Vec::new(),
    }
}

fn failure(
    paths: &Paths,
    command: &'static str,
    error: &PoolError,
    before: Option<AuthorityObservation>,
    selected_operation: Option<OperationId>,
) -> Envelope {
    let authority = catalog::inspect_authority(paths).ok().or(before);
    let operation = selected_operation.and_then(|id| {
        authority
            .as_ref()
            .and_then(|authority| exact_operation(authority, id))
    });
    let mut envelope = Envelope::failure(command.into(), error);
    envelope.context = context(paths, authority.as_ref(), selected_operation);
    envelope.data = json!({
        "scope": "catalog",
        "authority": authority,
        "operation": operation.as_ref().map(operation_value),
        "ownership": null,
        "availability": null,
        "catalog_mutations_available": authority.as_ref().is_some_and(mutations_available),
        "next_action": error.to_string(),
    });
    envelope
}
