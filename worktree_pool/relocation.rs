//! Same-machine relocation: semantic `CloudEvents` with stable physical coordination.
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
    domain::{CatalogId, CatalogProjection, management_event},
    error::PoolError,
    management::{ManagementEvent, OperationId},
    paths::{EncodedPath, Paths},
    relocation_events::{RelocationEvent, RelocationIntent, RelocationState, unresolved},
    store::Store,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Checkpoint {
    Starting,
    Intended,
    Prepared,
    Switched,
    Committing,
    Completed,
}
impl Checkpoint {
    const fn next(self) -> Option<Self> {
        match self {
            Self::Starting => Some(Self::Intended),
            Self::Intended => Some(Self::Prepared),
            Self::Prepared => Some(Self::Switched),
            Self::Switched => Some(Self::Committing),
            Self::Committing => Some(Self::Completed),
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
    /// Recorded post-Started measurement; never live size or authority evidence.
    #[serde(default)]
    pub source_bytes: Option<u64>,
    pub checkpoint: Checkpoint,
    #[serde(default)]
    pub intent_event: Option<Value>,
    #[serde(default)]
    pub intent_record: Option<String>,
    #[serde(default)]
    pub completion_event: Option<Value>,
    #[serde(default)]
    pub completion_record: Option<String>,
    #[serde(default)]
    pub prefix_content: Option<ContentEvidence>,
    #[serde(default)]
    pub destination_content: Option<ContentEvidence>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelocationFact {
    pub schema_version: u32,
    pub position: u64,
    pub relocation: Relocation,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryProof {
    pub source_revision: u64,
    pub prefix_content: ContentEvidence,
    pub started: Value,
    #[serde(default)]
    pub started_record: String,
    pub completed: Option<Value>,
    #[serde(default)]
    pub completed_record: Option<String>,
    pub allow_uncommitted_start: bool,
}
pub(crate) fn history_proof(
    operation: &Relocation,
    selected: &Path,
) -> Result<HistoryProof, PoolError> {
    Ok(HistoryProof {
        source_revision: operation.source_revision,
        prefix_content: operation.prefix_content.clone().ok_or(PoolError::Corrupt)?,
        started: operation.intent_event.clone().ok_or(PoolError::Corrupt)?,
        started_record: operation.intent_record.clone().ok_or(PoolError::Corrupt)?,
        completed: if operation.destination.to_path()? == selected {
            operation.completion_event.clone()
        } else {
            None
        },
        completed_record: if operation.destination.to_path()? == selected {
            operation.completion_record.clone()
        } else {
            None
        },
        allow_uncommitted_start: operation.checkpoint == Checkpoint::Starting,
    })
}
pub(crate) fn validate_history(
    proof: &HistoryProof,
    events: &[Value],
    records: &[String],
) -> Result<(), PoolError> {
    if records.len() != events.len() {
        return Err(PoolError::Conflict);
    }
    let count = events.len() as u64;
    let prefix =
        if proof.allow_uncommitted_start && count.checked_add(1) == Some(proof.source_revision) {
            events
        } else if count == proof.source_revision
            && events.last() == Some(&proof.started)
            && records.last() == Some(&proof.started_record)
        {
            &events[..events.len() - 1]
        } else if Some(count) == proof.source_revision.checked_add(1)
            && proof.completed.as_ref() == events.last()
            && events.get(events.len() - 2) == Some(&proof.started)
            && records.get(records.len() - 2) == Some(&proof.started_record)
            && proof.completed_record.as_ref() == records.last()
        {
            &events[..events.len() - 2]
        } else {
            return Err(PoolError::Conflict);
        };
    if records_evidence(&records[..prefix.len()])? != proof.prefix_content {
        return Err(PoolError::Conflict);
    }
    Ok(())
}
fn expected_content(
    locator: &Locator,
    operation: &Relocation,
    selected: &Path,
) -> Result<ContentEvidence, PoolError> {
    if let Some(content) = locator
        .recovery_history
        .iter()
        .rev()
        .filter_map(|r| r.relocation_repair.as_ref())
        .find(|r| {
            r.operation_id == operation.operation_id
                && r.catalog_path.to_path().is_ok_and(|p| p == selected)
                && r.content_after.is_some()
        })
        .and_then(|r| r.content_after.clone())
    {
        return Ok(content);
    }
    if selected == operation.destination.to_path()? {
        Ok(operation
            .destination_content
            .clone()
            .unwrap_or(expected_content(
                locator,
                operation,
                &operation.source.to_path()?,
            )?))
    } else {
        Ok(operation.source_content.clone())
    }
}
fn inspect_for_resume(
    paths: &Paths,
    locator: &mut Locator,
    operation: &Relocation,
    selected: &Path,
    repair: bool,
    observe: &mut impl FnMut(RelocationCheckpoint),
) -> Result<(), PoolError> {
    if repair {
        catalog::repair_relocation_locked(
            paths,
            locator,
            operation.operation_id,
            selected,
            &history_proof(operation, selected)?,
            observe,
        )?;
    }
    Ok(())
}
/// Observations occur after actual effects or durable journal replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelocationCheckpoint {
    RepairIntentRecorded,
    RepairStoreValidated,
    RepairRebuilt,
    RepairCompleted,
    JournalRecorded,
    StartedCommitted,
    IntentRecorded,
    DirectoryPrepared,
    DestinationCreated,
    CopyStarted,
    CopySynced,
    DestinationPrepared,
    LocatorSwitched,
    CompletionRecorded,
    CompletionCommitted,
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
        if fact.schema_version != 3 {
            return Err(if fact.schema_version == 1 {
                PoolError::UnsupportedRelocation
            } else if fact.schema_version == 2 {
                PoolError::UnsupportedRelocationProof
            } else {
                PoolError::Unsupported
            });
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
                if prior.checkpoint == Checkpoint::Starting {
                    expected.source_content = current.source_content.clone();
                    expected.source_bytes = current.source_bytes;
                }
                if prior.checkpoint == Checkpoint::Intended {
                    expected
                        .destination_content
                        .clone_from(&current.destination_content);
                }
                if prior.checkpoint == Checkpoint::Switched {
                    expected
                        .completion_event
                        .clone_from(&current.completion_event);
                    expected
                        .completion_record
                        .clone_from(&current.completion_record);
                }
                if expected != *current {
                    return Err(PoolError::Corrupt);
                }
            } else if prior.checkpoint != Checkpoint::Completed
                || current.checkpoint != Checkpoint::Starting
                || prior.destination != current.source
                || !ids.insert(current.operation_id.to_string())
            {
                return Err(PoolError::Corrupt);
            }
        } else if current.checkpoint != Checkpoint::Starting
            || !ids.insert(current.operation_id.to_string())
        {
            return Err(PoolError::Corrupt);
        }
        validate_wire(current)?;
        previous = Some(current);
    }
    if previous != locator.relocation.as_ref() {
        return Err(PoolError::Corrupt);
    }
    if let Some(current) = previous {
        let expected = if matches!(
            current.checkpoint,
            Checkpoint::Starting | Checkpoint::Intended | Checkpoint::Prepared
        ) {
            &current.source
        } else {
            &current.destination
        };
        if &locator.catalog_path != expected
            || locator.phase != "active"
            || (current.checkpoint != Checkpoint::Completed
                && locator.recovery_pending()
                && !locator.relocation_repair_pending(current.operation_id))
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
    for fact in &locator.relocation_history {
        let operation = &fact.relocation;
        let recorded = projection
            .relocations
            .iter()
            .find(|r| r.intent.operation_id == operation.operation_id);
        if let Some(recorded) = recorded {
            if recorded.intent != intent(operation)
                || Some(recorded.intent_event_id.as_str())
                    != operation
                        .intent_event
                        .as_ref()
                        .and_then(|e| e["id"].as_str())
            {
                return Err(PoolError::Corrupt);
            }
        } else if projection.operation_identity_used(operation.operation_id)
            || operation.checkpoint != Checkpoint::Starting
        {
            return Err(PoolError::Corrupt);
        }
    }
    for recorded in &projection.relocations {
        let latest = locator
            .relocation_history
            .iter()
            .rev()
            .find(|f| f.relocation.operation_id == recorded.intent.operation_id);
        if let Some(latest) = latest {
            let operation = &latest.relocation;
            if operation.checkpoint == Checkpoint::Completed
                && recorded.state != RelocationState::Completed
                || recorded.state == RelocationState::Completed
                    && !matches!(
                        operation.checkpoint,
                        Checkpoint::Committing | Checkpoint::Completed
                    )
                || recorded.state == RelocationState::Completed
                    && recorded.completion_event_id.as_deref()
                        != operation
                            .completion_event
                            .as_ref()
                            .and_then(|e| e["id"].as_str())
            {
                return Err(PoolError::Corrupt);
            }
        } else if recorded.state == RelocationState::Completed {
            return Err(PoolError::Corrupt);
        }
    }
    Ok(())
}

fn intent(operation: &Relocation) -> RelocationIntent {
    RelocationIntent {
        operation_id: operation.operation_id,
        catalog_id: operation.catalog_id,
        source: operation.source.clone(),
        destination: operation.destination.clone(),
        source_revision: operation.source_revision,
    }
}
fn validate_record(
    operation: &Relocation,
    wire: Option<&String>,
    event: &Value,
    position: u64,
) -> Result<(), PoolError> {
    let wire = wire.ok_or(PoolError::Corrupt)?;
    let recorded: crate::rebuild::RecordedEvent =
        serde_json::from_str(wire).map_err(|_| PoolError::Corrupt)?;
    let typed = crate::domain::decode_event(event, operation.catalog_id)?;
    if recorded.position != position
        || recorded.event != *event
        || recorded.stream_id != Some(typed.stream_id(operation.catalog_id))
        || Store::encoded_record(&recorded)? != *wire
    {
        return Err(PoolError::Corrupt);
    }
    Ok(())
}

fn validate_wire(operation: &Relocation) -> Result<(), PoolError> {
    let event = operation.intent_event.as_ref().ok_or(PoolError::Corrupt)?;
    validate_record(
        operation,
        operation.intent_record.as_ref(),
        event,
        operation.source_revision,
    )?;
    let typed = crate::domain::decode_event(event, operation.catalog_id)?;
    if !matches!(typed, crate::domain::DomainEvent::Management { event: ref e, .. } if e.as_ref() == &ManagementEvent::Relocation(RelocationEvent::Started(intent(operation))))
        || operation.prefix_content.is_none()
    {
        return Err(PoolError::Corrupt);
    }
    if let Some(event) = &operation.completion_event {
        validate_record(
            operation,
            operation.completion_record.as_ref(),
            event,
            operation
                .source_revision
                .checked_add(1)
                .ok_or(PoolError::Corrupt)?,
        )?;
        let typed = crate::domain::decode_event(event, operation.catalog_id)?;
        if !matches!(typed, crate::domain::DomainEvent::Management { event: ref e, .. } if e.as_ref() == &ManagementEvent::Relocation(RelocationEvent::Completed { operation_id: operation.operation_id, catalog_id: operation.catalog_id }))
            || event["causationid"]
                != operation.intent_event.as_ref().ok_or(PoolError::Corrupt)?["id"]
            || !matches!(
                operation.checkpoint,
                Checkpoint::Committing | Checkpoint::Completed
            )
        {
            return Err(PoolError::Corrupt);
        }
    } else if operation.completion_record.is_some()
        || matches!(
            operation.checkpoint,
            Checkpoint::Committing | Checkpoint::Completed
        )
    {
        return Err(PoolError::Corrupt);
    }
    Ok(())
}

fn append(paths: &Paths, locator: &mut Locator, operation: Relocation) -> Result<(), PoolError> {
    locator.relocation_history.push(RelocationFact {
        schema_version: 3,
        position: u64::try_from(locator.relocation_history.len())
            .map_err(|_| PoolError::Corrupt)?
            + 1,
        relocation: operation.clone(),
    });
    locator.relocation = Some(operation);
    validate(locator)?;
    catalog::replace_locator(paths, locator)
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
    let mut operation = Relocation {
        operation_id: id,
        catalog_id: locator.catalog_id,
        source: locator.catalog_path.clone(),
        destination: EncodedPath::from_path(&target),
        source_revision: projection
            .revision
            .checked_add(1)
            .ok_or(PoolError::Corrupt)?,
        source_content: content_evidence(&paths.catalog)?,
        source_bytes: None,
        checkpoint: Checkpoint::Starting,
        intent_event: None,
        intent_record: None,
        completion_event: None,
        completion_record: None,
        destination_content: None,
        prefix_content: Some(records_evidence(&Store::raw_records_rebuild_history(
            &paths.catalog,
            locator.catalog_id,
        )?)?),
    };
    operation.intent_event = Some(management_event(
        locator.catalog_id,
        &ManagementEvent::Relocation(RelocationEvent::Started(intent(&operation))),
    )?);
    operation.intent_record = Some(Store::planned_record(
        &projection,
        operation.intent_event.as_ref().ok_or(PoolError::Corrupt)?,
    )?);
    append(paths, &mut locator, operation)?;
    observe(RelocationCheckpoint::JournalRecorded);
    finish(paths, &mut locator, &mut observe, false)
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
    finish(paths, &mut locator, &mut observe, true)
}

pub(crate) fn content_evidence(path: &Path) -> Result<ContentEvidence, PoolError> {
    let mut input = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; 65536];
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
    let mut a = vec![0u8; 65536];
    let mut b = vec![0u8; 65536];
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

fn validate_copy(locator: &Locator, operation: &Relocation) -> Result<(), PoolError> {
    let source = operation.source.to_path()?;
    let target = operation.destination.to_path()?;
    private_file(&source)?;
    private_file(&target)?;
    let original = Store::inspect_read_only(&source, operation.catalog_id)?;
    let copy = Store::inspect_read_only(&target, operation.catalog_id)?;
    if original.revision != operation.source_revision
        || content_evidence(&source)? != expected_content(locator, operation, &source)?
        || content_evidence(&target)? != expected_content(locator, operation, &target)?
        || original != copy
        || Store::events_read_only(&target, operation.catalog_id)? != source_history(operation)?
        || unresolved(&copy)
        || (content_evidence(&source)? == content_evidence(&target)?
            && !same_bytes(&source, &target)?)
    {
        return Err(PoolError::Conflict);
    }
    Ok(())
}

fn prepare(
    locator: &Locator,
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
        Ok(_) => validate_copy(locator, operation)?,
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
            let mut buffer = vec![0u8; 65536];
            let n = input.read(&mut buffer)?;
            output.write_all(&buffer[..n])?;
            observe(RelocationCheckpoint::CopyStarted);
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            drop(output);
            File::open(directory)?.sync_all()?;
            observe(RelocationCheckpoint::CopySynced);
            validate_copy(locator, operation)?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn records_evidence(records: &[String]) -> Result<ContentEvidence, PoolError> {
    Ok(ContentEvidence::Sha256(
        Sha256::digest(serde_json::to_vec(records).map_err(|_| PoolError::Corrupt)?).into(),
    ))
}
fn source_history(operation: &Relocation) -> Result<Vec<Value>, PoolError> {
    let source = operation.source.to_path()?;
    let events = Store::events_read_only(&source, operation.catalog_id)?;
    let records = Store::raw_records_rebuild_history(&source, operation.catalog_id)?;
    if events.len() as u64 != operation.source_revision {
        return Err(PoolError::Conflict);
    }
    validate_history(&history_proof(operation, &source)?, &events, &records)?;
    Ok(events)
}

fn start(
    paths: &Paths,
    locator: &mut Locator,
    observe: &mut impl FnMut(RelocationCheckpoint),
    repair: bool,
) -> Result<(), PoolError> {
    let mut operation = locator.relocation.clone().ok_or(PoolError::Corrupt)?;
    if operation.checkpoint != Checkpoint::Starting {
        return Ok(());
    }
    let source = operation.source.to_path()?;
    inspect_for_resume(paths, locator, &operation, &source, repair, observe)?;
    let events = Store::events_read_only(&source, operation.catalog_id)?;
    if events.len() as u64 == operation.source_revision - 1 {
        validate_history(
            &history_proof(&operation, &source)?,
            &events,
            &Store::raw_records_rebuild_history(&source, operation.catalog_id)?,
        )?;
        let current = Store::inspect_read_only(&source, operation.catalog_id)?;
        if Store::planned_record(
            &current,
            operation.intent_event.as_ref().ok_or(PoolError::Corrupt)?,
        )? != *operation.intent_record.as_ref().ok_or(PoolError::Corrupt)?
            || content_evidence(&source)? != expected_content(locator, &operation, &source)?
        {
            return Err(PoolError::Conflict);
        }
        let store = Store::open(&source, operation.catalog_id)?;
        store.append_batch(
            operation.source_revision - 1,
            vec![operation.intent_event.clone().ok_or(PoolError::Corrupt)?],
        )?;
        observe(RelocationCheckpoint::StartedCommitted);
        drop(store);
    }
    source_history(&operation)?;
    operation.source_content = content_evidence(&source)?;
    operation.source_bytes = Some(fs::metadata(&source)?.len());
    operation.checkpoint = Checkpoint::Intended;
    append(paths, locator, operation)?;
    observe(RelocationCheckpoint::IntentRecorded);
    Ok(())
}
fn complete(
    paths: &Paths,
    locator: &mut Locator,
    observe: &mut impl FnMut(RelocationCheckpoint),
    repair: bool,
) -> Result<Relocation, PoolError> {
    let mut operation = locator.relocation.clone().ok_or(PoolError::Corrupt)?;
    let source = source_history(&operation)?;
    if content_evidence(&operation.source.to_path()?)?
        != expected_content(locator, &operation, &operation.source.to_path()?)?
    {
        return Err(PoolError::Conflict);
    }
    let target = operation.destination.to_path()?;
    if operation.checkpoint == Checkpoint::Switched {
        validate_copy(locator, &operation)?;
        let mut event = management_event(
            operation.catalog_id,
            &ManagementEvent::Relocation(RelocationEvent::Completed {
                operation_id: operation.operation_id,
                catalog_id: operation.catalog_id,
            }),
        )?;
        event["causationid"] =
            operation.intent_event.as_ref().ok_or(PoolError::Corrupt)?["id"].clone();
        operation.completion_record = Some(Store::planned_record(
            &Store::inspect_read_only(&target, operation.catalog_id)?,
            &event,
        )?);
        operation.completion_event = Some(event);
        operation.checkpoint = Checkpoint::Committing;
        append(paths, locator, operation.clone())?;
        observe(RelocationCheckpoint::CompletionRecorded);
    }
    let event = operation
        .completion_event
        .clone()
        .ok_or(PoolError::Corrupt)?;
    let mut expected = source.clone();
    expected.push(event.clone());
    inspect_for_resume(paths, locator, &operation, &target, repair, observe)?;
    let events = Store::events_read_only(&target, operation.catalog_id)?;
    validate_history(
        &history_proof(&operation, &target)?,
        &events,
        &Store::raw_records_rebuild_history(&target, operation.catalog_id)?,
    )?;
    if events == source {
        if Store::planned_record(
            &Store::inspect_read_only(&target, operation.catalog_id)?,
            &event,
        )? != *operation
            .completion_record
            .as_ref()
            .ok_or(PoolError::Corrupt)?
        {
            return Err(PoolError::Conflict);
        }
        if content_evidence(&target)? != expected_content(locator, &operation, &target)? {
            return Err(PoolError::Conflict);
        }
        let store = Store::open(&target, operation.catalog_id)?;
        store.append_batch(operation.source_revision, vec![event])?;
        observe(RelocationCheckpoint::CompletionCommitted);
        drop(store);
    } else if events != expected {
        return Err(PoolError::Conflict);
    }
    if Store::events_read_only(&target, operation.catalog_id)? != expected {
        return Err(PoolError::Conflict);
    }
    operation.checkpoint = Checkpoint::Completed;
    append(paths, locator, operation.clone())?;
    observe(RelocationCheckpoint::Completed);
    Ok(operation)
}
fn finish(
    paths: &Paths,
    locator: &mut Locator,
    observe: &mut impl FnMut(RelocationCheckpoint),
    repair: bool,
) -> Result<Relocation, PoolError> {
    start(paths, locator, observe, repair)?;
    let mut operation = locator.relocation.clone().ok_or(PoolError::Corrupt)?;
    inspect_for_resume(
        paths,
        locator,
        &operation,
        &operation.source.to_path()?,
        repair,
        observe,
    )?;
    let source = Store::inspect_read_only(&operation.source.to_path()?, operation.catalog_id)?;
    // The inactive source retains Started, even after destination completion.
    source_history(&operation)?;
    if source.revision != operation.source_revision
        || content_evidence(&operation.source.to_path()?)?
            != expected_content(locator, &operation, &operation.source.to_path()?)?
        || unresolved(&source)
    {
        return Err(PoolError::Conflict);
    }
    if operation.checkpoint == Checkpoint::Intended {
        prepare(locator, &operation, observe)?;
        operation.destination_content = Some(content_evidence(&operation.destination.to_path()?)?);
        operation.checkpoint = Checkpoint::Prepared;
        append(paths, locator, operation.clone())?;
        observe(RelocationCheckpoint::DestinationPrepared);
    }
    if operation.checkpoint == Checkpoint::Prepared {
        validate_copy(locator, &operation)?;
        locator.catalog_path = operation.destination.clone();
        operation.checkpoint = Checkpoint::Switched;
        append(paths, locator, operation)?;
        observe(RelocationCheckpoint::LocatorSwitched);
    }
    complete(paths, locator, observe, repair)
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
    value["journal"] = json!("catalog_cloud_events");
    value
}

fn receipt(
    paths: &Paths,
    command: &str,
    operation: Option<&Relocation>,
    error: Option<&PoolError>,
) -> Envelope {
    let authority = catalog::inspect_active_authority(paths).ok();
    let mut envelope = error.map_or_else(
        || Envelope {
            schema_version: 1,
            command: command.into(),
            outcome: "completed",
            reason_code: "catalog_relocation_observed",
            context: json!({}),
            data: json!({}),
            warnings: Vec::new(),
        },
        |error| Envelope::failure(command.into(), error),
    );
    envelope.context = json!({"catalog_id": authority.as_ref().map(|a| a.catalog_id), "catalog_path": authority.as_ref().map(|a| &a.catalog_path), "selected_catalog_path": EncodedPath::from_path(&paths.catalog), "operation_id": operation.map(|o| o.operation_id), "repository_id": null, "worktree_id": null, "assignment_handle": null});
    envelope.data = json!({"authority": authority, "operation": operation.map(operation_value), "worktrees_moved": false, "retained_source": operation.map(|o| &o.source), "retained_source_bytes": operation.and_then(|o| o.source_bytes), "retained_source_bytes_checkpoint": operation.and_then(|o| o.source_bytes.map(|_| "intended")), "retained_source_active": operation.is_some_and(|o| matches!(o.checkpoint, Checkpoint::Starting | Checkpoint::Intended | Checkpoint::Prepared)), "catalog_mutations_available": authority.as_ref().is_some_and(catalog::AuthorityObservation::mutations_available), "next_action": authority.as_ref().map_or_else(|| next_action(operation), |a| authority_next_action(a, operation))});
    envelope
}
pub(crate) fn authority_next_action(
    authority: &catalog::AuthorityObservation,
    operation: Option<&Relocation>,
) -> String {
    if let Some(pending) = authority
        .relocation
        .as_ref()
        .filter(|r| r.checkpoint != Checkpoint::Completed)
    {
        return format!(
            "{}; this exact relocation ID can explicitly record physical storage repair and history-validated derived rebuild when required; ordinary commands never repair",
            next_action(Some(pending))
        );
    }
    if authority.phase != "active"
        || authority.store_state != "validated"
        || authority.recovery_checkpoint.as_deref() == Some("intent_recorded")
    {
        crate::catalog_recovery_cli::next_action(authority).into()
    } else if let Some(pending) = authority
        .semantic_relocations
        .iter()
        .find(|r| r.state == RelocationState::Pending)
        && !authority
            .relocation_history
            .iter()
            .any(|r| r.relocation.operation_id == pending.intent.operation_id)
    {
        missing_journal_action(pending.intent.operation_id)
    } else {
        next_action(operation)
    }
}
fn missing_journal_action(id: OperationId) -> String {
    format!(
        "relocation {id} is pending in authoritative CloudEvents; preserve catalog and locator bytes and manually restore its exact matching technical journal before recover apply --operation {id}; no replacement operation or automatic backfill is allowed"
    )
}
fn missing_journal_receipt(
    paths: &Paths,
    command: &str,
    authority: &catalog::AuthorityObservation,
    operation: &crate::relocation_events::RelocationOperation,
    conflicting_scope: bool,
) -> Envelope {
    let mut result = Envelope::failure(
        command.into(),
        if conflicting_scope {
            &PoolError::Selectors
        } else {
            &PoolError::RelocationPending
        },
    );
    result.context = json!({"catalog_id": authority.catalog_id, "catalog_path": authority.catalog_path, "selected_catalog_path": EncodedPath::from_path(&paths.catalog), "operation_id": operation.intent.operation_id});
    result.data = json!({"authority": authority, "operation": operation, "catalog_mutations_available": false, "worktrees_moved": false, "next_action": missing_journal_action(operation.intent.operation_id)});
    result
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
    let recorded = authority
        .relocation_history
        .iter()
        .rev()
        .find(|f| f.relocation.operation_id == id);
    let operation = match recorded {
        Some(fact) => fact.relocation.clone(),
        None => {
            return authority
                .semantic_relocations
                .iter()
                .find(|r| r.intent.operation_id == id)
                .map(|r| {
                    missing_journal_receipt(paths, command, &authority, r, conflicting_scope)
                });
        }
    };
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
    let id = authority
        .relocation
        .as_ref()
        .filter(|r| r.checkpoint != Checkpoint::Completed)
        .map(|r| r.operation_id)
        .or_else(|| {
            authority
                .semantic_relocations
                .iter()
                .find(|r| r.state == RelocationState::Pending)
                .map(|r| r.intent.operation_id)
        })?;
    known(paths, command, &id.to_string(), apply, conflicting_scope)
}
