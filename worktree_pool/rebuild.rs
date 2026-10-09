//! Pure reconstruction of complete immutable positioned history.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    domain::{CatalogId, CatalogProjection, decode_event, reduce},
    error::PoolError,
};

/// Committed ordering metadata is authoritative; timestamps never order replay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedEvent {
    pub position: u64,
    pub expected_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
    pub event: Value,
}

pub struct Reconstruction {
    pub projection: CatalogProjection,
    pub(crate) identities: BTreeMap<String, u64>,
}

/// Reconstructs supported facts in their complete committed order, without effects.
/// # Errors
/// Rejects empty, unsupported, duplicate, gapped, misrouted or causally invalid history.
pub fn reconstruct(
    catalog_id: CatalogId,
    records: &[(u64, RecordedEvent)],
) -> Result<Reconstruction, PoolError> {
    let mut rebuilt = None;
    let mut identities = BTreeMap::new();
    let mut revision = 0_u64;
    for (position, recorded) in records {
        let next = revision.checked_add(1).ok_or(PoolError::Corrupt)?;
        if *position != next || recorded.position != next {
            return Err(PoolError::Corrupt);
        }
        if identities
            .insert(identity(&recorded.event)?, next)
            .is_some()
        {
            return Err(PoolError::Corrupt);
        }
        let event = decode_event(&recorded.event, catalog_id)?;
        let stream_id = event.stream_id(catalog_id);
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
        rebuilt = Some(reduce(rebuilt.as_ref(), revision, &event).map_err(|_| PoolError::Corrupt)?);
        revision = next;
    }
    Ok(Reconstruction {
        projection: rebuilt.ok_or(PoolError::Corrupt)?,
        identities,
    })
}

pub(crate) fn identity(event: &Value) -> Result<String, PoolError> {
    let source = event["source"].as_str().ok_or(PoolError::Corrupt)?;
    let id = event["id"].as_str().ok_or(PoolError::Corrupt)?;
    Ok(format!("{source}\0{id}"))
}

/// Observation points surround actual derived-state publication, never Git effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RebuildCheckpoint {
    Validated,
    BeforeCommit,
    Committed,
}
