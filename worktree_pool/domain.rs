//! Pure catalog facts and replay. Replay never performs filesystem or Git effects.
use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use cloudevents::{Event, EventBuilder, EventBuilderV10};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::PoolError;

pub const STORE_VERSION: u32 = 1;
pub const PROJECTION_VERSION: u32 = 1;
pub const INITIALIZED_TYPE: &str = "io.lowkeylab.worktreepool.catalog.initialized.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CatalogId(Uuid);

impl CatalogId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CatalogId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CatalogId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<CatalogId> for String {
    fn from(id: CatalogId) -> Self {
        id.to_string()
    }
}

impl TryFrom<String> for CatalogId {
    type Error = PoolError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl FromStr for CatalogId {
    type Err = PoolError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let id = Uuid::parse_str(value).map_err(|_| PoolError::Corrupt)?;
        if id.to_string() != value || id.is_nil() {
            return Err(PoolError::Corrupt);
        }
        Ok(Self(id))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogProjection {
    pub catalog_id: CatalogId,
    pub revision: u64,
    pub store_version: u32,
    pub projection_version: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogInitialized {
    pub catalog_id: CatalogId,
    pub store_version: u32,
    pub projection_version: u32,
}

#[derive(Clone, Debug)]
pub enum DomainEvent {
    CatalogInitialized(CatalogInitialized),
}

/// Records a fresh initialization fact with its event and operation identities.
///
/// # Errors
/// Returns a corruption error if the typed envelope cannot be built or serialized.
pub fn initialized_event(catalog_id: CatalogId) -> Result<Value, PoolError> {
    let data = CatalogInitialized {
        catalog_id,
        store_version: STORE_VERSION,
        projection_version: PROJECTION_VERSION,
    };
    let envelope = EventBuilderV10::new()
        .id(Uuid::new_v4().to_string())
        .source(format!("urn:uuid:{catalog_id}"))
        .ty(INITIALIZED_TYPE)
        .subject(format!("catalogs/{catalog_id}"))
        .time(Utc::now())
        .extension("operationid", Uuid::new_v4().to_string())
        .data(
            "application/json",
            serde_json::to_value(data).map_err(|_| PoolError::Corrupt)?,
        )
        .build()
        .map_err(|_| PoolError::Corrupt)?;
    serde_json::to_value(envelope).map_err(|_| PoolError::Corrupt)
}

/// Validates the supported authoritative `CloudEvents` profile before replay.
///
/// # Errors
/// Returns an unsupported-version error for unknown event or schema versions,
/// or a corruption error for malformed metadata, payloads, or identities.
pub fn decode_event(value: &Value, catalog_id: CatalogId) -> Result<DomainEvent, PoolError> {
    let object = value.as_object().ok_or(PoolError::Corrupt)?;
    if value["specversion"] != "1.0" || value["type"] != INITIALIZED_TYPE {
        return Err(PoolError::Unsupported);
    }
    let _: Event = serde_json::from_value(value.clone()).map_err(|_| PoolError::Corrupt)?;
    let event_id = value["id"].as_str().ok_or(PoolError::Corrupt)?;
    let parsed_id = Uuid::parse_str(event_id).map_err(|_| PoolError::Corrupt)?;
    if parsed_id.is_nil()
        || parsed_id.to_string() != event_id
        || value["source"] != format!("urn:uuid:{catalog_id}")
        || value["subject"] != format!("catalogs/{catalog_id}")
        || value["datacontenttype"] != "application/json"
    {
        return Err(PoolError::Corrupt);
    }
    let time = value["time"].as_str().ok_or(PoolError::Corrupt)?;
    let parsed_time = DateTime::parse_from_rfc3339(time).map_err(|_| PoolError::Corrupt)?;
    if parsed_time.offset().local_minus_utc() != 0 {
        return Err(PoolError::Corrupt);
    }
    for (key, field) in object {
        match key.as_str() {
            "specversion" | "id" | "source" | "type" | "subject" | "time" | "datacontenttype"
            | "data" => {}
            "operationid" | "causationid"
                if field.as_str().is_some_and(|text| {
                    !text.is_empty() && !text.chars().any(char::is_control)
                }) => {}
            _ => return Err(PoolError::Corrupt),
        }
    }
    let data: CatalogInitialized =
        serde_json::from_value(value["data"].clone()).map_err(|_| PoolError::Corrupt)?;
    if data.catalog_id != catalog_id {
        return Err(PoolError::Corrupt);
    }
    if data.store_version != STORE_VERSION || data.projection_version != PROJECTION_VERSION {
        return Err(PoolError::Unsupported);
    }
    Ok(DomainEvent::CatalogInitialized(data))
}

/// Applies exactly one fact at its expected stream revision.
///
/// # Errors
/// Returns a conflict for stale revisions or repeated initialization, or an
/// unsupported-version error for incompatible fact data.
pub fn reduce(
    current: Option<&CatalogProjection>,
    expected_revision: u64,
    event: &DomainEvent,
) -> Result<CatalogProjection, PoolError> {
    if current.map_or(0, |projection| projection.revision) != expected_revision {
        return Err(PoolError::Conflict);
    }
    match event {
        DomainEvent::CatalogInitialized(data)
            if data.store_version != STORE_VERSION
                || data.projection_version != PROJECTION_VERSION =>
        {
            Err(PoolError::Unsupported)
        }
        DomainEvent::CatalogInitialized(data) if current.is_none() => Ok(CatalogProjection {
            catalog_id: data.catalog_id,
            revision: 1,
            store_version: data.store_version,
            projection_version: data.projection_version,
        }),
        DomainEvent::CatalogInitialized(_) => Err(PoolError::Conflict),
    }
}
