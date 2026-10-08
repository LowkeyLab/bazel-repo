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
    paths::{EncodedPath, Paths},
    store::Store,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Locator {
    version: u32,
    catalog_id: CatalogId,
    catalog_path: EncodedPath,
    phase: String,
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
fn read_locator(paths: &Paths) -> Result<Locator, PoolError> {
    let locator = paths.state.join("active.json");
    match fs::read(&locator) {
        Ok(bytes) => {
            private_file(&locator)?;
            let value: Locator = serde_json::from_slice(&bytes).map_err(|_| PoolError::Corrupt)?;
            if value.version != 1 {
                return Err(PoolError::Unsupported);
            }
            if value.catalog_path.to_path()? != paths.catalog {
                return Err(PoolError::Conflict);
            }
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
        Ok(locator) if locator.phase == "active" => {
            // Validate existing authority before rejecting reinitialization.
            let _ = Store::open(&paths.catalog, locator.catalog_id)?;
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
    if locator.phase == "initializing" {
        return Err(PoolError::Pending);
    }
    if locator.phase != "active" {
        return Err(PoolError::Corrupt);
    }
    private_file(&paths.catalog).map_err(|error| match error {
        PoolError::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound => PoolError::Missing,
        _ => error,
    })?;
    let store = Store::open(&paths.catalog, locator.catalog_id)?;
    let projection = store.projection()?;
    Ok(CatalogSession {
        projection,
        store,
        _catalog_lock: catalog,
        _maintenance_lock: maintenance,
    })
}

#[derive(Serialize)]
pub struct AuthorityObservation {
    pub catalog_id: CatalogId,
    pub catalog_path: EncodedPath,
    pub phase: String,
    pub last_checkpoint: &'static str,
    pub store_state: &'static str,
    pub revision: Option<u64>,
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
    if locator.phase != "active" && locator.phase != "initializing" {
        return Err(PoolError::Corrupt);
    }
    let projection = if paths.catalog.exists() {
        private_file(&paths.catalog)
            .and_then(|()| Store::open(&paths.catalog, locator.catalog_id))
            .and_then(|store| store.projection())
            .ok()
    } else {
        None
    };
    let store_state = if projection.is_some() {
        "validated"
    } else if paths.catalog.exists() {
        "unreadable"
    } else {
        "missing"
    };
    let last_checkpoint = if locator.phase == "active" {
        "authority_published"
    } else if projection.is_some() {
        "store_committed"
    } else {
        "intent_recorded"
    };
    Ok(AuthorityObservation {
        catalog_id: locator.catalog_id,
        catalog_path: locator.catalog_path,
        phase: locator.phase,
        last_checkpoint,
        store_state,
        revision: projection.map(|p| p.revision),
    })
}
