use crate::{Observation, ObservationDeliveryError, Observer, Outcome, SharedObserver};
use std::sync::Arc;

struct TracingObserver;

/// Uses the caller's current tracing subscriber and span; installs no global state.
#[must_use]
pub fn tracing_observer() -> SharedObserver {
    Arc::new(TracingObserver)
}

impl Observer for TracingObserver {
    fn observe(&self, observation: &Observation) -> Result<(), ObservationDeliveryError> {
        let correlation = observation.correlation_id.map(|id| id.to_string());
        let reader = observation.reader_id.map(|id| id.to_string());
        let event = observation.event_id.map(|id| id.to_string());
        let command = observation.command_kind.map(|kind| format!("{kind:?}"));
        let count = observation
            .event_count
            .and_then(|count| u64::try_from(count).ok());
        macro_rules! record {
            ($level:expr) => {
                tracing::event!(target: "book_smartz::storage", $level,
                    operation = ?observation.operation, outcome = ?observation.outcome,
                    command_kind = command.as_deref(),
                    duration_ms = observation.duration.as_secs_f64() * 1000.0,
                    correlation_id = correlation.as_deref(), reader_id = reader.as_deref(),
                    event_id = event.as_deref(), revision = observation.revision, event_count = count)
            }
        }
        match observation.outcome {
            Outcome::Applied | Outcome::Created | Outcome::Committed => {
                record!(tracing::Level::INFO)
            }
            Outcome::Failed(_) | Outcome::CommitUncertain => record!(tracing::Level::ERROR),
            _ => record!(tracing::Level::DEBUG),
        }
        Ok(())
    }
}
