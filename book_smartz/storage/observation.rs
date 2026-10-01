use std::{sync::Arc, time::Duration};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Migration,
    Registration,
    BookById,
    BookByWorkId,
    RankingLoad,
    RankingCommand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKind {
    Start,
    Answer,
    Pause,
    Resume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    IdentityConflict,
    StaleRevision,
    DuplicateEventId,
    InvalidTransition,
    UnknownBook,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Rejected,
    CorruptHistory,
    Database,
    Migration,
    CommitUncertain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied,
    Created,
    AlreadyPresent,
    Found,
    Absent,
    Loaded,
    Committed,
    Rejected(Rejection),
    Failed(Failure),
    CommitUncertain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub operation: Operation,
    pub command_kind: Option<CommandKind>,
    pub outcome: Outcome,
    pub duration: Duration,
    pub correlation_id: Option<Uuid>,
    pub reader_id: Option<Uuid>,
    pub event_id: Option<Uuid>,
    pub revision: Option<u64>,
    pub event_count: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
#[error("observation delivery failed")]
pub struct ObservationDeliveryError;

/// Synchronous best-effort diagnostics. Implementations should return promptly and not panic.
pub trait Observer: Send + Sync {
    /// Delivers one completed operation fact.
    ///
    /// # Errors
    /// Returns a bounded delivery failure without exposing the sink cause.
    fn observe(&self, observation: &Observation) -> Result<(), ObservationDeliveryError>;
}

pub type SharedObserver = Arc<dyn Observer>;

pub(crate) fn dispatch(observer: &SharedObserver, observation: &Observation) {
    dispatch_with_fallback(observer, observation, |message| {
        use std::io::Write;
        let _ = std::io::stderr().write_all(message);
    });
}

fn dispatch_with_fallback(
    observer: &SharedObserver,
    observation: &Observation,
    fallback: impl FnOnce(&'static [u8]),
) {
    // The caller owns the panic hook. Unwind containment cannot silence it or catch aborts.
    let delivered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        observer.observe(observation)
    }));
    if !matches!(delivered, Ok(Ok(()))) {
        fallback(b"book_smartz observation delivery failed\n");
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Observation, ObservationDeliveryError, Observer, Operation, Outcome, SharedObserver,
        dispatch_with_fallback,
    };
    use googletest::{assert_that, matchers::eq};
    use std::{sync::Arc, time::Duration};
    struct Sink(u8);
    impl Observer for Sink {
        fn observe(&self, _: &Observation) -> Result<(), ObservationDeliveryError> {
            match self.0 {
                0 => Ok(()),
                1 => Err(ObservationDeliveryError),
                _ => panic!("SECRET_RAW_PAYLOAD"),
            }
        }
    }
    #[googletest::test]
    fn fallback_is_once_only_after_failed_delivery() {
        for mode in 0..3 {
            let observer: SharedObserver = Arc::new(Sink(mode));
            let observation = Observation {
                operation: Operation::Migration,
                command_kind: None,
                outcome: Outcome::Applied,
                duration: Duration::ZERO,
                correlation_id: None,
                reader_id: None,
                event_id: None,
                revision: None,
                event_count: None,
            };
            let count = std::cell::Cell::new(0);
            dispatch_with_fallback(&observer, &observation, |message| {
                count.set(count.get() + 1);
                assert_that!(message.len() < 80, eq(true));
                assert_that!(
                    String::from_utf8_lossy(message).contains("SECRET_RAW_PAYLOAD"),
                    eq(false)
                );
            });
            assert_that!(count.get(), eq(i32::from(mode != 0)));
        }
    }
}
