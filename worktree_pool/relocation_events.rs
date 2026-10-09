//! Pure catalog relocation intent/result facts; physical coordination is separate.
use serde::{Deserialize, Serialize};

use crate::{
    domain::{CatalogId, CatalogProjection},
    error::PoolError,
    management::OperationId,
    paths::EncodedPath,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelocationIntent {
    pub operation_id: OperationId,
    pub catalog_id: CatalogId,
    pub source: EncodedPath,
    pub destination: EncodedPath,
    pub source_revision: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelocationState {
    Pending,
    Completed,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelocationOperation {
    pub intent: RelocationIntent,
    pub state: RelocationState,
    pub intent_event_id: String,
    pub completion_event_id: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "record",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum RelocationEvent {
    Started(RelocationIntent),
    Completed {
        operation_id: OperationId,
        catalog_id: CatalogId,
    },
}
impl RelocationEvent {
    #[must_use]
    pub const fn event_type(&self) -> &'static str {
        match self {
            Self::Started(_) => "io.lowkeylab.worktreepool.catalog.relocation.started.v1",
            Self::Completed { .. } => "io.lowkeylab.worktreepool.catalog.relocation.completed.v1",
        }
    }
    #[must_use]
    pub const fn operation_id(&self) -> OperationId {
        match self {
            Self::Started(i) => i.operation_id,
            Self::Completed { operation_id, .. } => *operation_id,
        }
    }
    #[must_use]
    pub const fn catalog_id(&self) -> CatalogId {
        match self {
            Self::Started(i) => i.catalog_id,
            Self::Completed { catalog_id, .. } => *catalog_id,
        }
    }
    /// # Errors
    /// Rejects reused identities, unresolved work, stale intent and false completion linkage.
    pub fn apply(
        &self,
        state: &mut CatalogProjection,
        event_id: &str,
        cause: Option<&str>,
    ) -> Result<(), PoolError> {
        if self.catalog_id() != state.catalog_id {
            return Err(PoolError::Conflict);
        }
        match self {
            Self::Started(intent) => {
                if state.operation_identity_used(intent.operation_id)
                    || state.has_pending_relocation()
                    || unresolved(state)
                    || intent.source_revision
                        != state.revision.checked_add(1).ok_or(PoolError::Corrupt)?
                    || intent.source.to_path()? == intent.destination.to_path()?
                    || cause.is_some()
                    || state
                        .relocations
                        .last()
                        .is_some_and(|r| r.intent.destination != intent.source)
                {
                    return Err(PoolError::Conflict);
                }
                state.relocations.push(RelocationOperation {
                    intent: intent.clone(),
                    state: RelocationState::Pending,
                    intent_event_id: event_id.into(),
                    completion_event_id: None,
                });
            }
            Self::Completed { operation_id, .. } => {
                let operation = state
                    .relocations
                    .iter_mut()
                    .find(|r| r.intent.operation_id == *operation_id)
                    .ok_or(PoolError::Unregistered)?;
                if operation.state != RelocationState::Pending
                    || cause != Some(operation.intent_event_id.as_str())
                {
                    return Err(PoolError::Conflict);
                }
                operation.state = RelocationState::Completed;
                operation.completion_event_id = Some(event_id.into());
            }
        }
        Ok(())
    }
}

pub(crate) fn unresolved(state: &CatalogProjection) -> bool {
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

#[cfg(test)]
mod tests {
    use super::{RelocationEvent, RelocationIntent, RelocationState};
    use crate::domain::{decode_event, initialized_event, management_event, reduce};
    use crate::{
        domain::CatalogId,
        error::PoolError,
        management::{ManagementEvent, OperationId},
        paths::EncodedPath,
    };
    use googletest::{assert_that, matchers::eq};
    use serde_json::json;

    #[googletest::test]
    fn relocation_replay_explains_pending_and_rejects_false_completion_or_reused_identity() {
        let catalog = CatalogId::new();
        let initialized = decode_event(&initialized_event(catalog).unwrap(), catalog).unwrap();
        let initial = reduce(None, 0, &initialized).unwrap();
        let intent = RelocationIntent {
            operation_id: OperationId::new(),
            catalog_id: catalog,
            source: EncodedPath::from_path(std::path::Path::new("/source/catalog.redb")),
            destination: EncodedPath::from_path(std::path::Path::new("/destination/catalog.redb")),
            source_revision: 2,
        };
        let started = management_event(
            catalog,
            &ManagementEvent::Relocation(RelocationEvent::Started(intent.clone())),
        )
        .unwrap();
        let typed = decode_event(&started, catalog).unwrap();
        let pending = reduce(Some(&initial), 1, &typed).unwrap();
        assert_that!(pending.has_pending_relocation(), eq(true));
        assert_that!(
            pending.relocations[0].intent_event_id.as_str(),
            eq(started["id"].as_str().unwrap())
        );
        assert_that!(reduce(Some(&pending), 2, &typed).is_err(), eq(true));
        let other = management_event(
            catalog,
            &ManagementEvent::CapacityConfigured {
                repository_id: crate::management::RepositoryId::new(),
                maximum: 3,
            },
        )
        .unwrap();
        assert_that!(
            matches!(
                reduce(Some(&pending), 2, &decode_event(&other, catalog).unwrap()),
                Err(PoolError::RelocationPending)
            ),
            eq(true)
        );
        let completed = management_event(
            catalog,
            &ManagementEvent::Relocation(RelocationEvent::Completed {
                operation_id: intent.operation_id,
                catalog_id: catalog,
            }),
        )
        .unwrap();
        for cause in [
            json!(null),
            json!(OperationId::new()),
            started["id"].clone(),
        ] {
            let mut event = completed.clone();
            if !cause.is_null() {
                event["causationid"] = cause.clone();
            }
            let result = reduce(Some(&pending), 2, &decode_event(&event, catalog).unwrap());
            assert_that!(result.is_ok(), eq(cause == started["id"]));
        }
        let mut completed = completed;
        completed["causationid"] = started["id"].clone();
        let complete = reduce(
            Some(&pending),
            2,
            &decode_event(&completed, catalog).unwrap(),
        )
        .unwrap();
        assert_that!(complete.has_pending_relocation(), eq(false));
        assert_that!(
            complete.relocations[0].state,
            eq(RelocationState::Completed)
        );
        assert_that!(
            reduce(
                Some(&complete),
                3,
                &decode_event(&completed, catalog).unwrap()
            )
            .is_err(),
            eq(true)
        );
        assert_that!(reduce(Some(&complete), 3, &typed).is_err(), eq(true));
        let mut foreign = completed;
        foreign["source"] = json!(format!("urn:uuid:{}", CatalogId::new()));
        assert_that!(decode_event(&foreign, catalog).is_err(), eq(true));
    }
}
