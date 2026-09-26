pub mod http;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use uuid::Uuid;

pub mod logging;
pub use logging::{LoggingListener, Severity, StructuredRecord};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct RequestId(Uuid);

impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct OperationId(Uuid);

impl OperationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObservationContext {
    pub occurred_at: DateTime<Utc>,
    pub request_id: Option<RequestId>,
    pub operation_id: OperationId,
    pub parent_operation_id: Option<OperationId>,
    pub duration: Option<Duration>,
}

impl ObservationContext {
    pub fn new() -> Self {
        Self {
            occurred_at: Utc::now(),
            request_id: None,
            operation_id: OperationId::new(),
            parent_operation_id: None,
            duration: None,
        }
    }

    pub fn for_request(request_id: RequestId) -> Self {
        Self {
            request_id: Some(request_id),
            ..Self::new()
        }
    }

    pub fn child_of(parent: &Self) -> Self {
        Self {
            request_id: parent.request_id,
            parent_operation_id: Some(parent.operation_id),
            ..Self::new()
        }
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }
}

impl Default for ObservationContext {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub context: ObservationContext,
    pub fact: Fact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupStage {
    Configuration,
    Binding,
    Connection,
    Migration,
    Composition,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Succeeded,
    Rejected,
    Failed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Channel {
    Web,
    Api,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthenticationOutcome {
    Accepted,
    Rejected,
    Failed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessReason {
    Missing,
    Invalid,
    Expired,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationKind {
    Create,
    Update,
    Delete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationOrigin {
    Standalone,
    Bulk,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MutationOutcome {
    Committed,
    Rejected,
    Failed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BulkOperation {
    Import,
    Delete,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BulkOutcome {
    Succeeded,
    Rejected,
    Partial,
    Failed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportStage {
    Query,
    Serialization,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Other,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    Health,
    WebLogin,
    WebNames,
    WebName,
    WebBulk,
    WebExport,
    ApiLogin,
    ApiNames,
    ApiName,
    ApiExport,
    Other,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestOutcome {
    Completed,
    Aborted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShutdownOutcome {
    Drained,
    TimedOut,
    Failed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureCategory {
    Duplicate,
    MissingEntry,
    NoRowsAffected,
    MalformedInput,
    Database,
    Serialization,
    Template,
    Token,
    Configuration,
    Bind,
    InvalidCredentials,
    Internal,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fact {
    StartupStageFinished {
        stage: StartupStage,
        outcome: Outcome,
        category: Option<FailureCategory>,
    },
    ApplicationReady,
    AuthenticationFinished {
        channel: Channel,
        outcome: AuthenticationOutcome,
        category: Option<FailureCategory>,
    },
    AccessDenied {
        channel: Channel,
        reason: AccessReason,
    },
    NameMutationFinished {
        kind: MutationKind,
        origin: MutationOrigin,
        outcome: MutationOutcome,
        category: Option<FailureCategory>,
    },
    BulkOperationFinished {
        operation: BulkOperation,
        attempted: u64,
        succeeded: u64,
        skipped: u64,
        failed: u64,
        input_count: Option<u64>,
        outcome: BulkOutcome,
        categories: Vec<(FailureCategory, u64)>,
    },
    NamesReadFinished {
        filter_present: bool,
        result_count: Option<u64>,
        outcome: Outcome,
        category: Option<FailureCategory>,
    },
    ExportPrepared {
        entry_count: Option<u64>,
        prepared_bytes: Option<u64>,
        outcome: Outcome,
        stage: ExportStage,
        category: Option<FailureCategory>,
    },
    RequestFinished {
        route: Route,
        method: Method,
        status: u16,
        outcome: RequestOutcome,
    },
    ShutdownFinished {
        outcome: ShutdownOutcome,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct DeliveryError;

pub trait ObservationSink: Send + Sync {
    fn record(&self, event: &Observation);
}

pub trait ObservationListener: Send + Sync {
    fn on_event(&self, event: &Observation) -> Result<(), DeliveryError>;
    fn flush(&self) -> Result<(), DeliveryError>;
}

pub type SharedObserver = Arc<dyn ObservationSink>;

pub struct Dispatcher {
    listeners: Vec<Arc<dyn ObservationListener>>,
    diagnostic: Arc<dyn Fn() + Send + Sync>,
    last_diagnostic: Mutex<Option<Instant>>,
}

impl Dispatcher {
    pub fn new(listeners: Vec<Arc<dyn ObservationListener>>) -> Self {
        Self::with_diagnostic(
            listeners,
            Arc::new(|| tracing::error!("observation listener delivery failed")),
        )
    }

    pub fn with_diagnostic(
        listeners: Vec<Arc<dyn ObservationListener>>,
        diagnostic: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            listeners,
            diagnostic,
            last_diagnostic: Mutex::new(None),
        }
    }

    fn report_failure(&self) {
        let now = Instant::now();
        let mut last = self
            .last_diagnostic
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if last.is_none_or(|then| now.duration_since(then) >= Duration::from_secs(60)) {
            *last = Some(now);
            drop(last);
            let _ = catch_unwind(AssertUnwindSafe(|| (self.diagnostic)()));
        }
    }

    pub fn flush(&self) {
        for listener in &self.listeners {
            if !matches!(
                catch_unwind(AssertUnwindSafe(|| listener.flush())),
                Ok(Ok(()))
            ) {
                self.report_failure();
            }
        }
    }
}

impl ObservationSink for Dispatcher {
    fn record(&self, event: &Observation) {
        for listener in &self.listeners {
            if !matches!(
                catch_unwind(AssertUnwindSafe(|| listener.on_event(event))),
                Ok(Ok(()))
            ) {
                self.report_failure();
            }
        }
    }
}
