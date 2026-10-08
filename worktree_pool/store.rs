//! Focused redb adapter. Callers hold the catalog lock for this handle's lifetime.
use std::{collections::HashSet, fs::OpenOptions, os::unix::fs::OpenOptionsExt, path::Path};

use redb::{
    Database, Durability, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    domain::{CatalogId, CatalogProjection, decode_event, initialized_event, reduce},
    error::PoolError,
};

const EVENTS: TableDefinition<u64, &str> = TableDefinition::new("events");
const STATE: TableDefinition<&str, &str> = TableDefinition::new("state");
const IDENTITIES: TableDefinition<&str, u64> = TableDefinition::new("event_identities");

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordedEvent {
    position: u64,
    expected_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stream_id: Option<String>,
    event: Value,
}

pub struct Store {
    database: Database,
    catalog_id: CatalogId,
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
        let transaction = self.database.begin_read().map_err(|_| PoolError::Storage)?;
        let state_table = transaction
            .open_table(STATE)
            .map_err(|_| PoolError::Corrupt)?;
        let state = state_table
            .get("catalog")
            .map_err(|_| PoolError::Corrupt)?
            .ok_or(PoolError::Corrupt)?;
        let stored: CatalogProjection =
            serde_json::from_str(state.value()).map_err(|_| PoolError::Corrupt)?;
        if stored.catalog_id != self.catalog_id {
            return Err(PoolError::Conflict);
        }
        if stored.store_version != crate::domain::STORE_VERSION
            || stored.projection_version != crate::domain::PROJECTION_VERSION
        {
            return Err(PoolError::Unsupported);
        }
        let events = transaction
            .open_table(EVENTS)
            .map_err(|_| PoolError::Corrupt)?;
        let identities = transaction
            .open_table(IDENTITIES)
            .map_err(|_| PoolError::Corrupt)?;
        let mut rebuilt = None;
        let mut seen = HashSet::new();
        let mut revision = 0_u64;
        for record in events.iter().map_err(|_| PoolError::Corrupt)? {
            let (position, json) = record.map_err(|_| PoolError::Corrupt)?;
            let recorded: RecordedEvent =
                serde_json::from_str(json.value()).map_err(|_| PoolError::Corrupt)?;
            let next = revision.checked_add(1).ok_or(PoolError::Corrupt)?;
            if position.value() != next || recorded.position != next {
                return Err(PoolError::Corrupt);
            }
            let identity = identity(&recorded.event)?;
            if !seen.insert(identity.clone())
                || identities
                    .get(identity.as_str())
                    .map_err(|_| PoolError::Corrupt)?
                    .map(|entry| entry.value())
                    != Some(next)
            {
                return Err(PoolError::Corrupt);
            }
            let event = decode_event(&recorded.event, self.catalog_id)?;
            let stream_id = event.stream_id(self.catalog_id);
            let expected = match rebuilt.as_ref() {
                Some(state) => event.expected_revision(state)?,
                None => 0,
            };
            if recorded.expected_revision != expected
                || recorded.stream_id.as_ref().is_some_and(|s| *s != stream_id)
                || (recorded.stream_id.is_none() && next != 1)
            {
                return Err(PoolError::Corrupt);
            }
            rebuilt =
                Some(reduce(rebuilt.as_ref(), revision, &event).map_err(|_| PoolError::Corrupt)?);
            revision = next;
        }
        if identities.len().map_err(|_| PoolError::Corrupt)? != revision
            || rebuilt.as_ref() != Some(&stored)
        {
            return Err(PoolError::Corrupt);
        }
        Ok(stored)
    }

    /// Returns validated authoritative envelopes in committed stream order.
    ///
    /// # Errors
    /// Returns storage errors or catalog validation errors before exposing history.
    pub fn events(&self) -> Result<Vec<Value>, PoolError> {
        self.projection()?;
        let transaction = self.database.begin_read().map_err(|_| PoolError::Storage)?;
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
        let projection = reduce(Some(&current), current.revision, &typed)?;
        let recorded = RecordedEvent {
            position: projection.revision,
            expected_revision,
            stream_id: Some(typed.stream_id(self.catalog_id)),
            event,
        };
        let wire = serde_json::to_string(&recorded).map_err(|_| PoolError::Corrupt)?;
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
            let identity = identity(&recorded.event)?;
            if identities
                .get(identity.as_str())
                .map_err(|_| PoolError::Storage)?
                .is_some()
            {
                return Err(PoolError::Conflict);
            }
            identities
                .insert(identity.as_str(), projection.revision)
                .map_err(|_| PoolError::Storage)?;
            transaction
                .open_table(EVENTS)
                .map_err(|_| PoolError::Storage)?
                .insert(projection.revision, wire.as_str())
                .map_err(|_| PoolError::Storage)?;
            transaction
                .open_table(STATE)
                .map_err(|_| PoolError::Storage)?
                .insert("catalog", state.as_str())
                .map_err(|_| PoolError::Storage)?;
        }
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

fn identity(event: &Value) -> Result<String, PoolError> {
    let source = event["source"].as_str().ok_or(PoolError::Corrupt)?;
    let id = event["id"].as_str().ok_or(PoolError::Corrupt)?;
    Ok(format!("{source}\0{id}"))
}
