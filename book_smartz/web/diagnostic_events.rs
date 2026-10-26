//! Allowlisted operation facts and structured listener records.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct OperationId(Uuid);
impl OperationId {
    pub fn fresh() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ProcessInstanceId(Uuid);
impl ProcessInstanceId {
    pub fn fresh() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventIdentity {
    OperationFinished,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    CatalogSearch,
    RankingRead,
    SignIn,
    SignInStart,
    SignOut,
    Add,
    Preference,
    Pause,
    Resume,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    Confirmed,
    Rejected,
    DependencyFailure,
    Interrupted,
    UncertainPersistence,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Validation,
    Admission,
    Provider,
    Selection,
    Persistence,
    Authentication,
    Session,
    SessionRevocation,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    None,
    InvalidInput,
    AdmissionLimited,
    ProviderLimited,
    Timeout,
    Unavailable,
    Malformed,
    Oversized,
    Database,
    Schema,
    SelectionPreparation,
    Interrupted,
    CorruptHistory,
    PersistenceFailure,
    IdentityRejected,
    SessionOperation,
    CommitUncertain,
    ExistingSaved,
    ActionRejected,
    ActionBlocked,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct OriginalActionId(Uuid);
impl From<Uuid> for OriginalActionId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

/// This closed shape applies the privacy allowlist before listener dispatch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationFinished {
    pub event: EventIdentity,
    pub operation: OperationKind,
    pub outcome: OperationOutcome,
    pub stage: Stage,
    pub reason: Reason,
    pub duration_ms: u64,
    pub release: String,
    pub process_instance: ProcessInstanceId,
    pub operation_id: OperationId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_action_id: Option<OriginalActionId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRecord {
    pub severity: Severity,
    pub fact: OperationFinished,
}
