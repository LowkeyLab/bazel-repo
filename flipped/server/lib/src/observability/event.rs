use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    pub schema_version: u32,
    pub event_name: ServiceEventName,
    pub event_id: String,
    pub sequence: u64,
    pub occurred_at: String,
    pub severity: Severity,
    pub service: ServiceIdentity,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub request_id: Option<String>,
    pub command_id: Option<String>,
    pub causation_id: Option<String>,
    pub session_ref: Option<String>,
    pub event: ServiceEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceIdentity {
    pub name: String,
    pub version: String,
    pub environment: String,
    pub instance_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ServiceEventName {
    #[serde(rename = "service.started")]
    ServiceStarted,
    #[serde(rename = "service.ready")]
    ServiceReady,
    #[serde(rename = "service.stopping")]
    ServiceStopping,
    #[serde(rename = "service.stopped")]
    ServiceStopped,
    #[serde(rename = "import.started")]
    ImportStarted,
    #[serde(rename = "import.completed")]
    ImportCompleted,
    #[serde(rename = "import.rejected")]
    ImportRejected,
    #[serde(rename = "import.capacity_rejected")]
    ImportCapacityRejected,
    #[serde(rename = "session.created")]
    SessionCreated,
    #[serde(rename = "session.examiner_joined")]
    SessionExaminerJoined,
    #[serde(rename = "session.started")]
    SessionStarted,
    #[serde(rename = "session.advanced")]
    SessionAdvanced,
    #[serde(rename = "session.ended")]
    SessionEnded,
    #[serde(rename = "session.expired")]
    SessionExpired,
    #[serde(rename = "invitation.issued")]
    InvitationIssued,
    #[serde(rename = "invitation.exchanged")]
    InvitationExchanged,
    #[serde(rename = "invitation.rejected")]
    InvitationRejected,
    #[serde(rename = "invitation.revoked")]
    InvitationRevoked,
    #[serde(rename = "grpc.request_completed")]
    GrpcRequestCompleted,
    #[serde(rename = "oauth.request_completed")]
    OAuthRequestCompleted,
    #[serde(rename = "grpc.subscriber_lagged")]
    GrpcSubscriberLagged,
    #[serde(rename = "grpc.stream_cancelled")]
    GrpcStreamCancelled,
    #[serde(rename = "grpc.watch_rejected")]
    GrpcWatchRejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceEvent {
    pub name: ServiceEventName,
    pub outcome: Outcome,
    pub error_code: Option<EventErrorCode>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventErrorCode {
    #[serde(rename = "invalid_grant")]
    InvalidGrant,
    #[serde(rename = "RESYNC_REQUIRED")]
    ResyncRequired,
    #[serde(rename = "UPLOAD_TOO_LARGE")]
    UploadTooLarge,
    #[serde(rename = "UNSUPPORTED_EXTENSION")]
    UnsupportedExtension,
    #[serde(rename = "INVALID_ZIP")]
    InvalidZip,
    #[serde(rename = "ENTRY_COUNT_EXCEEDED")]
    EntryCountExceeded,
    #[serde(rename = "ENTRY_SIZE_EXCEEDED")]
    EntrySizeExceeded,
    #[serde(rename = "TOTAL_EXTRACTED_SIZE_EXCEEDED")]
    TotalExtractedSizeExceeded,
    #[serde(rename = "COMPRESSION_RATIO_EXCEEDED")]
    CompressionRatioExceeded,
    #[serde(rename = "MISSING_COLLECTION_DATABASE")]
    MissingCollectionDatabase,
    #[serde(rename = "UNSUPPORTED_PACKAGE_VERSION")]
    UnsupportedPackageVersion,
    #[serde(rename = "SQLITE_LIMIT_EXCEEDED")]
    SqliteLimitExceeded,
    #[serde(rename = "SQLITE_SCHEMA_INVALID")]
    SqliteSchemaInvalid,
    #[serde(rename = "NO_SUPPORTED_NOTES")]
    NoSupportedNotes,
    #[serde(rename = "TOO_MANY_CARDS")]
    TooManyCards,
    #[serde(rename = "CLOZE_REJECTED")]
    ClozeRejected,
    #[serde(rename = "MEDIA_REJECTED")]
    MediaRejected,
    #[serde(rename = "CUSTOM_TEMPLATE_REJECTED")]
    CustomTemplateRejected,
    #[serde(rename = "INVALID_UTF8")]
    InvalidUtf8,
    #[serde(rename = "EMPTY_FRONT")]
    EmptyFront,
    #[serde(rename = "EMPTY_BACK")]
    EmptyBack,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "RESOURCE_EXHAUSTED")]
    ResourceExhausted,
}

impl From<flipped_anki::ImportErrorCode> for EventErrorCode {
    fn from(value: flipped_anki::ImportErrorCode) -> Self {
        use flipped_anki::ImportErrorCode;
        match value {
            ImportErrorCode::UploadTooLarge => Self::UploadTooLarge,
            ImportErrorCode::UnsupportedExtension => Self::UnsupportedExtension,
            ImportErrorCode::InvalidZip => Self::InvalidZip,
            ImportErrorCode::EntryCountExceeded => Self::EntryCountExceeded,
            ImportErrorCode::EntrySizeExceeded => Self::EntrySizeExceeded,
            ImportErrorCode::TotalExtractedSizeExceeded => Self::TotalExtractedSizeExceeded,
            ImportErrorCode::CompressionRatioExceeded => Self::CompressionRatioExceeded,
            ImportErrorCode::MissingCollectionDatabase => Self::MissingCollectionDatabase,
            ImportErrorCode::UnsupportedPackageVersion => Self::UnsupportedPackageVersion,
            ImportErrorCode::SqliteLimitExceeded => Self::SqliteLimitExceeded,
            ImportErrorCode::SqliteSchemaInvalid => Self::SqliteSchemaInvalid,
            ImportErrorCode::NoSupportedNotes => Self::NoSupportedNotes,
            ImportErrorCode::TooManyCards => Self::TooManyCards,
            ImportErrorCode::ClozeRejected => Self::ClozeRejected,
            ImportErrorCode::MediaRejected => Self::MediaRejected,
            ImportErrorCode::CustomTemplateRejected => Self::CustomTemplateRejected,
            ImportErrorCode::InvalidUtf8 => Self::InvalidUtf8,
            ImportErrorCode::EmptyFront => Self::EmptyFront,
            ImportErrorCode::EmptyBack => Self::EmptyBack,
            ImportErrorCode::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Success,
    Rejected,
    Cancelled,
    Failure,
}

#[derive(Debug, Clone, Default)]
pub struct EventContext {
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub request_id: Option<String>,
    pub command_id: Option<String>,
    pub causation_id: Option<String>,
    pub session_ref: Option<String>,
}
