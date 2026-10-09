//! Same-machine catalog relocation. The stable locator remains the sole authority.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    catalog::{self, Locator},
    cli::Envelope,
    coordination::{LockGuard, private_directory, private_file},
    domain::{CatalogId, CatalogProjection},
    error::PoolError,
    management::OperationId,
    paths::{EncodedPath, Paths},
    store::Store,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Checkpoint {
    Intended,
    Prepared,
    Switched,
    Completed,
}
impl Checkpoint {
    const fn next(self) -> Option<Self> {
        match self {
            Self::Intended => Some(Self::Prepared),
            Self::Prepared => Some(Self::Switched),
            Self::Switched => Some(Self::Completed),
            Self::Completed => None,
        }
    }
}
/// Content evidence fixes the source bytes selected by a durable relocation intent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "algorithm",
    content = "digest",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ContentEvidence {
    Sha256([u8; 32]),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relocation {
    pub operation_id: OperationId,
    pub catalog_id: CatalogId,
    pub source: EncodedPath,
    pub destination: EncodedPath,
    pub source_revision: u64,
    pub source_content: ContentEvidence,
    pub checkpoint: Checkpoint,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelocationFact {
    pub schema_version: u32,
    pub position: u64,
    pub relocation: Relocation,
}
/// Observations occur after actual effects or durable journal replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelocationCheckpoint {
    IntentRecorded,
    DirectoryPrepared,
    DestinationCreated,
    CopyStarted,
    CopySynced,
    DestinationPrepared,
    LocatorSwitched,
    Completed,
}

pub(crate) fn pending(locator: &Locator) -> bool {
    locator
        .relocation
        .as_ref()
        .is_some_and(|r| r.checkpoint != Checkpoint::Completed)
}

pub(crate) fn validate(locator: &Locator) -> Result<(), PoolError> {
    let mut previous: Option<&Relocation> = None;
    let mut ids = std::collections::HashSet::new();
    for (index, fact) in locator.relocation_history.iter().enumerate() {
        if fact.schema_version != 1 {
            return Err(PoolError::Unsupported);
        }
        let current = &fact.relocation;
        if fact.position != u64::try_from(index).map_err(|_| PoolError::Corrupt)? + 1
            || current.catalog_id != locator.catalog_id
            || current.source_revision == 0
            || current.source.to_path()? == current.destination.to_path()?
            || locator.recovery_identity_used(current.operation_id)
        {
            return Err(PoolError::Corrupt);
        }
        if let Some(prior) = previous {
            if prior.operation_id == current.operation_id {
                let mut expected = prior.clone();
                expected.checkpoint = prior.checkpoint.next().ok_or(PoolError::Corrupt)?;
                if expected != *current {
                    return Err(PoolError::Corrupt);
                }
            } else if prior.checkpoint != Checkpoint::Completed
                || current.checkpoint != Checkpoint::Intended
                || prior.destination != current.source
                || !ids.insert(current.operation_id.to_string())
            {
                return Err(PoolError::Corrupt);
            }
        } else if current.checkpoint != Checkpoint::Intended
            || !ids.insert(current.operation_id.to_string())
        {
            return Err(PoolError::Corrupt);
        }
        previous = Some(current);
    }
    if previous != locator.relocation.as_ref() {
        return Err(PoolError::Corrupt);
    }
    if let Some(current) = previous {
        let expected = if matches!(
            current.checkpoint,
            Checkpoint::Intended | Checkpoint::Prepared
        ) {
            &current.source
        } else {
            &current.destination
        };
        if &locator.catalog_path != expected
            || locator.phase != "active"
            || (current.checkpoint != Checkpoint::Completed && locator.recovery_pending())
        {
            return Err(PoolError::Corrupt);
        }
    }
    Ok(())
}

pub(crate) fn validate_operations(
    locator: &Locator,
    projection: &CatalogProjection,
) -> Result<(), PoolError> {
    if locator
        .relocation_history
        .iter()
        .any(|fact| projection.operation_identity_used(fact.relocation.operation_id))
    {
        return Err(PoolError::Corrupt);
    }
    Ok(())
}

fn append(paths: &Paths, locator: &mut Locator, operation: Relocation) -> Result<(), PoolError> {
    locator.relocation_history.push(RelocationFact {
        schema_version: 1,
        position: u64::try_from(locator.relocation_history.len())
            .map_err(|_| PoolError::Corrupt)?
            + 1,
        relocation: operation.clone(),
    });
    locator.relocation = Some(operation);
    validate(locator)?;
    catalog::replace_locator(paths, locator)
}

fn unresolved(state: &CatalogProjection) -> bool {
    state
        .repositories
        .iter()
        .any(|r| state.has_pending_recovery(r.repository_id))
        || state.operations.iter().any(|o| o.state.is_pending())
        || state.acquisitions.iter().any(|o| o.state.is_pending())
        || state.creations.iter().any(|o| o.state.is_pending())
        || state.releases.iter().any(|o| o.state.is_pending())
        || state
            .recoveries
            .iter()
            .any(|o| o.state != crate::recovery::RecoveryState::Completed)
        || state
            .repository_recoveries
            .iter()
            .any(|o| o.state != crate::repository_recovery::RepositoryRecoveryState::Completed)
}

fn destination(
    paths: &Paths,
    selected: &Path,
    state: &CatalogProjection,
) -> Result<PathBuf, PoolError> {
    if selected.as_os_str().is_empty()
        || selected
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(PoolError::Configuration);
    }
    let directory = if selected.is_absolute() {
        selected.to_path_buf()
    } else {
        std::env::current_dir()?.join(selected)
    };
    let parent = directory.parent().ok_or(PoolError::Configuration)?;
    if fs::canonicalize(parent)? != parent || fs::symlink_metadata(&directory).is_ok() {
        return Err(PoolError::Conflict);
    }
    let source = paths.catalog.parent().ok_or(PoolError::Configuration)?;
    if directory.starts_with(source)
        || source.starts_with(&directory)
        || directory.starts_with(&paths.state)
        || paths.state.starts_with(&directory)
        || state.worktrees.iter().any(|w| {
            w.path
                .to_path()
                .is_ok_and(|p| directory.starts_with(&p) || p.starts_with(&directory))
        })
        || state.repositories.iter().any(|r| {
            r.common_directory
                .to_path()
                .is_ok_and(|p| directory.starts_with(&p) || p.starts_with(&directory))
        })
    {
        return Err(PoolError::Conflict);
    }
    Ok(directory.join("catalog.redb"))
}

/// Explicitly relocates storage, retaining all checkout paths and handles.
/// # Errors
/// Refuses conflicting authority, unresolved operations, unsafe destinations or uncertain effects.
pub fn relocate(paths: &Paths, selected: &Path) -> Result<Relocation, PoolError> {
    relocate_observed(paths, selected, |_| {})
}
/// Executes the production workflow with observations for deterministic interruption.
/// # Errors
/// Refuses unsafe state without adopting or overwriting a destination.
pub fn relocate_observed(
    paths: &Paths,
    selected: &Path,
    mut observe: impl FnMut(RelocationCheckpoint),
) -> Result<Relocation, PoolError> {
    let _maintenance = LockGuard::acquire(&paths.state.join("maintenance.lock"), true)?;
    let _catalog = LockGuard::acquire(&paths.state.join("catalog.lock"), true)?;
    let mut locator = catalog::read_locator(paths)?;
    if pending(&locator) {
        return Err(PoolError::RelocationPending);
    }
    if locator.phase != "active" || locator.recovery_pending() {
        return Err(PoolError::Pending);
    }
    private_file(&paths.catalog)?;
    let projection = Store::inspect_read_only(&paths.catalog, locator.catalog_id)?;
    validate_operations(&locator, &projection)?;
    if unresolved(&projection) {
        return Err(PoolError::OperationPending);
    }
    let target = destination(paths, selected, &projection)?;
    let id = OperationId::new();
    if projection.operation_identity_used(id)
        || locator.recovery_identity_used(id)
        || locator
            .relocation_history
            .iter()
            .any(|f| f.relocation.operation_id == id)
    {
        return Err(PoolError::Corrupt);
    }
    let operation = Relocation {
        operation_id: id,
        catalog_id: locator.catalog_id,
        source: locator.catalog_path.clone(),
        destination: EncodedPath::from_path(&target),
        source_revision: projection.revision,
        source_content: content_evidence(&paths.catalog)?,
        checkpoint: Checkpoint::Intended,
    };
    append(paths, &mut locator, operation)?;
    observe(RelocationCheckpoint::IntentRecorded);
    finish(paths, &mut locator, &mut observe)
}

/// Resumes only the exact recorded relocation; historical completion is a no-op.
/// # Errors
/// Refuses unknown identities, incomplete/foreign copies or conflicting authority.
pub fn resume(paths: &Paths, operation_id: OperationId) -> Result<Relocation, PoolError> {
    resume_observed(paths, operation_id, |_| {})
}
/// Resumes the actual workflow with checkpoint observations.
/// # Errors
/// Refuses ambiguity instead of overwriting, resetting or choosing another operation.
pub fn resume_observed(
    paths: &Paths,
    operation_id: OperationId,
    mut observe: impl FnMut(RelocationCheckpoint),
) -> Result<Relocation, PoolError> {
    let _maintenance = LockGuard::acquire(&paths.state.join("maintenance.lock"), true)?;
    let _catalog = LockGuard::acquire(&paths.state.join("catalog.lock"), true)?;
    let mut locator = catalog::read_active_locator(paths)?;
    let operation = locator
        .relocation_history
        .iter()
        .rev()
        .find(|f| f.relocation.operation_id == operation_id)
        .map(|f| f.relocation.clone())
        .ok_or(PoolError::Unregistered)?;
    if operation.checkpoint == Checkpoint::Completed {
        return Ok(operation);
    }
    if locator.relocation.as_ref() != Some(&operation)
        || ![&operation.source, &operation.destination]
            .iter()
            .any(|p| p.to_path().is_ok_and(|p| p == paths.catalog))
    {
        return Err(PoolError::Conflict);
    }
    finish(paths, &mut locator, &mut observe)
}

fn content_evidence(path: &Path) -> Result<ContentEvidence, PoolError> {
    let mut input = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            return Ok(ContentEvidence::Sha256(digest.finalize().into()));
        }
        digest.update(&buffer[..count]);
    }
}

fn same_bytes(source: &Path, target: &Path) -> Result<bool, PoolError> {
    let mut left = File::open(source)?;
    let mut right = File::open(target)?;
    if left.metadata()?.len() != right.metadata()?.len() {
        return Ok(false);
    }
    let mut a = [0u8; 65536];
    let mut b = [0u8; 65536];
    loop {
        let n = left.read(&mut a)?;
        right.read_exact(&mut b[..n])?;
        if a[..n] != b[..n] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

fn validate_copy(operation: &Relocation) -> Result<(), PoolError> {
    let source = operation.source.to_path()?;
    let target = operation.destination.to_path()?;
    private_file(&source)?;
    private_file(&target)?;
    let original = Store::inspect_read_only(&source, operation.catalog_id)?;
    let copy = Store::inspect_read_only(&target, operation.catalog_id)?;
    if original.revision != operation.source_revision
        || content_evidence(&source)? != operation.source_content
        || content_evidence(&target)? != operation.source_content
        || original != copy
        || unresolved(&copy)
        || !same_bytes(&source, &target)?
    {
        return Err(PoolError::Conflict);
    }
    Ok(())
}

fn prepare(
    operation: &Relocation,
    observe: &mut impl FnMut(RelocationCheckpoint),
) -> Result<(), PoolError> {
    let source = operation.source.to_path()?;
    let target = operation.destination.to_path()?;
    let directory = target.parent().ok_or(PoolError::Corrupt)?;
    match fs::symlink_metadata(directory) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(PoolError::Conflict);
            }
            private_directory(directory)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new().mode(0o700).create(directory)?;
            File::open(directory.parent().ok_or(PoolError::Corrupt)?)?.sync_all()?;
        }
        Err(e) => return Err(e.into()),
    }
    if fs::metadata(directory)?.uid() != fs::metadata("/proc/self")?.uid() {
        return Err(PoolError::Permissions);
    }
    for entry in fs::read_dir(directory)? {
        if entry?.path() != target {
            return Err(PoolError::Conflict);
        }
    }
    observe(RelocationCheckpoint::DirectoryPrepared);
    match fs::symlink_metadata(&target) {
        Ok(_) => validate_copy(operation)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let projection = Store::inspect_read_only(&source, operation.catalog_id)?;
            if projection.revision != operation.source_revision || unresolved(&projection) {
                return Err(PoolError::Conflict);
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&target)?;
            File::open(directory)?.sync_all()?;
            observe(RelocationCheckpoint::DestinationCreated);
            let mut input = File::open(&source)?;
            let mut buffer = [0u8; 65536];
            let n = input.read(&mut buffer)?;
            output.write_all(&buffer[..n])?;
            observe(RelocationCheckpoint::CopyStarted);
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            drop(output);
            File::open(directory)?.sync_all()?;
            observe(RelocationCheckpoint::CopySynced);
            validate_copy(operation)?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn finish(
    paths: &Paths,
    locator: &mut Locator,
    observe: &mut impl FnMut(RelocationCheckpoint),
) -> Result<Relocation, PoolError> {
    let mut operation = locator.relocation.clone().ok_or(PoolError::Corrupt)?;
    let source = Store::inspect_read_only(&operation.source.to_path()?, operation.catalog_id)?;
    validate_operations(locator, &source)?;
    if source.revision != operation.source_revision
        || content_evidence(&operation.source.to_path()?)? != operation.source_content
        || unresolved(&source)
    {
        return Err(PoolError::Conflict);
    }
    if operation.checkpoint == Checkpoint::Intended {
        prepare(&operation, observe)?;
        operation.checkpoint = Checkpoint::Prepared;
        append(paths, locator, operation.clone())?;
        observe(RelocationCheckpoint::DestinationPrepared);
    }
    if operation.checkpoint == Checkpoint::Prepared {
        validate_copy(&operation)?;
        locator.catalog_path = operation.destination.clone();
        operation.checkpoint = Checkpoint::Switched;
        append(paths, locator, operation.clone())?;
        observe(RelocationCheckpoint::LocatorSwitched);
    }
    if operation.checkpoint == Checkpoint::Switched {
        validate_copy(&operation)?;
        operation.checkpoint = Checkpoint::Completed;
        append(paths, locator, operation.clone())?;
        observe(RelocationCheckpoint::Completed);
    }
    Ok(operation)
}

/// A stable-locator receipt: retained old storage is inactive, and checkout paths never move.
#[must_use]
pub fn operation_value(operation: &Relocation) -> Value {
    let mut value = json!(operation);
    value["scope"] = json!("catalog");
    value["kind"] = json!("relocation");
    value["state"] = json!(if operation.checkpoint == Checkpoint::Completed {
        "completed"
    } else {
        "pending"
    });
    value["last_checkpoint"] = json!(operation.checkpoint);
    value["journal"] = json!("active_catalog_locator");
    value
}

fn receipt(
    paths: &Paths,
    command: &str,
    operation: Option<&Relocation>,
    error: Option<&PoolError>,
) -> Envelope {
    let authority = catalog::inspect_active_authority(paths).ok();
    let mut envelope = if let Some(error) = error {
        Envelope::failure(command.into(), error)
    } else {
        Envelope {
            schema_version: 1,
            command: command.into(),
            outcome: "completed",
            reason_code: "catalog_relocation_observed",
            context: json!({}),
            data: json!({}),
            warnings: Vec::new(),
        }
    };
    envelope.context = json!({"catalog_id": authority.as_ref().map(|a| a.catalog_id), "catalog_path": authority.as_ref().map(|a| &a.catalog_path), "selected_catalog_path": EncodedPath::from_path(&paths.catalog), "operation_id": operation.map(|o| o.operation_id), "repository_id": null, "worktree_id": null, "assignment_handle": null});
    envelope.data = json!({"authority": authority, "operation": operation.map(operation_value), "worktrees_moved": false, "retained_source": operation.map(|o| &o.source), "retained_source_bytes": operation.and_then(|o| o.source.to_path().ok()).and_then(|p| fs::metadata(p).ok()).map(|m| m.len()), "retained_source_active": operation.is_some_and(|o| matches!(o.checkpoint, Checkpoint::Intended | Checkpoint::Prepared)), "catalog_mutations_available": authority.as_ref().is_some_and(catalog::AuthorityObservation::mutations_available), "next_action": next_action(operation)});
    envelope
}
pub(crate) fn next_action(operation: Option<&Relocation>) -> String {
    match operation {
        Some(operation) if operation.checkpoint == Checkpoint::Intended => format!("inspect relocation {} and verify its source against recorded SHA-256 evidence; if source bytes are missing or changed, preserve conflicting bytes and manually restore the exact recorded source first; run recover apply --operation {}; if the destination copy is incomplete or conflicting, preserve it outside the destination manually before retrying this exact ID; no overwrite or reset is performed", operation.operation_id, operation.operation_id),
        Some(operation) if operation.checkpoint != Checkpoint::Completed => format!("inspect relocation {}; keep mutations stopped, verify both copies against recorded SHA-256 evidence and restore exact source or destination bytes manually if missing or changed, preserving conflicting bytes first; then run recover apply --operation {}", operation.operation_id, operation.operation_id),
        Some(_) => "select the recorded active catalog directory explicitly; old catalog storage is retained inactive and consumes disk; worktrees remain at their original paths; configuration was not rewritten".into(),
        None => "use the explicitly selected active catalog location; worktree paths are unchanged; configuration was not rewritten".into(),
    }
}

/// Composes the explicit relocate command through the concrete workflow.
#[must_use]
pub fn command(paths: &Paths, destination: &Path) -> Envelope {
    match relocate(paths, destination) {
        Ok(operation) => {
            let mut result = receipt(paths, "catalog relocate", Some(&operation), None);
            result.reason_code = "catalog_relocated";
            result
        }
        Err(error) => {
            let current = catalog::inspect_active_authority(paths)
                .ok()
                .and_then(|a| a.relocation);
            receipt(paths, "catalog relocate", current.as_ref(), Some(&error))
        }
    }
}

/// Handles exact relocation operation selection; other identities are left to their owner.
#[must_use]
pub fn known(
    paths: &Paths,
    command: &str,
    selector: &str,
    apply: bool,
    conflicting_scope: bool,
) -> Option<Envelope> {
    let id = selector.parse::<OperationId>().ok()?;
    let authority = catalog::inspect_active_authority(paths).ok()?;
    let operation = authority
        .relocation_history
        .iter()
        .rev()
        .find(|f| f.relocation.operation_id == id)?
        .relocation
        .clone();
    if conflicting_scope {
        return Some(receipt(
            paths,
            command,
            Some(&operation),
            Some(&PoolError::Selectors),
        ));
    }
    if !apply {
        return Some(receipt(paths, command, Some(&operation), None));
    }
    Some(match resume(paths, id) {
        Ok(result) => {
            let mut envelope = receipt(paths, command, Some(&result), None);
            envelope.reason_code = if operation.checkpoint == Checkpoint::Completed {
                "catalog_relocation_already_completed"
            } else {
                "catalog_relocation_completed"
            };
            envelope
        }
        Err(error) => receipt(paths, command, Some(&operation), Some(&error)),
    })
}

/// Catalog scope resumes a pending relocation, never guesses a repository operation.
#[must_use]
pub fn current(
    paths: &Paths,
    command: &str,
    apply: bool,
    conflicting_scope: bool,
) -> Option<Envelope> {
    let authority = catalog::inspect_active_authority(paths).ok()?;
    let operation = authority
        .relocation
        .filter(|r| r.checkpoint != Checkpoint::Completed)?;
    known(
        paths,
        command,
        &operation.operation_id.to_string(),
        apply,
        conflicting_scope,
    )
}
