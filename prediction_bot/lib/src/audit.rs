//! Typed, sanitized operational audit events.

use std::{sync::Arc, time::Duration};

use serenity::http::HttpError;

use crate::types::{ApplicationId, ChannelId, ConfigurationVersion, EventRevision, GuildId};
use crate::{
    domain::{Command, DomainError},
    store::StoreError,
};

mod logging;

use logging::LoggingListener;

pub trait AuditListener: Send + Sync {
    fn on_event(&self, event: &AuditEvent);
}

pub type SharedAudit = Arc<dyn AuditListener>;

#[must_use]
pub fn logging_listener() -> SharedAudit {
    Arc::new(LoggingListener)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Validate,
    Acquire,
    Replay,
    Decide,
    Append,
    Commit,
    Refresh,
    Query,
    Acknowledge,
    Deliver,
    Discover,
    Reconstruct,
    Register,
    Migrate,
    Startup,
    Ready,
    ShutdownRequested,
    GatewayShutdown,
    GrantWorkerShutdown,
    AnnouncementWorker,
    AnnouncementWorkerShutdown,
    GatewayLockRelease,
    Shutdown,
}

impl Stage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Validate => "validate",
            Self::Acquire => "acquire",
            Self::Replay => "replay",
            Self::Decide => "decide",
            Self::Append => "append",
            Self::Commit => "commit",
            Self::Refresh => "refresh",
            Self::Query => "query",
            Self::Acknowledge => "acknowledge",
            Self::Deliver => "deliver",
            Self::Discover => "discover",
            Self::Reconstruct => "reconstruct",
            Self::Register => "register",
            Self::Migrate => "migrate",
            Self::Startup => "startup",
            Self::Ready => "ready",
            Self::ShutdownRequested => "shutdown_requested",
            Self::GatewayShutdown => "gateway_shutdown",
            Self::GrantWorkerShutdown => "grant_worker_shutdown",
            Self::AnnouncementWorker => "announcement_worker",
            Self::AnnouncementWorkerShutdown => "announcement_worker_shutdown",
            Self::GatewayLockRelease => "gateway_lock_release",
            Self::Shutdown => "shutdown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureCategory {
    Configuration,
    Database,
    Connection,
    PoolTimeout,
    Constraint,
    History,
    Metadata,
    Overflow,
    Timeout,
    Transport,
    Discord,
    Unknown,
}

impl FailureCategory {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::Database => "database",
            Self::Connection => "connection",
            Self::PoolTimeout => "pool_timeout",
            Self::Constraint => "constraint",
            Self::History => "history",
            Self::Metadata => "metadata",
            Self::Overflow => "overflow",
            Self::Timeout => "timeout",
            Self::Transport => "transport",
            Self::Discord => "discord",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub category: FailureCategory,
    pub sqlstate: Option<String>,
    pub http_status: Option<u16>,
    pub discord_code: Option<isize>,
}

impl Failure {
    pub(crate) const fn category(category: FailureCategory) -> Self {
        Self {
            category,
            sqlstate: None,
            http_status: None,
            discord_code: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    InsufficientPoints,
    NotEnrolled,
    MarketUnavailable,
    PermissionDenied,
    InvalidInput,
}

impl Rejection {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InsufficientPoints => "insufficient_points",
            Self::NotEnrolled => "not_enrolled",
            Self::MarketUnavailable => "market_unavailable",
            Self::PermissionDenied => "permission_denied",
            Self::InvalidInput => "invalid_input",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Rejected(Rejection),
    Failed(Failure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Join,
    Create,
    Bet,
    Resolve,
    Cancel,
    Grant,
}

impl CommandKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Join => "join",
            Self::Create => "create",
            Self::Bet => "bet",
            Self::Resolve => "resolve",
            Self::Cancel => "cancel",
            Self::Grant => "grant",
        }
    }
}

impl From<&Command> for CommandKind {
    fn from(command: &Command) -> Self {
        match command {
            Command::Join => Self::Join,
            Command::Create { .. } => Self::Create,
            Command::Bet { .. } => Self::Bet,
            Command::Resolve { .. } => Self::Resolve,
            Command::Cancel { .. } => Self::Cancel,
            Command::Grant { .. } => Self::Grant,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryKind {
    Balance,
    Leaderboard,
    List,
    Show,
    Component,
}

impl QueryKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Balance => "balance",
            Self::Leaderboard => "leaderboard",
            Self::List => "list",
            Self::Show => "show",
            Self::Component => "component",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleKind {
    Migration,
    Startup,
    Ready,
    Shutdown,
}

impl LifecycleKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Migration => "migration",
            Self::Startup => "startup",
            Self::Ready => "ready",
            Self::Shutdown => "shutdown",
        }
    }
}

/// The selected handling decision; a concurrent configuration change can supersede it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnouncementDecision {
    NotSent,
    Delivered,
    Retry,
    Pause,
}
impl AnnouncementDecision {
    const fn as_str(self) -> &'static str {
        match self {
            Self::NotSent => "not_sent",
            Self::Delivered => "delivered",
            Self::Retry => "retry",
            Self::Pause => "pause",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditEvent {
    AnnouncementAttemptCompleted {
        guild: GuildId,
        revision: EventRevision,
        channel_id: ChannelId,
        configuration_version: ConfigurationVersion,
        decision: AnnouncementDecision,
        outcome: Outcome,
        stage: Stage,
    },
    AnnouncementWorkerFailed {
        outcome: Outcome,
        stage: Stage,
    },
    CommandCompleted {
        guild: GuildId,
        key: Option<String>,
        command: CommandKind,
        outcome: Outcome,
        stage: Stage,
        elapsed: Duration,
    },
    QueryCompleted {
        guild: GuildId,
        interaction_id: u64,
        query: QueryKind,
        outcome: Outcome,
        stage: Stage,
        elapsed: Duration,
    },
    InteractionCompleted {
        guild: Option<GuildId>,
        interaction_id: u64,
        outcome: Outcome,
        stage: Stage,
    },
    MentionReplyCompleted {
        guild: GuildId,
        channel_id: ChannelId,
        message_id: u64,
        outcome: Outcome,
        stage: Stage,
    },
    GrantFailed {
        guild: Option<GuildId>,
        outcome: Outcome,
        stage: Stage,
    },
    RegistrationCompleted {
        guild: GuildId,
        outcome: Outcome,
        stage: Stage,
    },
    Lifecycle {
        kind: LifecycleKind,
        application_id: Option<ApplicationId>,
        outcome: Outcome,
        stage: Stage,
    },
}

/// Return a copy only when a command key has a canonical, typed correlation shape.
#[must_use]
pub fn canonical_command_key(key: &str) -> Option<String> {
    if let Some(id) = key.strip_prefix("discord:") {
        let parsed = id.parse::<u64>().ok()?;
        return (parsed != 0 && parsed.to_string() == id).then(|| key.to_owned());
    }

    let grant = key.strip_prefix("grant:")?;
    let (user_text, boundary_text) = grant.split_once(':')?;
    let user = user_text.parse::<u64>().ok()?;
    let boundary = boundary_text.parse::<i64>().ok()?;
    (user != 0 && user.to_string() == user_text && boundary.to_string() == boundary_text)
        .then(|| key.to_owned())
}

fn rejection(reason: &str) -> Rejection {
    match reason {
        "insufficient points" => Rejection::InsufficientPoints,
        "member not enrolled" | "grant to unknown member" => Rejection::NotEnrolled,
        "unknown market"
        | "market is not open for betting"
        | "market is not ready to resolve"
        | "market already terminal" => Rejection::MarketUnavailable,
        "moderator required"
        | "market creator or moderator required"
        | "bots cannot bet"
        | "bots cannot create markets" => Rejection::PermissionDenied,
        _ => Rejection::InvalidInput,
    }
}

fn safe_sqlstate(code: &str) -> Option<String> {
    (code.len() == 5
        && code
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()))
    .then(|| code.to_owned())
}

fn database_failure(error: &sqlx::Error) -> Failure {
    match error {
        sqlx::Error::Configuration(_) | sqlx::Error::ConfigFile(_) => {
            Failure::category(FailureCategory::Configuration)
        }
        sqlx::Error::Database(error) => {
            let sqlstate = error.code().as_deref().and_then(safe_sqlstate);
            let category = match sqlstate.as_deref() {
                Some(code) if code.starts_with("08") => FailureCategory::Connection,
                Some(code) if code.starts_with("23") => FailureCategory::Constraint,
                Some("57014") => FailureCategory::Timeout,
                _ => FailureCategory::Database,
            };
            Failure {
                category,
                sqlstate,
                http_status: None,
                discord_code: None,
            }
        }
        sqlx::Error::PoolTimedOut => Failure::category(FailureCategory::PoolTimeout),
        sqlx::Error::Io(_)
        | sqlx::Error::Tls(_)
        | sqlx::Error::PoolClosed
        | sqlx::Error::WorkerCrashed
        | sqlx::Error::BeginFailed => Failure::category(FailureCategory::Connection),
        _ => Failure::category(FailureCategory::Database),
    }
}

#[must_use]
pub fn store_outcome(stage: Stage, error: &StoreError) -> Outcome {
    if stage == Stage::Replay && matches!(error, StoreError::Domain(_)) {
        return Outcome::Failed(Failure::category(FailureCategory::History));
    }

    match error {
        StoreError::Domain(DomainError::Invalid(reason)) if stage == Stage::Decide => {
            Outcome::Rejected(rejection(reason))
        }
        StoreError::Domain(DomainError::Invalid(_)) => {
            Outcome::Failed(Failure::category(FailureCategory::History))
        }
        StoreError::Domain(DomainError::Overflow) => {
            Outcome::Failed(Failure::category(FailureCategory::Overflow))
        }
        StoreError::Database(error) => Outcome::Failed(database_failure(error)),
        StoreError::Migration(_) => Outcome::Failed(Failure::category(FailureCategory::Database)),
        StoreError::Metadata(_) => Outcome::Failed(Failure::category(FailureCategory::Metadata)),
        StoreError::History(_) => Outcome::Failed(Failure::category(FailureCategory::History)),
        StoreError::Configuration(_) => {
            Outcome::Failed(Failure::category(FailureCategory::Configuration))
        }
    }
}

#[must_use]
pub fn discord_failure(error: &serenity::Error) -> Failure {
    match error {
        serenity::Error::Http(HttpError::UnsuccessfulRequest(response)) => Failure {
            category: FailureCategory::Discord,
            sqlstate: None,
            http_status: Some(response.status_code.as_u16()),
            discord_code: Some(response.error.code),
        },
        serenity::Error::Http(HttpError::Request(error)) if error.is_timeout() => {
            Failure::category(FailureCategory::Timeout)
        }
        serenity::Error::Http(HttpError::Request(_))
        | serenity::Error::Io(_)
        | serenity::Error::Gateway(_)
        | serenity::Error::Tungstenite(_) => Failure::category(FailureCategory::Transport),
        serenity::Error::Http(_) => Failure::category(FailureCategory::Discord),
        _ => Failure::category(FailureCategory::Unknown),
    }
}
