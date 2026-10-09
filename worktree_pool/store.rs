//! Focused redb adapter. Callers hold the catalog lock for this handle's lifetime.
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt, path::Path};

use redb::{
    Database, Durability, ReadOnlyDatabase, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition,
};
use serde_json::Value;

use crate::{
    domain::{CatalogId, CatalogProjection, decode_event, initialized_event, reduce},
    error::PoolError,
    rebuild::{RebuildCheckpoint, Reconstruction, RecordedEvent, identity, reconstruct},
};

const EVENTS: TableDefinition<u64, &str> = TableDefinition::new("events");
const STATE: TableDefinition<&str, &str> = TableDefinition::new("state");
const IDENTITIES: TableDefinition<&str, u64> = TableDefinition::new("event_identities");

pub struct Store {
    database: Database,
    catalog_id: CatalogId,
}

/// A repair-needed database is distinct from invalid or unreadable authority.
#[derive(Debug)]
pub enum ReadOnlyInspectionError {
    RepairRequired,
    Rejected(PoolError),
}

impl Store {
    /// Creates a new catalog with a fresh identity.
    ///
    /// # Errors
    /// Returns filesystem or storage errors without replacing existing files.
    /// A commit error is indeterminate and requires reopen and inspection.
    pub fn create(path: &Path) -> Result<CatalogProjection, PoolError> {
        Self::create_with_id(path, CatalogId::new())
    }

    /// Creates a new catalog using the identity recorded in its durable intent.
    ///
    /// # Errors
    /// Returns filesystem or storage errors without replacing existing files.
    /// A commit error is indeterminate and requires reopen and inspection.
    pub fn create_with_id(
        path: &Path,
        catalog_id: CatalogId,
    ) -> Result<CatalogProjection, PoolError> {
        Self::initialize(path, catalog_id, false)
    }

    fn initialize(
        path: &Path,
        catalog_id: CatalogId,
        lose_result: bool,
    ) -> Result<CatalogProjection, PoolError> {
        // Reserving the filename is atomic and refuses every existing file, even empty/corrupt ones.
        let reservation = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        drop(reservation);
        let database = Database::create(path).map_err(|_| PoolError::Storage)?;
        let projection = Self::initialize_database(database, catalog_id, || {})?;
        if lose_result {
            return Err(PoolError::CommitUnknown);
        }
        Ok(projection)
    }

    fn initialize_database(
        database: Database,
        catalog_id: CatalogId,
        before_commit: impl FnOnce(),
    ) -> Result<CatalogProjection, PoolError> {
        let event = initialized_event(catalog_id)?;
        let projection = reduce(None, 0, &decode_event(&event, catalog_id)?)?;
        let recorded = RecordedEvent {
            position: 1,
            expected_revision: 0,
            stream_id: None,
            event,
        };
        let wire = serde_json::to_string(&recorded).map_err(|_| PoolError::Corrupt)?;
        let state = serde_json::to_string(&projection).map_err(|_| PoolError::Corrupt)?;
        let transaction = database.begin_write().map_err(|_| PoolError::Storage)?;
        let mut transaction = transaction;
        transaction
            .set_durability(Durability::Immediate)
            .map_err(|_| PoolError::Storage)?;
        {
            transaction
                .open_table(EVENTS)
                .map_err(|_| PoolError::Storage)?
                .insert(1, wire.as_str())
                .map_err(|_| PoolError::Storage)?;
            transaction
                .open_table(STATE)
                .map_err(|_| PoolError::Storage)?
                .insert("catalog", state.as_str())
                .map_err(|_| PoolError::Storage)?;
            transaction
                .open_table(IDENTITIES)
                .map_err(|_| PoolError::Storage)?
                .insert(identity(&recorded.event)?.as_str(), 1)
                .map_err(|_| PoolError::Storage)?;
        }
        // A commit failure can be indeterminate; the handle is dropped and callers must inspect by identity.
        before_commit();
        transaction.commit().map_err(|_| PoolError::CommitUnknown)?;
        drop(database);
        Ok(projection)
    }

    /// Initializes through a supplied real backend with a commit fault seam.
    ///
    /// # Errors
    /// Returns storage errors or an indeterminate commit error.
    #[cfg(test)]
    pub fn create_with_fault_backend(
        catalog_id: CatalogId,
        backend: impl redb::StorageBackend,
        before_commit: impl FnOnce(),
    ) -> Result<CatalogProjection, PoolError> {
        let database = Database::builder()
            .create_with_backend(backend)
            .map_err(|_| PoolError::Storage)?;
        Self::initialize_database(database, catalog_id, before_commit)
    }

    /// Opens the real adapter through a supplied backend for storage-failure checks.
    /// # Errors
    /// Rejects storage failures or an invalid existing catalog.
    #[cfg(test)]
    pub fn open_with_fault_backend(
        catalog_id: CatalogId,
        backend: impl redb::StorageBackend,
    ) -> Result<Self, PoolError> {
        let database = Database::builder()
            .create_with_backend(backend)
            .map_err(|_| PoolError::Storage)?;
        let store = Self {
            database,
            catalog_id,
        };
        store.projection()?;
        Ok(store)
    }

    /// Opens an existing catalog and validates its identity and full history.
    ///
    /// # Errors
    /// Returns storage errors, identity conflicts, unsupported versions, or
    /// corruption errors when history and derived state disagree.
    pub fn open(path: &Path, expected_id: CatalogId) -> Result<Self, PoolError> {
        let database = Database::open(path).map_err(|_| PoolError::Storage)?;
        let store = Self {
            database,
            catalog_id: expected_id,
        };
        store.projection()?;
        Ok(store)
    }

    /// Validates metadata and replay against every retained authoritative event.
    ///
    /// # Errors
    /// Returns storage errors, identity conflicts, unsupported versions, or
    /// corruption errors for invalid history, ordering, or derived state.
    pub fn projection(&self) -> Result<CatalogProjection, PoolError> {
        Self::validated_projection(&self.database, self.catalog_id)
    }

    /// Opens existing authority without modifying its bytes, including on rejection.
    /// # Errors
    /// Rejects unreadable, repair-required, conflicting, unsupported, or corrupt state.
    pub fn inspect_read_only(
        path: &Path,
        catalog_id: CatalogId,
    ) -> Result<CatalogProjection, PoolError> {
        Self::inspect_for_recovery(path, catalog_id).map_err(|error| match error {
            ReadOnlyInspectionError::RepairRequired => PoolError::Storage,
            ReadOnlyInspectionError::Rejected(error) => error,
        })
    }

    /// Distinguishes redb's actual repair requirement without writing the database.
    /// # Errors
    /// Returns `RepairRequired` only for redb `RepairAborted`; otherwise preserves validation errors.
    pub fn inspect_for_recovery(
        path: &Path,
        catalog_id: CatalogId,
    ) -> Result<CatalogProjection, ReadOnlyInspectionError> {
        let database = ReadOnlyDatabase::open(path).map_err(|error| match error {
            redb::DatabaseError::RepairAborted => ReadOnlyInspectionError::RepairRequired,
            _ => ReadOnlyInspectionError::Rejected(PoolError::Storage),
        })?;
        Self::validated_projection(&database, catalog_id).map_err(ReadOnlyInspectionError::Rejected)
    }

    fn validated_projection(
        database: &impl ReadableDatabase,
        catalog_id: CatalogId,
    ) -> Result<CatalogProjection, PoolError> {
        let transaction = database.begin_read().map_err(|_| PoolError::Storage)?;
        let state_table = transaction
            .open_table(STATE)
            .map_err(|_| PoolError::Corrupt)?;
        let state = state_table
            .get("catalog")
            .map_err(|_| PoolError::Corrupt)?
            .ok_or(PoolError::Corrupt)?;
        let stored: CatalogProjection =
            serde_json::from_str(state.value()).map_err(|_| PoolError::Corrupt)?;
        if stored.catalog_id != catalog_id {
            return Err(PoolError::Conflict);
        }
        if stored.store_version != crate::domain::STORE_VERSION
            || stored.projection_version != crate::domain::PROJECTION_VERSION
        {
            return Err(PoolError::Unsupported);
        }
        let rebuilt = reconstruct(catalog_id, &Self::records(&transaction)?)?;
        let identities = transaction
            .open_table(IDENTITIES)
            .map_err(|_| PoolError::Corrupt)?;
        if identities.len().map_err(|_| PoolError::Corrupt)? != rebuilt.projection.revision {
            return Err(PoolError::Corrupt);
        }
        for (identity, position) in &rebuilt.identities {
            if identities
                .get(identity.as_str())
                .map_err(|_| PoolError::Corrupt)?
                .map(|entry| entry.value())
                != Some(*position)
            {
                return Err(PoolError::Corrupt);
            }
        }
        if rebuilt.projection != stored {
            return Err(PoolError::Corrupt);
        }
        Ok(stored)
    }

    fn records(
        transaction: &redb::ReadTransaction,
    ) -> Result<Vec<(u64, RecordedEvent)>, PoolError> {
        let events = transaction
            .open_table(EVENTS)
            .map_err(|_| PoolError::Corrupt)?;
        events
            .iter()
            .map_err(|_| PoolError::Corrupt)?
            .map(|record| {
                let (position, wire) = record.map_err(|_| PoolError::Corrupt)?;
                Ok((
                    position.value(),
                    serde_json::from_str(wire.value()).map_err(|_| PoolError::Corrupt)?,
                ))
            })
            .collect()
    }

    // A derived record is replaceable, but recognizable foreign identity or version
    // metadata must never authorize adoption or an implicit upgrade.
    fn rebuild_metadata(
        transaction: &redb::ReadTransaction,
        catalog_id: CatalogId,
    ) -> Result<(), PoolError> {
        let table = match transaction.open_table(STATE) {
            Ok(table) => table,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(()),
            Err(_) => return Err(PoolError::Corrupt),
        };
        if let Some(state) = table.get("catalog").map_err(|_| PoolError::Corrupt)?
            && let Ok(value) = serde_json::from_str::<Value>(state.value())
        {
            if value
                .get("catalog_id")
                .is_some_and(|id| *id != serde_json::json!(catalog_id))
            {
                return Err(PoolError::Conflict);
            }
            if value
                .get("store_version")
                .is_some_and(|v| *v != crate::domain::STORE_VERSION)
                || value
                    .get("projection_version")
                    .is_some_and(|v| *v != crate::domain::PROJECTION_VERSION)
            {
                return Err(PoolError::Unsupported);
            }
        }
        Ok(())
    }

    fn authoritative_projection(
        database: &impl ReadableDatabase,
        catalog_id: CatalogId,
    ) -> Result<Reconstruction, PoolError> {
        let transaction = database.begin_read().map_err(|_| PoolError::Storage)?;
        Self::rebuild_metadata(&transaction, catalog_id)?;
        reconstruct(catalog_id, &Self::records(&transaction)?)
    }

    /// Validates immutable authority independently of replaceable derived records.
    /// # Errors
    /// Rejects unsafe, unsupported, corrupt or repair-required authority without writes.
    pub(crate) fn inspect_rebuild_history(
        path: &Path,
        catalog_id: CatalogId,
    ) -> Result<CatalogProjection, PoolError> {
        let database = ReadOnlyDatabase::open(path).map_err(|_| PoolError::Storage)?;
        Ok(Self::authoritative_projection(&database, catalog_id)?.projection)
    }

    /// Only the explicit journaled storage-repair lifecycle may call this writable open.
    /// # Errors
    /// Rejects invalid immutable authority after any necessary redb metadata repair.
    pub(crate) fn repair_authority(
        path: &Path,
        catalog_id: CatalogId,
    ) -> Result<CatalogProjection, PoolError> {
        let database = Database::open(path).map_err(|_| PoolError::Storage)?;
        Ok(Self::authoritative_projection(&database, catalog_id)?.projection)
    }

    /// Explicitly replaces derived state from complete supported immutable history.
    /// Callers hold exclusive maintenance and catalog locks for the whole operation.
    /// # Errors
    /// Refuses unsafe authority before writable open; an uncertain commit requires inspection.
    pub fn rebuild(
        path: &Path,
        catalog_id: CatalogId,
        mut observe: impl FnMut(RebuildCheckpoint),
    ) -> Result<CatalogProjection, PoolError> {
        {
            let database = ReadOnlyDatabase::open(path).map_err(|_| PoolError::Storage)?;
            Self::authoritative_projection(&database, catalog_id)?;
        }
        observe(RebuildCheckpoint::Validated);
        let database = Database::open(path).map_err(|_| PoolError::Storage)?;
        Self::replace_projection(database, catalog_id, observe)
    }

    /// Exercises the same publication with a real storage backend supplying sync faults.
    /// # Errors
    /// Preserves history refusal and indeterminate commit semantics.
    #[cfg(test)]
    pub fn rebuild_with_fault_backend(
        catalog_id: CatalogId,
        backend: impl redb::StorageBackend,
        observe: impl FnMut(RebuildCheckpoint),
    ) -> Result<CatalogProjection, PoolError> {
        let database = Database::builder()
            .create_with_backend(backend)
            .map_err(|_| PoolError::Storage)?;
        Self::replace_projection(database, catalog_id, observe)
    }

    fn replace_projection(
        database: Database,
        catalog_id: CatalogId,
        mut observe: impl FnMut(RebuildCheckpoint),
    ) -> Result<CatalogProjection, PoolError> {
        let records = {
            let transaction = database.begin_read().map_err(|_| PoolError::Storage)?;
            Self::rebuild_metadata(&transaction, catalog_id)?;
            Self::records(&transaction)?
        };
        let reconstructed = reconstruct(catalog_id, &records)?;
        let mut transaction = database.begin_write().map_err(|_| PoolError::Storage)?;
        transaction
            .set_durability(Durability::Immediate)
            .map_err(|_| PoolError::Storage)?;
        {
            let events = transaction
                .open_table(EVENTS)
                .map_err(|_| PoolError::Corrupt)?;
            let persisted: Vec<(u64, RecordedEvent)> = events
                .iter()
                .map_err(|_| PoolError::Corrupt)?
                .map(|record| {
                    let (position, wire) = record.map_err(|_| PoolError::Corrupt)?;
                    Ok((
                        position.value(),
                        serde_json::from_str(wire.value()).map_err(|_| PoolError::Corrupt)?,
                    ))
                })
                .collect::<Result<_, PoolError>>()?;
            if persisted != records {
                return Err(PoolError::Conflict);
            }
        }
        {
            let mut identities = transaction
                .open_table(IDENTITIES)
                .map_err(|_| PoolError::Storage)?;
            identities
                .retain(|_, _| false)
                .map_err(|_| PoolError::Storage)?;
            for (identity, position) in &reconstructed.identities {
                identities
                    .insert(identity.as_str(), *position)
                    .map_err(|_| PoolError::Storage)?;
            }
            transaction
                .open_table(STATE)
                .map_err(|_| PoolError::Storage)?
                .insert(
                    "catalog",
                    serde_json::to_string(&reconstructed.projection)
                        .map_err(|_| PoolError::Corrupt)?
                        .as_str(),
                )
                .map_err(|_| PoolError::Storage)?;
        }
        observe(RebuildCheckpoint::BeforeCommit);
        transaction.commit().map_err(|_| PoolError::CommitUnknown)?;
        drop(database);
        observe(RebuildCheckpoint::Committed);
        Ok(reconstructed.projection)
    }

    /// Returns validated authoritative envelopes in committed stream order.
    ///
    /// # Errors
    /// Returns storage errors or catalog validation errors before exposing history.
    pub fn events(&self) -> Result<Vec<Value>, PoolError> {
        Self::validated_events(&self.database, self.catalog_id)
    }

    /// Returns validated envelopes without modifying the catalog file.
    /// # Errors
    /// Rejects unreadable, repair-required, conflicting, unsupported, or corrupt state.
    pub fn events_read_only(path: &Path, catalog_id: CatalogId) -> Result<Vec<Value>, PoolError> {
        let database = ReadOnlyDatabase::open(path).map_err(|_| PoolError::Storage)?;
        Self::validated_events(&database, catalog_id)
    }

    fn validated_events(
        database: &impl ReadableDatabase,
        catalog_id: CatalogId,
    ) -> Result<Vec<Value>, PoolError> {
        Self::validated_projection(database, catalog_id)?;
        let transaction = database.begin_read().map_err(|_| PoolError::Storage)?;
        let table = transaction
            .open_table(EVENTS)
            .map_err(|_| PoolError::Corrupt)?;
        table
            .iter()
            .map_err(|_| PoolError::Corrupt)?
            .map(|record| {
                let (_, json) = record.map_err(|_| PoolError::Corrupt)?;
                let record: RecordedEvent =
                    serde_json::from_str(json.value()).map_err(|_| PoolError::Corrupt)?;
                Ok(record.event)
            })
            .collect()
    }

    /// Atomically appends a fact and its derived projection at the expected revision.
    /// # Errors
    /// Rejects invalid facts or revisions; failed commit requires identity inspection.
    pub fn append(
        &self,
        expected_revision: u64,
        event: Value,
    ) -> Result<CatalogProjection, PoolError> {
        let current = self.projection()?;
        let typed = decode_event(&event, self.catalog_id)?;
        if typed.expected_revision(&current)? != expected_revision {
            return Err(PoolError::Conflict);
        }
        self.append_batch(current.revision, vec![event])
    }

    /// Commits a short ordered registration/reservation batch as one atomic fact set.
    /// # Errors
    /// Rejects stale state or any invalid fact; failed commit requires reopen/inspection.
    pub fn append_batch(
        &self,
        expected_global_revision: u64,
        events: Vec<Value>,
    ) -> Result<CatalogProjection, PoolError> {
        self.append_batch_at_commit(expected_global_revision, events, || {})
    }

    /// Observes the actual transaction commit after all preparation succeeds.
    /// # Errors
    /// Preserves normal append validation and indeterminate commit reporting.
    #[cfg(test)]
    pub fn append_batch_observed(
        &self,
        expected_global_revision: u64,
        events: Vec<Value>,
        before_commit: impl FnOnce(),
    ) -> Result<CatalogProjection, PoolError> {
        self.append_batch_at_commit(expected_global_revision, events, before_commit)
    }

    fn append_batch_at_commit(
        &self,
        expected_global_revision: u64,
        events: Vec<Value>,
        before_commit: impl FnOnce(),
    ) -> Result<CatalogProjection, PoolError> {
        let current = self.projection()?;
        if current.revision != expected_global_revision || events.is_empty() {
            return Err(PoolError::Conflict);
        }
        let mut projection = current.clone();
        let mut records = Vec::new();
        for event in events {
            let typed = decode_event(&event, self.catalog_id)?;
            let expected_revision = typed.expected_revision(&projection)?;
            projection = reduce(Some(&projection), projection.revision, &typed)?;
            records.push(RecordedEvent {
                position: projection.revision,
                expected_revision,
                stream_id: Some(typed.stream_id(self.catalog_id)),
                event,
            });
        }
        let state = serde_json::to_string(&projection).map_err(|_| PoolError::Corrupt)?;
        let mut transaction = self
            .database
            .begin_write()
            .map_err(|_| PoolError::Storage)?;
        {
            let table = transaction
                .open_table(STATE)
                .map_err(|_| PoolError::Storage)?;
            let persisted = table
                .get("catalog")
                .map_err(|_| PoolError::Storage)?
                .ok_or(PoolError::Corrupt)?;
            let persisted: CatalogProjection =
                serde_json::from_str(persisted.value()).map_err(|_| PoolError::Corrupt)?;
            if persisted != current {
                return Err(PoolError::Conflict);
            }
        }
        transaction
            .set_durability(Durability::Immediate)
            .map_err(|_| PoolError::Storage)?;
        {
            let mut identities = transaction
                .open_table(IDENTITIES)
                .map_err(|_| PoolError::Storage)?;
            let mut events = transaction
                .open_table(EVENTS)
                .map_err(|_| PoolError::Storage)?;
            for record in &records {
                let key = identity(&record.event)?;
                if identities
                    .get(key.as_str())
                    .map_err(|_| PoolError::Storage)?
                    .is_some()
                {
                    return Err(PoolError::Conflict);
                }
                identities
                    .insert(key.as_str(), record.position)
                    .map_err(|_| PoolError::Storage)?;
                let wire = serde_json::to_string(record).map_err(|_| PoolError::Corrupt)?;
                events
                    .insert(record.position, wire.as_str())
                    .map_err(|_| PoolError::Storage)?;
            }
            transaction
                .open_table(STATE)
                .map_err(|_| PoolError::Storage)?
                .insert("catalog", state.as_str())
                .map_err(|_| PoolError::Storage)?;
        }
        before_commit();
        transaction.commit().map_err(|_| PoolError::CommitUnknown)?;
        Ok(projection)
    }

    /// Simulates the caller losing a successful commit result after durable redb commit.
    ///
    /// # Errors
    /// Returns initialization failures or an indeterminate result after successful commit.
    #[cfg(test)]
    pub fn create_with_lost_commit_result(
        path: &Path,
        catalog_id: CatalogId,
    ) -> Result<CatalogProjection, PoolError> {
        Self::initialize(path, catalog_id, true)
    }
}
