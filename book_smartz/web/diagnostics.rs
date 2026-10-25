//! Privacy allowlist at the application boundary; delivery never blocks business work.
use std::{
    io::Write,
    sync::{Arc, mpsc},
    time::Instant,
};

use serde::Serialize;

use crate::actions::ActionId;
use crate::diagnostic_events::{
    EventIdentity, OperationFinished, OperationId, OperationKind, OperationOutcome,
    OperationRecord, ProcessInstanceId, Reason, Severity, Stage,
};

/// Listeners receive only the immutable allowlisted fact.
pub trait OperationListener: Send + Sync {
    fn observe(&self, fact: OperationFinished);
}

struct StderrListener(mpsc::SyncSender<Delivery>);
impl OperationListener for StderrListener {
    fn observe(&self, fact: OperationFinished) {
        let _ = self.0.try_send(Delivery::Operation(operation_record(fact)));
    }
}

const fn operation_record(fact: OperationFinished) -> OperationRecord {
    let severity = match (fact.outcome, fact.reason) {
        (
            OperationOutcome::DependencyFailure,
            Reason::Database | Reason::Schema | Reason::SelectionPreparation,
        ) => Severity::Error,
        (OperationOutcome::DependencyFailure | OperationOutcome::Interrupted, _) => Severity::Warn,
        _ => Severity::Info,
    };
    OperationRecord { severity, fact }
}

const QUEUE_CAPACITY: usize = 256;

enum Delivery {
    Legacy(Fact),
    Operation(OperationRecord),
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Authentication,
    Persistence,
    Checking,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Confirmed,
    Failed,
    Unconfirmed,
}

#[derive(Serialize)]
struct Fact {
    category: Category,
    outcome: Outcome,
    duration_ms: f64,
    release: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
}

pub struct Diagnostics {
    sender: mpsc::SyncSender<Delivery>,
    release: String,
    process_instance: ProcessInstanceId,
    listener: Arc<dyn OperationListener>,
}

impl Diagnostics {
    pub fn start(release: String) -> Self {
        let (sender, receiver) = mpsc::sync_channel::<Delivery>(QUEUE_CAPACITY);
        // A blocked or broken diagnostic destination cannot hold a request or
        // shutdown open. The bounded queue may drop records; history is authoritative.
        let _ = std::thread::Builder::new()
            .name("book-smartz-diagnostics".into())
            .spawn(move || {
                for delivery in receiver {
                    let record = match delivery {
                        Delivery::Legacy(fact) => serde_json::to_vec(&fact),
                        Delivery::Operation(record) => serde_json::to_vec(&record),
                    };
                    let Ok(mut record) = record else {
                        continue;
                    };
                    record.push(b'\n');
                    if std::io::stderr().write_all(&record).is_err() {
                        break;
                    }
                }
            });
        Self {
            listener: Arc::new(StderrListener(sender.clone())),
            sender,
            release,
            process_instance: ProcessInstanceId::fresh(),
        }
    }

    pub fn record(
        &self,
        category: Category,
        outcome: Outcome,
        started: Instant,
        action: Option<ActionId>,
    ) {
        let _ = self.sender.try_send(Delivery::Legacy(Fact {
            category,
            outcome,
            duration_ms: started.elapsed().as_secs_f64() * 1000.0,
            release: self.release.clone(),
            action: action.map(ActionId::text),
        }));
    }

    pub fn catalog_search(&self) -> Operation<'_> {
        Operation {
            diagnostics: self,
            id: OperationId::fresh(),
            started: Instant::now(),
            stage: Stage::Validation,
            terminal: None,
        }
    }

    pub fn unconfirmed_save(&self, started: Instant, action: ActionId) {
        for category in [Category::Persistence, Category::Checking] {
            self.record(category, Outcome::Unconfirmed, started, Some(action));
        }
    }

    pub fn scope(&self, category: Category) -> Scope<'_> {
        Scope {
            diagnostics: self,
            category,
            outcome: Outcome::Failed,
            started: Instant::now(),
        }
    }
}

pub struct Scope<'a> {
    diagnostics: &'a Diagnostics,
    category: Category,
    outcome: Outcome,
    started: Instant,
}
impl Scope<'_> {
    pub const fn outcome(&mut self, outcome: Outcome) {
        self.outcome = outcome;
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.diagnostics
            .record(self.category, self.outcome, self.started, None);
    }
}

/// Dropping incomplete work reports interruption at its last known stage.
pub struct Operation<'a> {
    diagnostics: &'a Diagnostics,
    id: OperationId,
    started: Instant,
    stage: Stage,
    terminal: Option<(OperationOutcome, Reason)>,
}
impl Operation<'_> {
    pub const fn stage(&mut self, stage: Stage) {
        self.stage = stage;
    }
    pub const fn finish(&mut self, outcome: OperationOutcome, reason: Reason) {
        self.terminal = Some((outcome, reason));
    }
}
impl Drop for Operation<'_> {
    fn drop(&mut self) {
        let (outcome, reason) = self
            .terminal
            .unwrap_or((OperationOutcome::Interrupted, Reason::Interrupted));
        self.diagnostics.listener.observe(OperationFinished {
            event: EventIdentity::OperationFinished,
            operation: OperationKind::CatalogSearch,
            outcome,
            stage: self.stage,
            reason,
            duration_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            release: self.diagnostics.release.clone(),
            process_instance: self.diagnostics.process_instance,
            operation_id: self.id,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex, mpsc},
        time::Duration,
    };

    use googletest::{assert_that, matchers::eq};

    use super::{Delivery, Diagnostics, OperationListener, QUEUE_CAPACITY, StderrListener};
    use crate::diagnostic_events::{
        EventIdentity, OperationFinished, OperationId, OperationKind, OperationOutcome,
        ProcessInstanceId, Reason, Severity, Stage,
    };

    #[derive(Default)]
    struct Capture(Mutex<Vec<OperationFinished>>);
    impl OperationListener for Capture {
        fn observe(&self, fact: OperationFinished) {
            self.0.lock().unwrap().push(fact);
        }
    }
    fn capture() -> (Diagnostics, Arc<Capture>) {
        let (sender, _) = mpsc::sync_channel(QUEUE_CAPACITY);
        let listener = Arc::new(Capture::default());
        (
            Diagnostics {
                sender,
                release: "a".repeat(64),
                process_instance: ProcessInstanceId::fresh(),
                listener: listener.clone(),
            },
            listener,
        )
    }

    #[googletest::test]
    fn producer_captures_safe_context_and_monotonic_milliseconds_once() {
        let (diagnostics, capture) = capture();
        {
            let mut operation = diagnostics.catalog_search();
            operation.stage(Stage::Selection);
            std::thread::sleep(Duration::from_millis(12));
            operation.finish(OperationOutcome::Confirmed, Reason::None);
        }
        {
            let mut operation = diagnostics.catalog_search();
            operation.finish(OperationOutcome::Rejected, Reason::InvalidInput);
        }
        let events = capture.0.lock().unwrap();
        assert_that!(events.len(), eq(2));
        assert_that!(events[0].event, eq(EventIdentity::OperationFinished));
        assert_that!(events[0].operation, eq(OperationKind::CatalogSearch));
        assert_that!(events[0].outcome, eq(OperationOutcome::Confirmed));
        assert_that!(events[0].stage, eq(Stage::Selection));
        assert_that!(events[0].reason, eq(Reason::None));
        assert_that!(events[0].duration_ms >= 12, eq(true));
        assert_that!(events[0].duration_ms < 5_000, eq(true));
        assert_that!(&events[0].release, eq(&"a".repeat(64)));
        assert_that!(events[0].process_instance, eq(events[1].process_instance));
        assert_that!(events[0].operation_id == events[1].operation_id, eq(false));
        assert_that!(events[1].outcome, eq(OperationOutcome::Rejected));
    }

    #[googletest::test]
    #[tokio::test]
    async fn cancellation_reports_interruption_at_the_known_stage() -> anyhow::Result<()> {
        let (diagnostics, capture) = capture();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut operation = diagnostics.catalog_search();
            operation.stage(Stage::Provider);
            let _ = started.send(());
            std::future::pending::<()>().await;
        });
        ready.await?;
        task.abort();
        assert_that!(task.await.unwrap_err().is_cancelled(), eq(true));
        let events = capture.0.lock().unwrap();
        assert_that!(events.len(), eq(1));
        assert_that!(events[0].stage, eq(Stage::Provider));
        assert_that!(events[0].outcome, eq(OperationOutcome::Interrupted));
        assert_that!(events[0].reason, eq(Reason::Interrupted));
        Ok(())
    }

    fn fact(outcome: OperationOutcome, reason: Reason) -> OperationFinished {
        OperationFinished {
            event: EventIdentity::OperationFinished,
            operation: OperationKind::CatalogSearch,
            outcome,
            stage: Stage::Provider,
            reason,
            duration_ms: 25,
            release: "b".repeat(64),
            process_instance: ProcessInstanceId::fresh(),
            operation_id: OperationId::fresh(),
        }
    }

    #[googletest::test]
    fn listener_preserves_allowlisted_facts_and_owns_severity() {
        for (outcome, reason, severity) in [
            (OperationOutcome::Confirmed, Reason::None, Severity::Info),
            (
                OperationOutcome::Rejected,
                Reason::AdmissionLimited,
                Severity::Info,
            ),
            (
                OperationOutcome::Rejected,
                Reason::ProviderLimited,
                Severity::Info,
            ),
            (
                OperationOutcome::DependencyFailure,
                Reason::Unavailable,
                Severity::Warn,
            ),
            (
                OperationOutcome::DependencyFailure,
                Reason::Database,
                Severity::Error,
            ),
            (
                OperationOutcome::DependencyFailure,
                Reason::Schema,
                Severity::Error,
            ),
            (
                OperationOutcome::DependencyFailure,
                Reason::SelectionPreparation,
                Severity::Error,
            ),
            (
                OperationOutcome::Interrupted,
                Reason::Interrupted,
                Severity::Warn,
            ),
        ] {
            let event = fact(outcome, reason);
            let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
            StderrListener(sender).observe(event.clone());
            let Delivery::Operation(record) = receiver.try_recv().unwrap() else {
                panic!("The operation listener emitted a legacy record.");
            };
            assert_that!(record.severity, eq(severity));
            assert_that!(&record.fact, eq(&event));
        }
    }

    #[googletest::test]
    fn full_or_closed_destination_never_blocks_a_producer() -> anyhow::Result<()> {
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let listener = StderrListener(sender);
        let (completed, completion) = mpsc::channel();
        let task = std::thread::spawn(move || {
            for _ in 0..257 {
                listener.observe(fact(OperationOutcome::Confirmed, Reason::None));
            }
            let _ = completed.send(());
        });
        completion.recv_timeout(Duration::from_secs(2))?;
        task.join().unwrap();
        let records: Vec<_> = receiver.try_iter().collect();
        assert_that!(records.len(), eq(256));
        assert_that!(
            records
                .iter()
                .all(|record| matches!(record, Delivery::Operation(_))),
            eq(true)
        );
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        drop(receiver);
        StderrListener(sender).observe(fact(OperationOutcome::Confirmed, Reason::None));
        Ok(())
    }
}
