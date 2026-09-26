use std::collections::BTreeMap;

use super::{
    AccessReason, AuthenticationOutcome, BulkOperation, BulkOutcome, Channel, DeliveryError,
    ExportStage, Fact, FailureCategory, Method, MutationKind, MutationOrigin, MutationOutcome,
    Observation, ObservationListener, Outcome, RequestOutcome, Route, ShutdownOutcome,
    StartupStage,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    Text(&'static str),
    Identifier(String),
    Timestamp(chrono::DateTime<chrono::Utc>),
    Count(u64),
    Flag(bool),
}

impl From<&'static str> for FieldValue {
    fn from(value: &'static str) -> Self {
        Self::Text(value)
    }
}

impl From<u64> for FieldValue {
    fn from(value: u64) -> Self {
        Self::Count(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructuredRecord {
    pub severity: Severity,
    pub event: &'static str,
    pub fields: BTreeMap<&'static str, FieldValue>,
}

pub struct LoggingListener;

impl LoggingListener {
    #[must_use]
    pub fn record_for(observation: &Observation) -> Option<StructuredRecord> {
        let fact = &observation.fact;
        if matches!(
            fact,
            Fact::RequestFinished {
                route: Route::Health,
                status: 200..=299,
                outcome: RequestOutcome::Completed,
                ..
            }
        ) {
            return None;
        }
        let mut fields = BTreeMap::new();
        fields.insert(
            "occurred_at",
            FieldValue::Timestamp(observation.context.occurred_at),
        );
        fields.insert(
            "operation_id",
            FieldValue::Identifier(observation.context.operation_id.to_string()),
        );
        if let Some(id) = observation.context.request_id {
            fields.insert("request_id", FieldValue::Identifier(id.to_string()));
        }
        if let Some(id) = observation.context.parent_operation_id {
            fields.insert(
                "parent_operation_id",
                FieldValue::Identifier(id.to_string()),
            );
        }
        if let Some(duration) = observation.context.duration {
            fields.insert(
                "duration_ms",
                FieldValue::Count(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)),
            );
        }
        let (event, severity) = match fact {
            Fact::StartupStageFinished {
                stage,
                outcome,
                category,
            } => {
                fields.insert("stage", stage.label().into());
                fields.insert("outcome", outcome.label().into());
                insert_category(&mut fields, *category);
                ("startup_stage_finished", severity_for_outcome(*outcome))
            }
            Fact::ApplicationReady => ("application_ready", Severity::Info),
            Fact::AuthenticationFinished {
                channel,
                outcome,
                category,
            } => {
                fields.insert("channel", channel.label().into());
                fields.insert("outcome", outcome.label().into());
                insert_category(&mut fields, *category);
                (
                    "authentication_finished",
                    if *outcome == AuthenticationOutcome::Failed {
                        Severity::Error
                    } else {
                        Severity::Info
                    },
                )
            }
            Fact::AccessDenied { channel, reason } => {
                fields.insert("channel", channel.label().into());
                fields.insert("reason", reason.label().into());
                ("access_denied", Severity::Info)
            }
            Fact::NameMutationFinished {
                kind,
                origin,
                outcome,
                category,
            } => {
                fields.insert("operation", kind.label().into());
                fields.insert("origin", origin.label().into());
                fields.insert("outcome", outcome.label().into());
                insert_category(&mut fields, *category);
                (
                    "name_mutation_finished",
                    if *outcome == MutationOutcome::Failed {
                        Severity::Error
                    } else {
                        Severity::Info
                    },
                )
            }
            Fact::BulkOperationFinished {
                operation,
                attempted,
                succeeded,
                skipped,
                failed,
                input_count,
                outcome,
                categories,
            } => {
                fields.insert("operation", operation.label().into());
                fields.insert("attempted", (*attempted).into());
                fields.insert("succeeded", (*succeeded).into());
                fields.insert("skipped", (*skipped).into());
                fields.insert("failed", (*failed).into());
                fields.insert("outcome", outcome.label().into());
                if let Some(count) = input_count {
                    fields.insert("input_count", (*count).into());
                }
                for (category, count) in categories {
                    fields.insert(category.count_field(), (*count).into());
                }
                (
                    "bulk_operation_finished",
                    match outcome {
                        BulkOutcome::Partial => Severity::Warn,
                        BulkOutcome::Failed => Severity::Error,
                        _ => Severity::Info,
                    },
                )
            }
            Fact::NamesReadFinished {
                filter_present,
                result_count,
                outcome,
                category,
            } => {
                fields.insert("filter_present", FieldValue::Flag(*filter_present));
                fields.insert("outcome", outcome.label().into());
                if let Some(count) = result_count {
                    fields.insert("result_count", (*count).into());
                }
                insert_category(&mut fields, *category);
                ("names_read_finished", severity_for_outcome(*outcome))
            }
            Fact::ExportPrepared {
                entry_count,
                prepared_bytes,
                outcome,
                stage,
                category,
            } => {
                fields.insert("stage", stage.label().into());
                fields.insert("outcome", outcome.label().into());
                if let Some(count) = entry_count {
                    fields.insert("entry_count", (*count).into());
                }
                if let Some(count) = prepared_bytes {
                    fields.insert("prepared_bytes", (*count).into());
                }
                insert_category(&mut fields, *category);
                ("export_prepared", severity_for_outcome(*outcome))
            }
            Fact::RequestFinished {
                route,
                method,
                status,
                outcome,
            } => {
                fields.insert("route", route.label().into());
                fields.insert("method", method.label().into());
                fields.insert("status", u64::from(*status).into());
                fields.insert("outcome", outcome.label().into());
                (
                    "request_finished",
                    if *status >= 500 {
                        Severity::Error
                    } else if *outcome == RequestOutcome::Aborted {
                        Severity::Warn
                    } else {
                        Severity::Info
                    },
                )
            }
            Fact::ShutdownFinished { outcome } => {
                fields.insert("outcome", outcome.label().into());
                (
                    "shutdown_finished",
                    match outcome {
                        ShutdownOutcome::Drained => Severity::Info,
                        ShutdownOutcome::TimedOut => Severity::Warn,
                        ShutdownOutcome::Failed => Severity::Error,
                    },
                )
            }
        };
        Some(StructuredRecord {
            severity,
            event,
            fields,
        })
    }
}

impl StructuredRecord {
    fn text(&self, key: &str) -> Option<&str> {
        match self.fields.get(key) {
            Some(FieldValue::Text(value)) => Some(value),
            Some(FieldValue::Identifier(value)) => Some(value),
            _ => None,
        }
    }

    fn count(&self, key: &str) -> Option<u64> {
        match self.fields.get(key) {
            Some(FieldValue::Count(value)) => Some(*value),
            _ => None,
        }
    }

    fn flag(&self, key: &str) -> Option<bool> {
        match self.fields.get(key) {
            Some(FieldValue::Flag(value)) => Some(*value),
            _ => None,
        }
    }
}

impl ObservationListener for LoggingListener {
    fn on_event(&self, event: &Observation) -> Result<(), DeliveryError> {
        let Some(record) = Self::record_for(event) else {
            return Ok(());
        };
        let occurred_at = event.context.occurred_at.to_rfc3339();
        // Static field names preserve native tracing value types. Optional values are
        // omitted by tracing; subscribers never have to parse a Debug-formatted map.
        macro_rules! emit {
            ($level:expr; $($key:ident = $value:expr),* $(,)?) => {
                tracing::event!(
                    $level,
                    event = record.event,
                    occurred_at = occurred_at.as_str(),
                    operation_id = record.text("operation_id"),
                    request_id = record.text("request_id"),
                    parent_operation_id = record.text("parent_operation_id"),
                    duration_ms = record.count("duration_ms"),
                    $($key = $value),*
                )
            };
        }
        macro_rules! emit_at_severity {
            ($($key:ident = $value:expr),* $(,)?) => {
                match record.severity {
                    Severity::Info => emit!(tracing::Level::INFO; $($key = $value),*),
                    Severity::Warn => emit!(tracing::Level::WARN; $($key = $value),*),
                    Severity::Error => emit!(tracing::Level::ERROR; $($key = $value),*),
                }
            };
        }
        if matches!(event.fact, Fact::BulkOperationFinished { .. }) {
            emit_at_severity!(
                operation = record.text("operation"),
                outcome = record.text("outcome"),
                attempted = record.count("attempted"),
                succeeded = record.count("succeeded"),
                skipped = record.count("skipped"),
                failed = record.count("failed"),
                input_count = record.count("input_count"),
                duplicate_count = record.count("duplicate_count"),
                missing_entry_count = record.count("missing_entry_count"),
                no_rows_affected_count = record.count("no_rows_affected_count"),
                malformed_input_count = record.count("malformed_input_count"),
                database_count = record.count("database_count"),
                serialization_count = record.count("serialization_count"),
                template_count = record.count("template_count"),
                token_count = record.count("token_count"),
                configuration_count = record.count("configuration_count"),
                bind_count = record.count("bind_count"),
                invalid_credentials_count = record.count("invalid_credentials_count"),
                internal_count = record.count("internal_count"),
            );
        } else {
            emit_at_severity!(
                stage = record.text("stage"),
                outcome = record.text("outcome"),
                category = record.text("category"),
                channel = record.text("channel"),
                reason = record.text("reason"),
                operation = record.text("operation"),
                origin = record.text("origin"),
                route = record.text("route"),
                method = record.text("method"),
                status = record.count("status"),
                result_count = record.count("result_count"),
                entry_count = record.count("entry_count"),
                prepared_bytes = record.count("prepared_bytes"),
                filter_present = record.flag("filter_present"),
            );
        }
        Ok(())
    }

    fn flush(&self) -> Result<(), DeliveryError> {
        // The local fmt writer is synchronous; there is no listener-owned queue.
        Ok(())
    }
}

fn insert_category(
    fields: &mut BTreeMap<&'static str, FieldValue>,
    category: Option<FailureCategory>,
) {
    if let Some(category) = category {
        fields.insert("category", category.label().into());
    }
}

fn severity_for_outcome(outcome: Outcome) -> Severity {
    if outcome == Outcome::Failed {
        Severity::Error
    } else {
        Severity::Info
    }
}

macro_rules! labels {
    ($type:ty { $($variant:ident => $text:literal),+ $(,)? }) => {
        impl $type {
            pub fn label(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }
    };
}

labels!(StartupStage { Configuration => "configuration", Binding => "binding", Connection => "connection", Migration => "migration", Composition => "composition" });
labels!(Outcome { Succeeded => "succeeded", Rejected => "rejected", Failed => "failed" });
labels!(Channel { Web => "web", Api => "api" });
labels!(AuthenticationOutcome { Accepted => "accepted", Rejected => "rejected", Failed => "failed" });
labels!(AccessReason { Missing => "missing", Invalid => "invalid", Expired => "expired" });
labels!(MutationKind { Create => "create", Update => "update", Delete => "delete" });
labels!(MutationOrigin { Standalone => "standalone", Bulk => "bulk" });
labels!(MutationOutcome { Committed => "committed", Rejected => "rejected", Failed => "failed" });
labels!(BulkOperation { Import => "import", Delete => "delete" });
labels!(BulkOutcome { Succeeded => "succeeded", Rejected => "rejected", Partial => "partial", Failed => "failed" });
labels!(ExportStage { Query => "query", Serialization => "serialization" });
labels!(Method { Get => "get", Post => "post", Put => "put", Patch => "patch", Delete => "delete", Other => "other" });
labels!(Route { Health => "health", WebLogin => "web_login", WebNames => "web_names", WebName => "web_name", WebBulk => "web_bulk", WebExport => "web_export", ApiLogin => "api_login", ApiNames => "api_names", ApiName => "api_name", ApiExport => "api_export", Other => "other" });
labels!(RequestOutcome { Completed => "completed", Aborted => "aborted" });
labels!(ShutdownOutcome { Drained => "drained", TimedOut => "timed_out", Failed => "failed" });
labels!(FailureCategory { Duplicate => "duplicate", MissingEntry => "missing_entry", NoRowsAffected => "no_rows_affected", MalformedInput => "malformed_input", Database => "database", Serialization => "serialization", Template => "template", Token => "token", Configuration => "configuration", Bind => "bind", InvalidCredentials => "invalid_credentials", Internal => "internal" });

impl FailureCategory {
    fn count_field(self) -> &'static str {
        match self {
            Self::Duplicate => "duplicate_count",
            Self::MissingEntry => "missing_entry_count",
            Self::NoRowsAffected => "no_rows_affected_count",
            Self::MalformedInput => "malformed_input_count",
            Self::Database => "database_count",
            Self::Serialization => "serialization_count",
            Self::Template => "template_count",
            Self::Token => "token_count",
            Self::Configuration => "configuration_count",
            Self::Bind => "bind_count",
            Self::InvalidCredentials => "invalid_credentials_count",
            Self::Internal => "internal_count",
        }
    }
}
