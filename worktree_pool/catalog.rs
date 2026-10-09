use std::os::unix::fs::OpenOptionsExt;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{
    coordination::{LockGuard, private_directory, private_file},
    domain::{CatalogId, CatalogProjection},
    error::PoolError,
    management::OperationId,
    paths::{EncodedPath, Paths},
    store::{ReadOnlyInspectionError, Store},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Locator {
    pub(crate) version: u32,
    pub(crate) catalog_id: CatalogId,
    pub(crate) catalog_path: EncodedPath,
    pub(crate) phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery: Option<AuthorityRecovery>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) recovery_history: Vec<AuthorityRecoveryFact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) relocation: Option<crate::relocation::Relocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) relocation_history: Vec<crate::relocation::RelocationFact>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityRecovery {
    operation_id: OperationId,
    checkpoint: String,
    #[serde(default)]
    kind: AuthorityRecoveryKind,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityRecoveryKind {
    #[default]
    Bootstrap,
    StorageRepair,
}
impl Locator {
    pub(crate) fn recovery_identity_used(&self, operation_id: OperationId) -> bool {
        self.recovery
            .as_ref()
            .is_some_and(|r| r.operation_id == operation_id)
            || self
                .recovery_history
                .iter()
                .any(|r| r.operation_id == operation_id)
    }
    pub(crate) fn recovery_pending(&self) -> bool {
        self.recovery
            .as_ref()
            .is_some_and(|recovery| recovery.checkpoint == "intent_recorded")
    }
}

pub struct CatalogSession {
    pub projection: CatalogProjection,
    store: Store,
    _catalog_lock: LockGuard,
    _maintenance_lock: LockGuard,
}
impl CatalogSession {
    #[must_use]
    pub const fn store(&self) -> &Store {
        &self.store
    }
}
pub(crate) fn read_locator(paths: &Paths) -> Result<Locator, PoolError> {
    let value = read_active_locator(paths)?;
    if value.catalog_path.to_path()? != paths.catalog {
        return Err(PoolError::Conflict);
    }
    Ok(value)
}

pub(crate) fn read_active_locator(paths: &Paths) -> Result<Locator, PoolError> {
    let locator = paths.state.join("active.json");
    match fs::read(&locator) {
        Ok(bytes) => {
            private_file(&locator)?;
            let value: Locator = serde_json::from_slice(&bytes).map_err(|_| PoolError::Corrupt)?;
            if value.version != 1 {
                return Err(PoolError::Unsupported);
            }
            if value.recovery.as_ref().is_some_and(|recovery| {
                !matches!(
                    (value.phase.as_str(), recovery.checkpoint.as_str()),
                    ("initializing", "intent_recorded")
                        | ("active", "intent_recorded" | "completed")
                )
            }) {
                return Err(PoolError::Corrupt);
            }
            validate_recovery_history(&value)?;
            crate::relocation::validate(&value)?;
            Ok(value)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(PoolError::Missing),
        Err(error) => Err(error.into()),
    }
}
fn write_locator(path: &Path, locator: &Locator, create: bool) -> Result<(), PoolError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&serde_json::to_vec(locator).map_err(|_| PoolError::Corrupt)?)?;
    file.sync_all()?;
    if create {
        fs::File::open(path.parent().ok_or(PoolError::Configuration)?)?.sync_all()?;
    }
    Ok(())
}
/// Observations occur only after the named durable boundary; they never skip effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitializationCheckpoint {
    IntentRecorded,
    StoreCommitted,
    AuthorityPublished,
}

/// # Errors
/// Rejects existing/conflicting authority and failed durable storage operations.
pub fn initialize(paths: &Paths) -> Result<CatalogProjection, PoolError> {
    initialize_observed(paths, |_| {})
}
/// # Errors
/// Rejects unsafe authority, unsupported state, or failed durable filesystem/storage effects.
pub fn initialize_observed(
    paths: &Paths,
    mut observe: impl FnMut(InitializationCheckpoint),
) -> Result<CatalogProjection, PoolError> {
    private_directory(&paths.state)?;
    let _maintenance = LockGuard::acquire(&paths.state.join("maintenance.lock"), true)?;
    let _catalog = LockGuard::acquire(&paths.state.join("catalog.lock"), true)?;
    match read_locator(paths) {
        Ok(locator) if crate::relocation::pending(&locator) => {
            return Err(PoolError::RelocationPending);
        }
        Ok(locator) if locator.recovery_pending() => return Err(PoolError::Pending),
        Ok(locator) if locator.phase == "active" => {
            // Validate existing authority before rejecting reinitialization.
            let _ = Store::inspect_read_only(&paths.catalog, locator.catalog_id)?;
            return Err(PoolError::AlreadyInitialized);
        }
        Ok(_) => return Err(PoolError::Pending),
        Err(PoolError::Missing) => {}
        Err(error) => return Err(error),
    }
    if paths.catalog.exists() {
        return Err(PoolError::Conflict);
    }
    private_directory(paths.catalog.parent().ok_or(PoolError::Configuration)?)?;
    let id = CatalogId::new();
    let mut locator = Locator {
        version: 1,
        catalog_id: id,
        catalog_path: EncodedPath::from_path(&paths.catalog),
        phase: "initializing".into(),
        recovery: None,
        recovery_history: Vec::new(),
        relocation: None,
        relocation_history: Vec::new(),
    };
    // Durable identity and intent precede database creation. Interrupted init cannot reset authority.
    write_locator(&paths.state.join("active.json"), &locator, true)?;
    observe(InitializationCheckpoint::IntentRecorded);
    let projection = Store::create_with_id(&paths.catalog, id)?;
    fs::File::open(paths.catalog.parent().ok_or(PoolError::Configuration)?)?.sync_all()?;
    observe(InitializationCheckpoint::StoreCommitted);
    locator.phase = "active".into();
    let temporary = paths
        .state
        .join(format!("locator-{}.tmp", uuid::Uuid::new_v4()));
    write_locator(&temporary, &locator, false)?;
    fs::rename(temporary, paths.state.join("active.json"))?;
    fs::File::open(&paths.state)?.sync_all()?;
    observe(InitializationCheckpoint::AuthorityPublished);
    Ok(projection)
}
/// # Errors
/// Rejects missing, pending, conflicting, unsupported, or corrupt catalog state.
pub fn open(paths: &Paths) -> Result<CatalogSession, PoolError> {
    // Ordinary commands do not initialize missing catalog or coordination state.
    if !paths.state.join("active.json").exists() {
        return Err(PoolError::Missing);
    }
    private_directory(&paths.state)?;
    let maintenance = LockGuard::acquire(&paths.state.join("maintenance.lock"), false)?;
    let catalog = LockGuard::acquire(&paths.state.join("catalog.lock"), true)?;
    let locator = read_locator(paths)?;
    if crate::relocation::pending(&locator) {
        return Err(PoolError::RelocationPending);
    }
    if locator.phase == "initializing" || locator.recovery_pending() {
        return Err(PoolError::Pending);
    }
    if locator.phase != "active" {
        return Err(PoolError::Corrupt);
    }
    private_file(&paths.catalog).map_err(|error| match error {
        PoolError::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound => PoolError::Missing,
        _ => error,
    })?;
    if !locator.relocation_history.is_empty() {
        let projection = Store::inspect_read_only(&paths.catalog, locator.catalog_id)?;
        crate::relocation::validate_operations(&locator, &projection)?;
    }
    let store = Store::open(&paths.catalog, locator.catalog_id)?;
    let projection = store.projection()?;
    Ok(CatalogSession {
        projection,
        store,
        _catalog_lock: catalog,
        _maintenance_lock: maintenance,
    })
}

/// Rebuilds only derived state under whole-command exclusive maintenance.
/// # Errors
/// Refuses missing, pending, foreign, unsupported or corrupt authoritative state.
pub fn rebuild(paths: &Paths) -> Result<CatalogProjection, PoolError> {
    rebuild_observed(paths, |_| {})
}

/// # Errors
/// Preserves rebuild refusal and indeterminate-commit semantics.
pub fn rebuild_observed(
    paths: &Paths,
    observe: impl FnMut(crate::rebuild::RebuildCheckpoint),
) -> Result<CatalogProjection, PoolError> {
    if !paths.state.join("active.json").exists() {
        return Err(PoolError::Missing);
    }
    let _maintenance = LockGuard::acquire(&paths.state.join("maintenance.lock"), true)?;
    let _catalog = LockGuard::acquire(&paths.state.join("catalog.lock"), true)?;
    let locator = read_locator(paths)?;
    if crate::relocation::pending(&locator) {
        return Err(PoolError::RelocationPending);
    }
    if locator.phase == "initializing" || locator.recovery_pending() {
        return Err(PoolError::Pending);
    }
    if locator.phase != "active" {
        return Err(PoolError::Corrupt);
    }
    private_file(&paths.catalog).map_err(|error| match error {
        PoolError::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound => PoolError::Missing,
        _ => error,
    })?;
    if !locator.relocation_history.is_empty() {
        let authoritative = Store::inspect_rebuild_history(&paths.catalog, locator.catalog_id)?;
        crate::relocation::validate_operations(&locator, &authoritative)?;
    }
    Store::rebuild(&paths.catalog, locator.catalog_id, observe)
}

/// Inspects active catalog projection without changing protected storage.
/// # Errors
/// Rejects missing, pending, conflicting, unsupported, or corrupt authority.
pub fn inspect_projection(paths: &Paths) -> Result<CatalogProjection, PoolError> {
    if !paths.state.join("active.json").exists() {
        return Err(PoolError::Missing);
    }
    let _maintenance = LockGuard::acquire(&paths.state.join("maintenance.lock"), false)?;
    let _catalog = LockGuard::acquire(&paths.state.join("catalog.lock"), true)?;
    let locator = read_locator(paths)?;
    if crate::relocation::pending(&locator) {
        return Err(PoolError::RelocationPending);
    }
    if locator.phase == "initializing" || locator.recovery_pending() {
        return Err(PoolError::Pending);
    }
    if locator.phase != "active" {
        return Err(PoolError::Corrupt);
    }
    private_file(&paths.catalog).map_err(|error| match error {
        PoolError::Io(ref error) if error.kind() == std::io::ErrorKind::NotFound => {
            PoolError::Missing
        }
        _ => error,
    })?;
    let projection = Store::inspect_read_only(&paths.catalog, locator.catalog_id)?;
    crate::relocation::validate_operations(&locator, &projection)?;
    Ok(projection)
}

/// Inspects retained catalog events under the same authority coordination.
/// # Errors
/// Rejects missing, pending, conflicting, unsupported, or corrupt authority.
pub fn inspect_events(
    paths: &Paths,
) -> Result<(CatalogProjection, Vec<serde_json::Value>), PoolError> {
    if !paths.state.join("active.json").exists() {
        return Err(PoolError::Missing);
    }
    let _maintenance = LockGuard::acquire(&paths.state.join("maintenance.lock"), false)?;
    let _catalog = LockGuard::acquire(&paths.state.join("catalog.lock"), true)?;
    let locator = read_locator(paths)?;
    if crate::relocation::pending(&locator) {
        return Err(PoolError::RelocationPending);
    }
    if locator.phase == "initializing" || locator.recovery_pending() {
        return Err(PoolError::Pending);
    }
    if locator.phase != "active" {
        return Err(PoolError::Corrupt);
    }
    private_file(&paths.catalog).map_err(|error| match error {
        PoolError::Io(ref error) if error.kind() == std::io::ErrorKind::NotFound => {
            PoolError::Missing
        }
        _ => error,
    })?;
    let projection = Store::inspect_read_only(&paths.catalog, locator.catalog_id)?;
    crate::relocation::validate_operations(&locator, &projection)?;
    let events = Store::events_read_only(&paths.catalog, locator.catalog_id)?;
    Ok((projection, events))
}

#[derive(Serialize)]
pub struct AuthorityObservation {
    pub catalog_id: CatalogId,
    pub catalog_path: EncodedPath,
    pub phase: String,
    pub last_checkpoint: &'static str,
    pub store_state: &'static str,
    pub revision: Option<u64>,
    pub recovery_operation_id: Option<OperationId>,
    pub recovery_checkpoint: Option<String>,
    pub recovery_history: Vec<AuthorityRecoveryFact>,
    pub relocation: Option<crate::relocation::Relocation>,
    pub relocation_history: Vec<crate::relocation::RelocationFact>,
}

impl AuthorityObservation {
    /// Whether global catalog authority permits ordinary mutations.
    #[must_use]
    pub fn mutations_available(&self) -> bool {
        self.phase == "active"
            && self.store_state == "validated"
            && self.recovery_checkpoint.as_deref() != Some("intent_recorded")
            && self
                .relocation
                .as_ref()
                .is_none_or(|r| r.checkpoint == crate::relocation::Checkpoint::Completed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityRecoveryFact {
    pub schema_version: u32,
    pub position: u64,
    pub operation_id: OperationId,
    pub kind: AuthorityRecoveryKind,
    pub checkpoint: String,
    pub legacy: bool,
}

/// An exact known catalog recovery result; it never selects the newest operation.
/// # Errors
/// Rejects missing authority or an unknown operation identity.
pub fn inspect_authority_operation(
    paths: &Paths,
    operation_id: OperationId,
) -> Result<AuthorityRecoveryFact, PoolError> {
    select_authority_operation(&inspect_authority(paths)?.recovery_history, operation_id)
}

/// Explicitly reconciles only the selected catalog operation.
/// # Errors
/// Rejects unknown identities and unsafe authority state.
pub fn reconcile_authority_operation(
    paths: &Paths,
    operation_id: OperationId,
) -> Result<AuthorityRecoveryFact, PoolError> {
    reconcile_authority_operation_observed(paths, operation_id, |_| {})
}

/// Reconciles one exact catalog operation with durable checkpoint observations.
/// # Errors
/// Rejects unknown identities before effects, and unsafe authority state.
pub fn reconcile_authority_operation_observed(
    paths: &Paths,
    operation_id: OperationId,
    observe: impl FnMut(AuthorityRecoveryCheckpoint),
) -> Result<AuthorityRecoveryFact, PoolError> {
    let authority = reconcile_authority_selected(paths, Some(operation_id), observe)?;
    select_authority_operation(&authority.recovery_history, operation_id)
}

/// Lists the latest recorded checkpoint for every known catalog recovery operation.
/// # Errors
/// Rejects invalid or missing locator authority; does not require readable database state.
pub fn list_authority_operations(paths: &Paths) -> Result<Vec<AuthorityRecoveryFact>, PoolError> {
    let authority = inspect_active_authority(paths)?;
    let mut seen = std::collections::HashSet::new();
    let mut operations = authority
        .recovery_history
        .into_iter()
        .rev()
        .filter(|fact| seen.insert(fact.operation_id.to_string()))
        .collect::<Vec<_>>();
    operations.reverse();
    Ok(operations)
}

fn select_authority_operation(
    history: &[AuthorityRecoveryFact],
    operation_id: OperationId,
) -> Result<AuthorityRecoveryFact, PoolError> {
    history
        .iter()
        .rev()
        .find(|fact| fact.operation_id == operation_id)
        .cloned()
        .ok_or(PoolError::Unregistered)
}

fn recovery_history(locator: &Locator) -> Vec<AuthorityRecoveryFact> {
    if !locator.recovery_history.is_empty() {
        return locator.recovery_history.clone();
    }
    // Older locators record only one observed checkpoint, not an invented intent chain.
    locator
        .recovery
        .as_ref()
        .map(|recovery| AuthorityRecoveryFact {
            schema_version: 1,
            position: 1,
            operation_id: recovery.operation_id,
            kind: recovery.kind,
            checkpoint: recovery.checkpoint.clone(),
            legacy: true,
        })
        .into_iter()
        .collect()
}

fn validate_recovery_history(locator: &Locator) -> Result<(), PoolError> {
    let mut latest = std::collections::HashMap::<String, &AuthorityRecoveryFact>::new();
    for (offset, fact) in locator.recovery_history.iter().enumerate() {
        if fact.schema_version != 1 {
            return Err(PoolError::Unsupported);
        }
        let expected = u64::try_from(offset).map_err(|_| PoolError::Corrupt)? + 1;
        if fact.position != expected
            || !matches!(fact.checkpoint.as_str(), "intent_recorded" | "completed")
            || (fact.legacy && offset != 0)
        {
            return Err(PoolError::Corrupt);
        }
        if let Some(previous) = latest.get(&fact.operation_id.to_string()) {
            if previous.checkpoint != "intent_recorded"
                || fact.checkpoint != "completed"
                || previous.kind != fact.kind
                || fact.legacy
            {
                return Err(PoolError::Corrupt);
            }
        } else if !fact.legacy && fact.checkpoint != "intent_recorded" {
            return Err(PoolError::Corrupt);
        }
        latest.insert(fact.operation_id.to_string(), fact);
    }
    if let Some(last) = locator.recovery_history.last() {
        let current = locator.recovery.as_ref().ok_or(PoolError::Corrupt)?;
        if last.operation_id != current.operation_id
            || last.kind != current.kind
            || last.checkpoint != current.checkpoint
            || latest
                .values()
                .filter(|fact| fact.checkpoint == "intent_recorded")
                .count()
                != usize::from(current.checkpoint == "intent_recorded")
        {
            return Err(PoolError::Corrupt);
        }
    }
    Ok(())
}

fn append_recovery_fact(locator: &mut Locator) -> Result<(), PoolError> {
    let current = locator.recovery.as_ref().ok_or(PoolError::Corrupt)?;
    let position = u64::try_from(locator.recovery_history.len())
        .map_err(|_| PoolError::Corrupt)?
        .checked_add(1)
        .ok_or(PoolError::Corrupt)?;
    locator.recovery_history.push(AuthorityRecoveryFact {
        schema_version: 1,
        position,
        operation_id: current.operation_id,
        kind: current.kind,
        checkpoint: current.checkpoint.clone(),
        legacy: false,
    });
    Ok(())
}

/// Read-only reconciliation evidence; observing a valid database never publishes it.
/// # Errors
/// Rejects missing, conflicting, malformed, or unsupported locator state.
pub fn inspect_authority(paths: &Paths) -> Result<AuthorityObservation, PoolError> {
    if !paths.state.join("active.json").exists() {
        return Err(PoolError::Missing);
    }
    let _maintenance = LockGuard::acquire(&paths.state.join("maintenance.lock"), false)?;
    let _catalog = LockGuard::acquire(&paths.state.join("catalog.lock"), true)?;
    let locator = read_locator(paths)?;
    observe_authority(paths, locator)
}

/// Observes the stable locator's active location without adopting an obsolete selector.
/// # Errors
/// Rejects missing, unsupported or corrupt stable authority.
pub fn inspect_active_authority(paths: &Paths) -> Result<AuthorityObservation, PoolError> {
    if !paths.state.join("active.json").exists() {
        return Err(PoolError::Missing);
    }
    let _maintenance = LockGuard::acquire(&paths.state.join("maintenance.lock"), false)?;
    let _catalog = LockGuard::acquire(&paths.state.join("catalog.lock"), true)?;
    let locator = read_active_locator(paths)?;
    let active = Paths {
        catalog: locator.catalog_path.to_path()?,
        ..paths.clone()
    };
    observe_authority(&active, locator)
}

fn observe_authority(paths: &Paths, locator: Locator) -> Result<AuthorityObservation, PoolError> {
    if locator.phase != "active" && locator.phase != "initializing" {
        return Err(PoolError::Corrupt);
    }
    let projection = if paths.catalog.exists() {
        private_file(&paths.catalog)
            .and_then(|()| Store::inspect_read_only(&paths.catalog, locator.catalog_id))
            .ok()
    } else {
        None
    };
    let rebuild_projection = if projection.is_none() && paths.catalog.exists() {
        private_file(&paths.catalog)
            .and_then(|()| Store::inspect_rebuild_history(&paths.catalog, locator.catalog_id))
            .ok()
    } else {
        None
    };
    if let Some(validated) = projection.as_ref().or(rebuild_projection.as_ref()) {
        crate::relocation::validate_operations(&locator, validated)?;
    }
    let rebuild_revision = rebuild_projection.map(|p| p.revision);
    let store_state = if projection.is_some() {
        "validated"
    } else if rebuild_revision.is_some() {
        "rebuild_required"
    } else if paths.catalog.exists() {
        "unreadable"
    } else {
        "missing"
    };
    let last_checkpoint = if locator.phase == "active" && locator.recovery_pending() {
        "recovery_intended"
    } else if locator.phase == "active" {
        "authority_published"
    } else if projection.is_some() {
        "store_committed"
    } else {
        "intent_recorded"
    };
    let recovery_history = recovery_history(&locator);
    Ok(AuthorityObservation {
        catalog_id: locator.catalog_id,
        catalog_path: locator.catalog_path,
        phase: locator.phase,
        last_checkpoint,
        store_state,
        revision: projection.map(|p| p.revision).or(rebuild_revision),
        recovery_operation_id: locator.recovery.as_ref().map(|r| r.operation_id),
        recovery_checkpoint: locator.recovery.map(|r| r.checkpoint),
        recovery_history,
        relocation: locator.relocation,
        relocation_history: locator.relocation_history,
    })
}

/// Explicitly reconciles a recorded initialization without creating a new authority.
/// # Errors
/// Rejects missing, conflicting, malformed, unsupported, or unsafe authority state.
pub fn reconcile_authority(paths: &Paths) -> Result<AuthorityObservation, PoolError> {
    reconcile_authority_observed(paths, |_| {})
}
/// Durable boundaries of explicit bootstrap reconciliation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityRecoveryCheckpoint {
    IntentRecorded,
    StoreCommitted,
    AuthorityPublished,
}

pub(crate) fn replace_locator(paths: &Paths, locator: &Locator) -> Result<(), PoolError> {
    let temporary = paths
        .state
        .join(format!("locator-{}.tmp", uuid::Uuid::new_v4()));
    write_locator(&temporary, locator, false)?;
    fs::rename(temporary, paths.state.join("active.json"))?;
    fs::File::open(&paths.state)?.sync_all()?;
    Ok(())
}

/// # Errors
/// Rejects unsafe authority or failed durable storage and filesystem effects.
pub fn reconcile_authority_observed(
    paths: &Paths,
    observe: impl FnMut(AuthorityRecoveryCheckpoint),
) -> Result<AuthorityObservation, PoolError> {
    reconcile_authority_selected(paths, None, observe)
}

fn recovery_store(
    paths: &Paths,
    locator: &Locator,
) -> Result<(Option<CatalogProjection>, bool), PoolError> {
    // A present but unreadable file is never treated as absent or replaced.
    let observation = match fs::symlink_metadata(&paths.catalog) {
        Ok(_) => {
            private_file(&paths.catalog)?;
            match Store::inspect_for_recovery(&paths.catalog, locator.catalog_id) {
                Ok(projection) => (Some(projection), false),
                Err(ReadOnlyInspectionError::RepairRequired) => (None, true),
                Err(ReadOnlyInspectionError::Rejected(PoolError::Corrupt))
                    if locator.phase == "active"
                        && locator.recovery_pending()
                        && locator
                            .recovery
                            .as_ref()
                            .is_some_and(|r| r.kind == AuthorityRecoveryKind::StorageRepair) =>
                {
                    (
                        Some(Store::inspect_rebuild_history(
                            &paths.catalog,
                            locator.catalog_id,
                        )?),
                        false,
                    )
                }
                Err(ReadOnlyInspectionError::Rejected(error)) => return Err(error),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (None, false),
        Err(error) => return Err(error.into()),
    };
    Ok(observation)
}

fn reconcile_authority_selected(
    paths: &Paths,
    selected_operation: Option<OperationId>,
    mut observe: impl FnMut(AuthorityRecoveryCheckpoint),
) -> Result<AuthorityObservation, PoolError> {
    if !paths.state.join("active.json").exists() {
        return Err(PoolError::Missing);
    }
    let _maintenance = LockGuard::acquire(&paths.state.join("maintenance.lock"), true)?;
    let _catalog = LockGuard::acquire(&paths.state.join("catalog.lock"), true)?;
    let mut locator = read_locator(paths)?;
    if let Some(operation_id) = selected_operation {
        let operation = select_authority_operation(&recovery_history(&locator), operation_id)?;
        if operation.checkpoint == "completed" {
            return observe_authority(paths, locator);
        }
        if locator
            .recovery
            .as_ref()
            .map(|recovery| recovery.operation_id)
            != Some(operation_id)
        {
            return Err(PoolError::Corrupt);
        }
    }
    if crate::relocation::pending(&locator) {
        return Err(PoolError::RelocationPending);
    }
    if locator.phase != "active" && locator.phase != "initializing" {
        return Err(PoolError::Corrupt);
    }
    let (existing, repair_required) = recovery_store(paths, &locator)?;
    if let Some(projection) = &existing {
        crate::relocation::validate_operations(&locator, projection)?;
    }
    if locator.phase == "active" && !repair_required {
        let projection = existing.as_ref().ok_or(PoolError::Missing)?;
        if !locator.recovery_pending() {
            return Ok(authority_reconciled(locator, projection.revision));
        }
    }
    let legacy_history = locator.recovery_history.is_empty() && locator.recovery.is_some();
    if legacy_history {
        locator.recovery_history = recovery_history(&locator);
    }
    if !locator.recovery_pending() {
        locator.recovery = Some(AuthorityRecovery {
            operation_id: OperationId::new(),
            checkpoint: "intent_recorded".into(),
            kind: if locator.phase == "initializing" {
                AuthorityRecoveryKind::Bootstrap
            } else {
                AuthorityRecoveryKind::StorageRepair
            },
        });
        append_recovery_fact(&mut locator)?;
        replace_locator(paths, &locator)?;
    } else if legacy_history {
        replace_locator(paths, &locator)?;
    }
    observe(AuthorityRecoveryCheckpoint::IntentRecorded);
    let validated_authority = if repair_required
        && locator.phase == "active"
        && locator
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.kind == AuthorityRecoveryKind::StorageRepair)
    {
        // Only explicit repair of already-active authority may leave derived damage.
        Store::repair_authority(&paths.catalog, locator.catalog_id)?
    } else if repair_required {
        // Bootstrap publication keeps its original strict complete-projection contract.
        Store::open(&paths.catalog, locator.catalog_id)?.projection()?
    } else if let Some(projection) = existing {
        projection
    } else {
        match Store::create_with_id(&paths.catalog, locator.catalog_id) {
            Ok(projection) => projection,
            Err(PoolError::CommitUnknown) => {
                // The real handle has closed: inspect the recorded identity before any retry.
                Store::open(&paths.catalog, locator.catalog_id)
                    .and_then(|store| store.projection())
                    .map_err(|_| PoolError::CommitUnknown)?
            }
            Err(error) => return Err(error),
        }
    };
    crate::relocation::validate_operations(&locator, &validated_authority)?;
    fs::File::open(paths.catalog.parent().ok_or(PoolError::Configuration)?)?.sync_all()?;
    observe(AuthorityRecoveryCheckpoint::StoreCommitted);
    locator.phase = "active".into();
    locator
        .recovery
        .as_mut()
        .ok_or(PoolError::Corrupt)?
        .checkpoint = "completed".into();
    append_recovery_fact(&mut locator)?;
    replace_locator(paths, &locator)?;
    observe(AuthorityRecoveryCheckpoint::AuthorityPublished);
    observe_authority(paths, locator)
}

fn authority_reconciled(locator: Locator, revision: u64) -> AuthorityObservation {
    let recovery_history = recovery_history(&locator);
    AuthorityObservation {
        catalog_id: locator.catalog_id,
        catalog_path: locator.catalog_path,
        phase: locator.phase,
        last_checkpoint: "authority_published",
        store_state: "validated",
        revision: Some(revision),
        recovery_operation_id: locator.recovery.as_ref().map(|r| r.operation_id),
        recovery_checkpoint: locator.recovery.map(|r| r.checkpoint),
        recovery_history,
        relocation: locator.relocation,
        relocation_history: locator.relocation_history,
    }
}

#[cfg(test)]
mod recovery_tests {
    use std::{
        fs,
        os::unix::fs::{OpenOptionsExt, symlink},
    };

    use googletest::{assert_that, matchers::eq};

    use super::{
        AuthorityRecoveryCheckpoint, InitializationCheckpoint, initialize, initialize_observed,
        inspect_authority, inspect_authority_operation, list_authority_operations, open,
        reconcile_authority, reconcile_authority_observed, reconcile_authority_operation,
    };
    use crate::{error::PoolError, management::OperationId, paths::Paths, store::Store};

    #[googletest::test]
    fn explicit_reconciliation_finishes_recorded_initialization_without_new_identity() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let interrupted = std::panic::catch_unwind(|| {
            initialize_observed(&paths, |checkpoint| {
                if checkpoint == InitializationCheckpoint::IntentRecorded {
                    panic!("interrupt after durable identity");
                }
            })
            .unwrap();
        });
        assert_that!(interrupted.is_err(), eq(true));
        let pending = inspect_authority(&paths).unwrap();
        assert_that!(pending.phase.as_str(), eq("initializing"));
        assert_that!(pending.store_state, eq("missing"));
        let reconciled = reconcile_authority(&paths).unwrap();
        assert_that!(reconciled.phase.as_str(), eq("active"));
        assert_that!(reconciled.catalog_id, eq(pending.catalog_id));
        assert_that!(reconciled.revision, eq(Some(1)));
        assert_that!(
            open(&paths).unwrap().projection.catalog_id,
            eq(pending.catalog_id)
        );
    }
    #[googletest::test]
    fn unknown_recovery_checkpoint_rejects_without_replacing_authority() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        initialize(&paths).unwrap();
        let locator_path = paths.state.join("active.json");
        let mut locator: serde_json::Value =
            serde_json::from_slice(&fs::read(&locator_path).unwrap()).unwrap();
        locator["recovery"] = serde_json::json!({
            "operation_id": OperationId::new(),
            "checkpoint": "unrecognized_checkpoint",
        });
        fs::write(&locator_path, serde_json::to_vec(&locator).unwrap()).unwrap();
        let before_locator = fs::read(&locator_path).unwrap();
        let before_catalog = fs::read(&paths.catalog).unwrap();
        assert_that!(
            matches!(reconcile_authority(&paths), Err(PoolError::Corrupt)),
            eq(true)
        );
        assert_that!(fs::read(&locator_path).unwrap(), eq(&before_locator));
        assert_that!(fs::read(&paths.catalog).unwrap(), eq(&before_catalog));
    }

    #[googletest::test]
    fn explicit_reconciliation_preserves_committed_initialization_and_active_noop() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let interrupted = std::panic::catch_unwind(|| {
            initialize_observed(&paths, |checkpoint| {
                if checkpoint == InitializationCheckpoint::StoreCommitted {
                    panic!("interrupt after committed initialization");
                }
            })
            .unwrap();
        });
        assert_that!(interrupted.is_err(), eq(true));
        let pending = inspect_authority(&paths).unwrap();
        let before = Store::open(&paths.catalog, pending.catalog_id)
            .unwrap()
            .events()
            .unwrap();
        let completed = reconcile_authority(&paths).unwrap();
        assert_that!(completed.catalog_id, eq(pending.catalog_id));
        assert_that!(
            completed.recovery_checkpoint.as_deref(),
            eq(Some("completed"))
        );
        assert_that!(
            Store::open(&paths.catalog, pending.catalog_id)
                .unwrap()
                .events()
                .unwrap(),
            eq(&before)
        );
        let locator = fs::read(paths.state.join("active.json")).unwrap();
        let catalog = fs::read(&paths.catalog).unwrap();
        let noop = reconcile_authority(&paths).unwrap();
        assert_that!(
            noop.recovery_operation_id,
            eq(completed.recovery_operation_id)
        );
        assert_that!(
            fs::read(paths.state.join("active.json")).unwrap(),
            eq(&locator)
        );
        assert_that!(fs::read(&paths.catalog).unwrap() == catalog, eq(true));
    }

    #[googletest::test]
    fn interrupted_bootstrap_recovery_reuses_identity_and_committed_history() {
        for boundary in [
            AuthorityRecoveryCheckpoint::IntentRecorded,
            AuthorityRecoveryCheckpoint::StoreCommitted,
            AuthorityRecoveryCheckpoint::AuthorityPublished,
        ] {
            let root = tempfile::tempdir().unwrap();
            let paths = Paths {
                catalog: root.path().join("data/catalog.redb"),
                state: root.path().join("state"),
                json: true,
            };
            let interrupted = std::panic::catch_unwind(|| {
                initialize_observed(&paths, |checkpoint| {
                    if checkpoint == InitializationCheckpoint::IntentRecorded {
                        panic!("interrupt initialization");
                    }
                })
                .unwrap();
            });
            assert_that!(interrupted.is_err(), eq(true));
            let catalog_id = inspect_authority(&paths).unwrap().catalog_id;
            let interrupted = std::panic::catch_unwind(|| {
                reconcile_authority_observed(&paths, |checkpoint| {
                    if checkpoint == boundary {
                        panic!("interrupt recovery");
                    }
                })
                .unwrap();
            });
            assert_that!(interrupted.is_err(), eq(true));
            let locator_before = fs::read(paths.state.join("active.json")).unwrap();
            let catalog_before = fs::read(&paths.catalog).ok();
            let observed = inspect_authority(&paths).unwrap();
            assert_that!(observed.catalog_id, eq(catalog_id));
            assert_that!(observed.recovery_operation_id.is_some(), eq(true));
            assert_that!(
                fs::read(paths.state.join("active.json")).unwrap(),
                eq(&locator_before)
            );
            assert_that!(fs::read(&paths.catalog).ok() == catalog_before, eq(true));
            let events_before = catalog_before.map(|_| {
                Store::open(&paths.catalog, catalog_id)
                    .unwrap()
                    .events()
                    .unwrap()
            });
            let recovered = reconcile_authority(&paths).unwrap();
            assert_that!(recovered.catalog_id, eq(catalog_id));
            assert_that!(
                recovered.recovery_operation_id,
                eq(observed.recovery_operation_id)
            );
            assert_that!(recovered.phase.as_str(), eq("active"));
            assert_that!(
                recovered.recovery_checkpoint.as_deref(),
                eq(Some("completed"))
            );
            let events = Store::open(&paths.catalog, catalog_id)
                .unwrap()
                .events()
                .unwrap();
            assert_that!(events.len(), eq(1));
            if let Some(events_before) = events_before {
                assert_that!(events, eq(&events_before));
            }
        }
    }

    #[googletest::test]
    fn bootstrap_recovery_refuses_present_unsafe_storage_without_overwrite() {
        for state in [
            "corrupt",
            "conflicting_identity",
            "symlink",
            "dangling_symlink",
            "directory",
        ] {
            let root = tempfile::tempdir().unwrap();
            let paths = Paths {
                catalog: root.path().join("data/catalog.redb"),
                state: root.path().join("state"),
                json: true,
            };
            let interrupted = std::panic::catch_unwind(|| {
                initialize_observed(&paths, |checkpoint| {
                    if checkpoint == InitializationCheckpoint::IntentRecorded {
                        panic!("interrupt initialization");
                    }
                })
                .unwrap();
            });
            assert_that!(interrupted.is_err(), eq(true));
            let target = root.path().join("retained");
            fs::write(&target, b"protected bytes").unwrap();
            match state {
                "corrupt" => {
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .mode(0o600)
                        .open(&paths.catalog)
                        .unwrap();
                    fs::write(&paths.catalog, b"not a database").unwrap();
                }
                "conflicting_identity" => {
                    Store::create(&paths.catalog).unwrap();
                }
                "symlink" => symlink(&target, &paths.catalog).unwrap(),
                "dangling_symlink" => {
                    symlink(root.path().join("missing-target"), &paths.catalog).unwrap()
                }
                "directory" => {
                    fs::create_dir(&paths.catalog).unwrap();
                    fs::write(paths.catalog.join("retained"), b"directory sentinel").unwrap();
                }
                _ => unreachable!(),
            }
            let locator_before = fs::read(paths.state.join("active.json")).unwrap();
            let catalog_before = fs::read(&paths.catalog).ok();
            let link_before = fs::read_link(&paths.catalog).ok();
            assert_that!(reconcile_authority(&paths).is_err(), eq(true));
            assert_that!(
                fs::read(paths.state.join("active.json")).unwrap(),
                eq(&locator_before)
            );
            assert_that!(fs::read(&paths.catalog).ok() == catalog_before, eq(true));
            assert_that!(fs::read_link(&paths.catalog).ok(), eq(&link_before));
            assert_that!(fs::read(&target).unwrap(), eq(&b"protected bytes".to_vec()));
            if state == "directory" {
                assert_that!(
                    fs::read(paths.catalog.join("retained")).unwrap(),
                    eq(&b"directory sentinel".to_vec())
                );
                assert_that!(fs::read_dir(&paths.catalog).unwrap().count(), eq(1));
            }
        }
    }

    fn crash_catalog_writer(path: &std::path::Path) {
        let ready = path.with_extension(format!("writer-ready-{}", uuid::Uuid::new_v4()));
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .env_clear()
            .env("POOL_UNCLEAN_TEST_CATALOG", path)
            .env("POOL_UNCLEAN_TEST_READY", &ready)
            .args(["--exact", "catalog::recovery_tests::explicit_apply_recovers_unclean_redb_without_resetting_identity"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while !ready.exists() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("unclean writer exited before readiness: {status}");
            }
            if std::time::Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("unclean writer readiness deadline");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert_that!(
            std::os::unix::process::ExitStatusExt::signal(&status),
            eq(Some(9))
        );
    }

    #[googletest::test]
    fn explicit_apply_recovers_unclean_redb_without_resetting_identity() {
        if let Some(path) = std::env::var_os("POOL_UNCLEAN_TEST_CATALOG") {
            let database = redb::Database::open(path).unwrap();
            let mut transaction = database.begin_write().unwrap();
            transaction
                .set_durability(redb::Durability::Immediate)
                .unwrap();
            transaction.set_quick_repair(false);
            transaction.commit().unwrap();
            fs::write(
                std::env::var_os("POOL_UNCLEAN_TEST_READY").unwrap(),
                b"ready",
            )
            .unwrap();
            loop {
                std::thread::park();
            }
        }
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let initialized = initialize(&paths).unwrap();
        let events = Store::events_read_only(&paths.catalog, initialized.catalog_id).unwrap();
        crash_catalog_writer(&paths.catalog);
        assert_that!(
            matches!(
                redb::ReadOnlyDatabase::open(&paths.catalog),
                Err(redb::DatabaseError::RepairAborted)
            ),
            eq(true)
        );
        let before_catalog = fs::read(&paths.catalog).unwrap();
        let before_locator = fs::read(paths.state.join("active.json")).unwrap();
        let preview = inspect_authority(&paths).unwrap();
        assert_that!(preview.store_state, eq("unreadable"));
        assert_that!(
            fs::read(&paths.catalog).unwrap() == before_catalog,
            eq(true)
        );
        assert_that!(
            fs::read(paths.state.join("active.json")).unwrap(),
            eq(&before_locator)
        );
        let completed = reconcile_authority(&paths).unwrap();
        assert_that!(completed.catalog_id, eq(initialized.catalog_id));
        assert_that!(completed.phase.as_str(), eq("active"));
        assert_that!(
            completed.recovery_checkpoint.as_deref(),
            eq(Some("completed"))
        );
        assert_that!(
            Store::events_read_only(&paths.catalog, initialized.catalog_id).unwrap(),
            eq(&events)
        );
        assert_that!(
            Store::inspect_read_only(&paths.catalog, initialized.catalog_id).unwrap(),
            eq(&initialized)
        );
    }

    #[googletest::test]
    fn later_storage_repair_preserves_prior_known_lifecycle_result() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        initialize(&paths).unwrap();
        crash_catalog_writer(&paths.catalog);
        let first = reconcile_authority(&paths).unwrap();
        let first_id = first.recovery_operation_id.unwrap();
        assert_that!(
            inspect_authority_operation(&paths, first_id)
                .unwrap()
                .checkpoint
                .as_str(),
            eq("completed")
        );
        crash_catalog_writer(&paths.catalog);
        let second = reconcile_authority(&paths).unwrap();
        assert_that!(second.recovery_operation_id == Some(first_id), eq(false));
        assert_that!(
            second.recovery_history.starts_with(&first.recovery_history),
            eq(true)
        );
        assert_that!(
            inspect_authority_operation(&paths, first_id)
                .unwrap()
                .checkpoint
                .as_str(),
            eq("completed")
        );
    }

    #[googletest::test]
    fn historical_catalog_apply_never_completes_a_newer_pending_repair() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        initialize(&paths).unwrap();
        crash_catalog_writer(&paths.catalog);
        let first_id = reconcile_authority(&paths)
            .unwrap()
            .recovery_operation_id
            .unwrap();
        crash_catalog_writer(&paths.catalog);
        let interrupted = std::panic::catch_unwind(|| {
            reconcile_authority_observed(&paths, |checkpoint| {
                if checkpoint == AuthorityRecoveryCheckpoint::IntentRecorded {
                    panic!("interrupt second repair after durable intent");
                }
            })
            .unwrap();
        });
        assert_that!(interrupted.is_err(), eq(true));
        let pending = inspect_authority(&paths).unwrap();
        let second_id = pending.recovery_operation_id.unwrap();
        assert_that!(second_id == first_id, eq(false));
        assert_that!(matches!(open(&paths), Err(PoolError::Pending)), eq(true));
        assert_that!(
            matches!(initialize(&paths), Err(PoolError::Pending)),
            eq(true)
        );
        let locator = fs::read(paths.state.join("active.json")).unwrap();
        let catalog = fs::read(&paths.catalog).unwrap();
        let previous = reconcile_authority_operation(&paths, first_id).unwrap();
        assert_that!(previous.checkpoint.as_str(), eq("completed"));
        assert_that!(
            fs::read(paths.state.join("active.json")).unwrap() == locator,
            eq(true)
        );
        assert_that!(fs::read(&paths.catalog).unwrap() == catalog, eq(true));
        assert_that!(
            matches!(
                reconcile_authority_operation(&paths, OperationId::new()),
                Err(PoolError::Unregistered)
            ),
            eq(true)
        );
        assert_that!(
            fs::read(paths.state.join("active.json")).unwrap() == locator,
            eq(true)
        );
        assert_that!(fs::read(&paths.catalog).unwrap() == catalog, eq(true));
        let completed = reconcile_authority_operation(&paths, second_id).unwrap();
        assert_that!(completed.operation_id, eq(second_id));
        assert_that!(completed.checkpoint.as_str(), eq("completed"));
        let authority = inspect_authority(&paths).unwrap();
        assert_that!(authority.recovery_history.len(), eq(4));
        assert_that!(
            authority
                .recovery_history
                .starts_with(&pending.recovery_history),
            eq(true)
        );
    }

    #[googletest::test]
    fn explicit_unclean_foreign_identity_repair_preserves_events_and_pending_authority() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let interrupted = std::panic::catch_unwind(|| {
            initialize_observed(&paths, |checkpoint| {
                if checkpoint == InitializationCheckpoint::IntentRecorded {
                    panic!("interrupt before catalog creation");
                }
            })
            .unwrap();
        });
        assert_that!(interrupted.is_err(), eq(true));
        let intended = inspect_authority(&paths).unwrap().catalog_id;
        let foreign = Store::create(&paths.catalog).unwrap().catalog_id;
        let events = Store::events_read_only(&paths.catalog, foreign).unwrap();
        crash_catalog_writer(&paths.catalog);
        assert_that!(
            matches!(
                redb::ReadOnlyDatabase::open(&paths.catalog),
                Err(redb::DatabaseError::RepairAborted)
            ),
            eq(true)
        );
        let unclean = fs::read(&paths.catalog).unwrap();
        let preview = inspect_authority(&paths).unwrap();
        assert_that!(preview.catalog_id, eq(intended));
        assert_that!(preview.store_state, eq("unreadable"));
        assert_that!(fs::read(&paths.catalog).unwrap() == unclean, eq(true));
        assert_that!(
            matches!(reconcile_authority(&paths), Err(PoolError::Conflict)),
            eq(true)
        );
        assert_that!(
            Store::events_read_only(&paths.catalog, foreign).unwrap(),
            eq(&events)
        );
        let pending = inspect_authority(&paths).unwrap();
        assert_that!(pending.catalog_id, eq(intended));
        assert_that!(pending.phase.as_str(), eq("initializing"));
        assert_that!(
            pending.recovery_checkpoint.as_deref(),
            eq(Some("intent_recorded"))
        );
        assert_that!(pending.recovery_history.len(), eq(1));
        assert_that!(matches!(open(&paths), Err(PoolError::Pending)), eq(true));
        let locator = fs::read(paths.state.join("active.json")).unwrap();
        let repaired = fs::read(&paths.catalog).unwrap();
        assert_that!(
            matches!(reconcile_authority(&paths), Err(PoolError::Conflict)),
            eq(true)
        );
        assert_that!(
            fs::read(paths.state.join("active.json")).unwrap() == locator,
            eq(true)
        );
        assert_that!(fs::read(&paths.catalog).unwrap() == repaired, eq(true));
    }

    #[googletest::test]
    fn historical_single_locator_checkpoint_is_retained_without_invented_intent() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let interrupted = std::panic::catch_unwind(|| {
            initialize_observed(&paths, |checkpoint| {
                if checkpoint == InitializationCheckpoint::IntentRecorded {
                    panic!("interrupt bootstrap");
                }
            })
            .unwrap();
        });
        assert_that!(interrupted.is_err(), eq(true));
        let first_id = reconcile_authority(&paths)
            .unwrap()
            .recovery_operation_id
            .unwrap();
        let locator_path = paths.state.join("active.json");
        let mut historical: serde_json::Value =
            serde_json::from_slice(&fs::read(&locator_path).unwrap()).unwrap();
        historical
            .as_object_mut()
            .unwrap()
            .remove("recovery_history");
        historical["recovery"]
            .as_object_mut()
            .unwrap()
            .remove("kind");
        fs::write(&locator_path, serde_json::to_vec(&historical).unwrap()).unwrap();
        let legacy = inspect_authority(&paths).unwrap().recovery_history;
        assert_that!(legacy.len(), eq(1));
        assert_that!(legacy[0].legacy, eq(true));
        assert_that!(legacy[0].operation_id, eq(first_id));
        assert_that!(legacy[0].checkpoint.as_str(), eq("completed"));
        crash_catalog_writer(&paths.catalog);
        let second = reconcile_authority(&paths).unwrap();
        let second_id = second.recovery_operation_id.unwrap();
        assert_that!(second.recovery_history.len(), eq(3));
        assert_that!(second.recovery_history.starts_with(&legacy), eq(true));
        assert_that!(
            inspect_authority_operation(&paths, first_id).unwrap(),
            eq(&legacy[0])
        );
        let operations = list_authority_operations(&paths).unwrap();
        assert_that!(
            operations
                .into_iter()
                .map(|fact| fact.operation_id)
                .collect::<Vec<_>>(),
            eq(&vec![first_id, second_id])
        );
        let locator = fs::read(&locator_path).unwrap();
        let catalog = fs::read(&paths.catalog).unwrap();
        assert_that!(
            reconcile_authority_operation(&paths, first_id).unwrap(),
            eq(&legacy[0])
        );
        assert_that!(fs::read(&locator_path).unwrap() == locator, eq(true));
        assert_that!(fs::read(&paths.catalog).unwrap() == catalog, eq(true));
    }

    #[googletest::test]
    fn unsupported_or_gapped_locator_history_never_authorizes_database_repair() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        initialize(&paths).unwrap();
        crash_catalog_writer(&paths.catalog);
        reconcile_authority(&paths).unwrap();
        let locator_path = paths.state.join("active.json");
        let original: serde_json::Value =
            serde_json::from_slice(&fs::read(&locator_path).unwrap()).unwrap();
        crash_catalog_writer(&paths.catalog);
        for field in ["schema_version", "position"] {
            let mut invalid = original.clone();
            invalid["recovery_history"][0][field] = serde_json::json!(2);
            fs::write(&locator_path, serde_json::to_vec(&invalid).unwrap()).unwrap();
            let locator = fs::read(&locator_path).unwrap();
            let catalog = fs::read(&paths.catalog).unwrap();
            let preview = inspect_authority(&paths);
            let apply = reconcile_authority(&paths);
            if field == "schema_version" {
                assert_that!(matches!(preview, Err(PoolError::Unsupported)), eq(true));
                assert_that!(matches!(apply, Err(PoolError::Unsupported)), eq(true));
            } else {
                assert_that!(matches!(preview, Err(PoolError::Corrupt)), eq(true));
                assert_that!(matches!(apply, Err(PoolError::Corrupt)), eq(true));
            }
            assert_that!(fs::read(&locator_path).unwrap() == locator, eq(true));
            assert_that!(fs::read(&paths.catalog).unwrap() == catalog, eq(true));
        }
    }
    #[googletest::test]
    fn explicit_storage_repair_can_retain_derived_damage_until_separate_rebuild() {
        use redb::{ReadableTable, TableDefinition};
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let initialized = initialize(&paths).unwrap();
        let history = Store::events_read_only(&paths.catalog, initialized.catalog_id).unwrap();
        {
            let database = redb::Database::open(&paths.catalog).unwrap();
            let transaction = database.begin_write().unwrap();
            {
                let mut table = transaction
                    .open_table(TableDefinition::<&str, &str>::new("state"))
                    .unwrap();
                let mut state: serde_json::Value =
                    serde_json::from_str(table.get("catalog").unwrap().unwrap().value()).unwrap();
                state["revision"] = serde_json::json!(999);
                table
                    .insert("catalog", serde_json::to_string(&state).unwrap().as_str())
                    .unwrap();
            }
            transaction.commit().unwrap();
        }
        crash_catalog_writer(&paths.catalog);
        assert_that!(
            matches!(
                redb::ReadOnlyDatabase::open(&paths.catalog),
                Err(redb::DatabaseError::RepairAborted)
            ),
            eq(true)
        );
        let bytes = fs::read(&paths.catalog).unwrap();
        assert_that!(super::rebuild(&paths).is_err(), eq(true));
        assert_that!(fs::read(&paths.catalog).unwrap(), eq(&bytes));
        let repaired = reconcile_authority(&paths).unwrap();
        assert_that!(repaired.catalog_id, eq(initialized.catalog_id));
        assert_that!(repaired.store_state, eq("rebuild_required"));
        assert_that!(super::inspect_projection(&paths).is_err(), eq(true));
        let restored = super::rebuild(&paths).unwrap();
        assert_that!(restored, eq(&initialized));
        assert_that!(
            Store::events_read_only(&paths.catalog, initialized.catalog_id).unwrap(),
            eq(&history)
        );
        assert_that!(
            super::inspect_authority(&paths).unwrap().store_state,
            eq("validated")
        );
    }
    #[googletest::test]
    fn relocated_storage_repair_keeps_rebuild_required_barrier_and_exact_historical_receipt() {
        use redb::TableDefinition;
        let root = tempfile::tempdir().unwrap();
        let source = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let initialized = initialize(&source).unwrap();
        let history = Store::events_read_only(&source.catalog, initialized.catalog_id).unwrap();
        let source_bytes = fs::read(&source.catalog).unwrap();
        let moved = crate::relocation::relocate(&source, &root.path().join("relocated")).unwrap();
        let paths = Paths {
            catalog: moved.destination.to_path().unwrap(),
            ..source.clone()
        };
        {
            let database = redb::Database::open(&paths.catalog).unwrap();
            let transaction = database.begin_write().unwrap();
            transaction
                .open_table(TableDefinition::<&str, &str>::new("state"))
                .unwrap()
                .insert("catalog", "damaged derived state")
                .unwrap();
            transaction.commit().unwrap();
        }
        crash_catalog_writer(&paths.catalog);
        let repaired = reconcile_authority(&paths).unwrap();
        assert_that!(repaired.store_state, eq("rebuild_required"));
        assert_that!(repaired.mutations_available(), eq(false));
        let bytes = fs::read(&paths.catalog).unwrap();
        let locator = fs::read(paths.state.join("active.json")).unwrap();
        assert_that!(super::open(&paths).is_err(), eq(true));
        assert_that!(
            crate::relocation::relocate(&paths, &root.path().join("forbidden")).is_err(),
            eq(true)
        );
        assert_that!(root.path().join("forbidden").exists(), eq(false));
        let receipt = crate::relocation::known(
            &paths,
            "recover preview",
            &moved.operation_id.to_string(),
            false,
            false,
        )
        .unwrap();
        assert_that!(
            receipt.data["catalog_mutations_available"].as_bool(),
            eq(Some(false))
        );
        assert_that!(
            receipt.data["next_action"]
                .as_str()
                .is_some_and(|action| action.contains("catalog rebuild")),
            eq(true)
        );
        assert_that!(
            crate::relocation::resume(&paths, moved.operation_id).unwrap(),
            eq(&moved)
        );
        assert_that!(fs::read(&paths.catalog).unwrap(), eq(&bytes));
        assert_that!(
            fs::read(paths.state.join("active.json")).unwrap(),
            eq(&locator)
        );
        assert_that!(super::rebuild(&paths).unwrap(), eq(&initialized));
        assert_that!(
            Store::events_read_only(&paths.catalog, initialized.catalog_id).unwrap(),
            eq(&history)
        );
        assert_that!(fs::read(&source.catalog).unwrap(), eq(&source_bytes));
        assert_that!(
            super::inspect_authority(&paths)
                .unwrap()
                .mutations_available(),
            eq(true)
        );
    }

    #[googletest::test]
    fn bootstrap_storage_repair_still_requires_a_complete_valid_projection() {
        use redb::TableDefinition;
        let root = tempfile::tempdir().unwrap();
        let paths = Paths {
            catalog: root.path().join("data/catalog.redb"),
            state: root.path().join("state"),
            json: true,
        };
        let interrupted = std::panic::catch_unwind(|| {
            initialize_observed(&paths, |checkpoint| {
                if checkpoint == InitializationCheckpoint::StoreCommitted {
                    panic!("interrupt bootstrap");
                }
            })
            .unwrap()
        });
        assert_that!(interrupted.is_err(), eq(true));
        {
            let database = redb::Database::open(&paths.catalog).unwrap();
            let transaction = database.begin_write().unwrap();
            transaction
                .open_table(TableDefinition::<&str, &str>::new("state"))
                .unwrap()
                .insert("catalog", "malformed derived state")
                .unwrap();
            transaction.commit().unwrap();
        }
        crash_catalog_writer(&paths.catalog);
        assert_that!(
            matches!(
                redb::ReadOnlyDatabase::open(&paths.catalog),
                Err(redb::DatabaseError::RepairAborted)
            ),
            eq(true)
        );
        assert_that!(reconcile_authority(&paths).is_err(), eq(true));
        assert_that!(
            inspect_authority(&paths).unwrap().phase.as_str(),
            eq("initializing")
        );
        assert_that!(super::rebuild(&paths).is_err(), eq(true));
    }
}
