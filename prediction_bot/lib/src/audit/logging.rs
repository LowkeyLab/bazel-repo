use std::time::Duration;

use super::{AuditEvent, AuditListener, FailureCategory, LifecycleKind, Outcome, Stage};

pub(super) struct LoggingListener;

#[derive(Clone, Copy)]
enum Level {
    Info,
    Warn,
    Error,
}

struct OutcomeFields<'a> {
    kind: &'static str,
    rejection: Option<&'static str>,
    failure_category: Option<&'static str>,
    sqlstate: Option<&'a str>,
    http_status: Option<u16>,
    discord_code: Option<isize>,
}

impl<'a> From<&'a Outcome> for OutcomeFields<'a> {
    fn from(outcome: &'a Outcome) -> Self {
        match outcome {
            Outcome::Succeeded => Self {
                kind: "succeeded",
                rejection: None,
                failure_category: None,
                sqlstate: None,
                http_status: None,
                discord_code: None,
            },
            Outcome::Rejected(rejection) => Self {
                kind: "rejected",
                rejection: Some(rejection.as_str()),
                failure_category: None,
                sqlstate: None,
                http_status: None,
                discord_code: None,
            },
            Outcome::Failed(failure) => Self {
                kind: "failed",
                rejection: None,
                failure_category: Some(failure.category.as_str()),
                sqlstate: failure.sqlstate.as_deref(),
                http_status: failure.http_status,
                discord_code: failure.discord_code,
            },
        }
    }
}

fn level(event: &AuditEvent, outcome: &Outcome) -> Level {
    match outcome {
        Outcome::Succeeded | Outcome::Rejected(_) => Level::Info,
        Outcome::Failed(_)
            if matches!(
                event,
                AuditEvent::InteractionCompleted {
                    stage: Stage::Acknowledge | Stage::Deliver,
                    ..
                } | AuditEvent::AnnouncementAttemptCompleted {
                    stage: Stage::Deliver,
                    ..
                } | AuditEvent::MentionReplyCompleted {
                    stage: Stage::Deliver,
                    ..
                }
            ) =>
        {
            Level::Warn
        }
        Outcome::Failed(failure)
            if failure.category == FailureCategory::Timeout
                && failure.sqlstate.is_none()
                && matches!(
                    event,
                    AuditEvent::QueryCompleted {
                        stage: Stage::Query,
                        ..
                    } | AuditEvent::Lifecycle {
                        kind: LifecycleKind::Shutdown,
                        stage: Stage::GatewayShutdown
                            | Stage::GrantWorkerShutdown
                            | Stage::AnnouncementWorkerShutdown,
                        ..
                    }
                ) =>
        {
            Level::Warn
        }
        Outcome::Failed(_) => Level::Error,
    }
}

fn elapsed_millis(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

macro_rules! emit {
    ($level:expr, $event_name:literal, $outcome:expr, $($name:literal = $value:expr),+ $(,)?) => {{
        let fields = OutcomeFields::from($outcome);
        match $level {
            Level::Info => tracing::info!(
                "event.name" = $event_name,
                "outcome.kind" = fields.kind,
                "outcome.rejection" = fields.rejection,
                "failure.category" = fields.failure_category,
                "failure.sqlstate" = fields.sqlstate,
                "failure.http_status" = fields.http_status,
                "failure.discord_code" = fields.discord_code,
                $($name = $value),+
            ),
            Level::Warn => tracing::warn!(
                "event.name" = $event_name,
                "outcome.kind" = fields.kind,
                "outcome.rejection" = fields.rejection,
                "failure.category" = fields.failure_category,
                "failure.sqlstate" = fields.sqlstate,
                "failure.http_status" = fields.http_status,
                "failure.discord_code" = fields.discord_code,
                $($name = $value),+
            ),
            Level::Error => tracing::error!(
                "event.name" = $event_name,
                "outcome.kind" = fields.kind,
                "outcome.rejection" = fields.rejection,
                "failure.category" = fields.failure_category,
                "failure.sqlstate" = fields.sqlstate,
                "failure.http_status" = fields.http_status,
                "failure.discord_code" = fields.discord_code,
                $($name = $value),+
            ),
        }
    }};
}

impl AuditListener for LoggingListener {
    fn on_event(&self, event: &AuditEvent) {
        match event {
            AuditEvent::AnnouncementAttemptCompleted {
                guild,
                revision,
                channel_id,
                configuration_version,
                decision,
                outcome,
                stage,
            } => {
                emit!(
                    level(event, outcome),
                    "announcement_attempt_completed",
                    outcome,
                    "announcement.guild" = *guild,
                    "announcement.revision" = *revision,
                    "announcement.channel_id" = *channel_id,
                    "announcement.configuration_version" = *configuration_version,
                    "announcement.decision" = decision.as_str(),
                    "operation.stage" = stage.as_str(),
                );
            }
            AuditEvent::AnnouncementWorkerFailed { outcome, stage } => {
                emit!(
                    level(event, outcome),
                    "announcement_worker_failed",
                    outcome,
                    "operation.stage" = stage.as_str(),
                );
            }
            AuditEvent::CommandCompleted {
                guild,
                key,
                command,
                outcome,
                stage,
                elapsed,
            } => {
                emit!(
                    level(event, outcome),
                    "command_completed",
                    outcome,
                    "command.kind" = command.as_str(),
                    "command.guild" = *guild,
                    "command.key" = key.as_deref(),
                    "operation.stage" = stage.as_str(),
                    "operation.elapsed_ms" = elapsed_millis(*elapsed),
                );
            }
            AuditEvent::QueryCompleted {
                guild,
                interaction_id,
                query,
                outcome,
                stage,
                elapsed,
            } => {
                emit!(
                    level(event, outcome),
                    "query_completed",
                    outcome,
                    "query.kind" = query.as_str(),
                    "query.guild" = *guild,
                    "interaction.id" = *interaction_id,
                    "operation.stage" = stage.as_str(),
                    "operation.elapsed_ms" = elapsed_millis(*elapsed),
                );
            }
            AuditEvent::InteractionCompleted {
                guild,
                interaction_id,
                outcome,
                stage,
            } => {
                emit!(
                    level(event, outcome),
                    "interaction_completed",
                    outcome,
                    "interaction.guild" = *guild,
                    "interaction.id" = *interaction_id,
                    "operation.stage" = stage.as_str(),
                );
            }
            AuditEvent::MentionReplyCompleted {
                guild,
                channel_id,
                message_id,
                outcome,
                stage,
            } => {
                emit!(
                    level(event, outcome),
                    "mention_reply_completed",
                    outcome,
                    "message.guild" = *guild,
                    "message.channel_id" = *channel_id,
                    "message.id" = *message_id,
                    "operation.stage" = stage.as_str(),
                );
            }
            AuditEvent::GrantFailed {
                guild,
                outcome,
                stage,
            } => {
                emit!(
                    level(event, outcome),
                    "grant_failed",
                    outcome,
                    "grant.guild" = *guild,
                    "operation.stage" = stage.as_str(),
                );
            }
            AuditEvent::RegistrationCompleted {
                guild,
                outcome,
                stage,
            } => {
                emit!(
                    level(event, outcome),
                    "registration_completed",
                    outcome,
                    "registration.guild" = *guild,
                    "operation.stage" = stage.as_str(),
                );
            }
            AuditEvent::Lifecycle {
                kind,
                application_id,
                outcome,
                stage,
            } => {
                emit!(
                    level(event, outcome),
                    "lifecycle",
                    outcome,
                    "lifecycle.kind" = kind.as_str(),
                    "application.id" = *application_id,
                    "operation.stage" = stage.as_str(),
                );
            }
        }
    }
}
