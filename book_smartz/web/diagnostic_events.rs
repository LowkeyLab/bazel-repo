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
    DiagnosticLossObserved,
    StartupFinished,
    MigrationFinished,
    ReadinessChanged,
    ShutdownStarted,
    ShutdownFinished,
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
    OriginalSaveCheck,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    Running,
    Stopped,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticLossObserved {
    pub event: EventIdentity,
    pub queue_drops: u64,
    pub destination_failures: u64,
    pub worker_status: WorkerStatus,
    pub release: String,
    pub process_instance: ProcessInstanceId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticLossRecord {
    pub severity: Severity,
    pub fact: DiagnosticLossObserved,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitializationOutcome {
    Succeeded,
    Failed,
    DeadlineExceeded,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitializationStage {
    Configuration,
    Database,
    Package,
    Provider,
    Schema,
    Listener,
    Complete,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InitializationReason {
    None,
    Configuration,
    Database,
    Package,
    Provider,
    Schema,
    Unavailable,
    Timeout,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InitializationFinished {
    pub event: EventIdentity,
    pub outcome: InitializationOutcome,
    pub stage: InitializationStage,
    pub reason: InitializationReason,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    pub process_instance: ProcessInstanceId,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InitializationRecord {
    pub severity: Severity,
    pub fact: InitializationFinished,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessState {
    Ready,
    Unavailable,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessStage {
    Complete,
    Package,
    Schema,
    Database,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadinessReason {
    None,
    Package,
    Schema,
    Database,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessResult {
    pub state: ReadinessState,
    pub stage: ReadinessStage,
    pub reason: ReadinessReason,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessChanged {
    pub event: EventIdentity,
    pub state: ReadinessState,
    pub stage: ReadinessStage,
    pub reason: ReadinessReason,
    pub duration_ms: u64,
    pub release: String,
    pub process_instance: ProcessInstanceId,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessRecord {
    pub severity: Severity,
    pub fact: ReadinessChanged,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownOutcome {
    Started,
    Completed,
    Failed,
    DeadlineExceeded,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownStage {
    RequestDrain,
    PoolClosure,
    DiagnosticDrain,
    Complete,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownReason {
    None,
    Signal,
    Listener,
    Timeout,
    Unavailable,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShutdownFact {
    pub event: EventIdentity,
    pub outcome: ShutdownOutcome,
    pub stage: ShutdownStage,
    pub reason: ShutdownReason,
    pub duration_ms: u64,
    pub release: String,
    pub process_instance: ProcessInstanceId,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShutdownRecord {
    pub severity: Severity,
    pub fact: ShutdownFact,
}
