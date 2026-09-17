use std::time::Duration;

use super::{AuditEvent, AuditListener, FailureCategory, Outcome, Stage};

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

fn level(stage: Stage, outcome: &Outcome) -> Level {
    match outcome {
        Outcome::Succeeded | Outcome::Rejected(_) => Level::Info,
        Outcome::Failed(failure)
            if matches!(stage, Stage::Acknowledge | Stage::Deliver)
                || failure.category == FailureCategory::Timeout =>
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
    ($level:expr, $event_name:literal, $fields:ident, $($name:literal = $value:expr),+ $(,)?) => {
        match $level {
            Level::Info => tracing::info!(
                "event.name" = $event_name,
                "outcome.kind" = $fields.kind,
                "outcome.rejection" = $fields.rejection,
                "failure.category" = $fields.failure_category,
                "failure.sqlstate" = $fields.sqlstate,
                "failure.http_status" = $fields.http_status,
                "failure.discord_code" = $fields.discord_code,
                $($name = $value),+
            ),
            Level::Warn => tracing::warn!(
                "event.name" = $event_name,
                "outcome.kind" = $fields.kind,
                "outcome.rejection" = $fields.rejection,
                "failure.category" = $fields.failure_category,
                "failure.sqlstate" = $fields.sqlstate,
                "failure.http_status" = $fields.http_status,
                "failure.discord_code" = $fields.discord_code,
                $($name = $value),+
            ),
            Level::Error => tracing::error!(
                "event.name" = $event_name,
                "outcome.kind" = $fields.kind,
                "outcome.rejection" = $fields.rejection,
                "failure.category" = $fields.failure_category,
                "failure.sqlstate" = $fields.sqlstate,
                "failure.http_status" = $fields.http_status,
                "failure.discord_code" = $fields.discord_code,
                $($name = $value),+
            ),
        }
    };
}

impl AuditListener for LoggingListener {
    fn on_event(&self, event: &AuditEvent) {
        match event {
            AuditEvent::CommandCompleted {
                guild,
                key,
                command,
                outcome,
                stage,
                elapsed,
            } => {
                let fields = OutcomeFields::from(outcome);
                emit!(
                    level(*stage, outcome),
                    "command_completed",
                    fields,
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
                let fields = OutcomeFields::from(outcome);
                emit!(
                    level(*stage, outcome),
                    "query_completed",
                    fields,
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
                let fields = OutcomeFields::from(outcome);
                emit!(
                    level(*stage, outcome),
                    "interaction_completed",
                    fields,
                    "interaction.guild" = *guild,
                    "interaction.id" = *interaction_id,
                    "operation.stage" = stage.as_str(),
                );
            }
            AuditEvent::GrantFailed {
                guild,
                outcome,
                stage,
            } => {
                let fields = OutcomeFields::from(outcome);
                emit!(
                    level(*stage, outcome),
                    "grant_failed",
                    fields,
                    "grant.guild" = *guild,
                    "operation.stage" = stage.as_str(),
                );
            }
            AuditEvent::RegistrationCompleted {
                guild,
                outcome,
                stage,
            } => {
                let fields = OutcomeFields::from(outcome);
                emit!(
                    level(*stage, outcome),
                    "registration_completed",
                    fields,
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
                let fields = OutcomeFields::from(outcome);
                emit!(
                    level(*stage, outcome),
                    "lifecycle",
                    fields,
                    "lifecycle.kind" = kind.as_str(),
                    "application.id" = *application_id,
                    "operation.stage" = stage.as_str(),
                );
            }
        }
    }
}
