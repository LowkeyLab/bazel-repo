//! Privacy allowlist at the application boundary; delivery never blocks business work.
use std::{
    io::{self, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Instant,
};

use serde::Serialize;

use crate::actions::ActionId;
use crate::diagnostic_events::{
    DiagnosticLossObserved, DiagnosticLossRecord, EventIdentity, OperationFinished, OperationId,
    OperationKind, OperationOutcome, OperationRecord, OriginalActionId, ProcessInstanceId, Reason,
    Severity, Stage, WorkerStatus,
};

/// Listeners receive only the immutable allowlisted fact.
pub trait OperationListener: Send + Sync {
    fn observe(&self, fact: OperationFinished);
}

struct StderrListener(DeliverySender);
impl OperationListener for StderrListener {
    fn observe(&self, fact: OperationFinished) {
        self.0.send(Delivery::Operation(operation_record(fact)));
    }
}

const fn operation_record(fact: OperationFinished) -> OperationRecord {
    let severity = match (fact.outcome, fact.reason) {
        (
            OperationOutcome::DependencyFailure | OperationOutcome::UncertainPersistence,
            Reason::Database
            | Reason::Schema
            | Reason::SelectionPreparation
            | Reason::CorruptHistory
            | Reason::PersistenceFailure
            | Reason::SessionOperation,
        ) => Severity::Error,
        (
            OperationOutcome::DependencyFailure
            | OperationOutcome::Interrupted
            | OperationOutcome::UncertainPersistence,
            _,
        ) => Severity::Warn,
        _ => Severity::Info,
    };
    OperationRecord { severity, fact }
}

const QUEUE_CAPACITY: usize = 256;

#[derive(Clone, Serialize)]
#[serde(untagged)]
enum Delivery {
    Legacy(Fact),
    Operation(OperationRecord),
    Loss(DiagnosticLossRecord),
}

#[derive(Default)]
struct DeliveryState {
    queue_drops: AtomicU64,
    destination_failures: AtomicU64,
    running: AtomicBool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DeliveryHealth {
    queue_drops: u64,
    destination_failures: u64,
    worker_status: WorkerStatus,
}
impl DeliveryState {
    fn increment(counter: &AtomicU64) {
        let _ = counter.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_add(1))
        });
    }
    fn snapshot(&self) -> DeliveryHealth {
        DeliveryHealth {
            queue_drops: self.queue_drops.load(Ordering::Relaxed),
            destination_failures: self.destination_failures.load(Ordering::Relaxed),
            worker_status: if self.running.load(Ordering::Acquire) {
                WorkerStatus::Running
            } else {
                WorkerStatus::Stopped
            },
        }
    }
}
#[derive(Clone)]
struct DeliverySender {
    sender: mpsc::SyncSender<Delivery>,
    health: Arc<DeliveryState>,
}
impl DeliverySender {
    fn send(&self, record: Delivery) {
        if let Err(error) = self.sender.try_send(record) {
            DeliveryState::increment(&self.health.queue_drops);
            if matches!(error, mpsc::TrySendError::Disconnected(_)) {
                self.health.running.store(false, Ordering::Release);
            }
        }
    }
}
fn channel() -> (DeliverySender, mpsc::Receiver<Delivery>) {
    let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
    let health = Arc::new(DeliveryState {
        running: AtomicBool::new(true),
        ..DeliveryState::default()
    });
    (DeliverySender { sender, health }, receiver)
}

trait Destination {
    fn write(&mut self, record: &Delivery) -> io::Result<()>;
}
struct StderrDestination<W> {
    writer: W,
    needs_boundary: bool,
}
impl<W: Write> Destination for StderrDestination<W> {
    fn write(&mut self, record: &Delivery) -> io::Result<()> {
        let mut encoded = serde_json::to_vec(record).map_err(io::Error::other)?;
        encoded.push(b'\n');
        if self.needs_boundary {
            self.writer.write_all(b"\n")?;
            self.needs_boundary = false;
        }
        let result = self.writer.write_all(&encoded);
        self.needs_boundary = result.is_err();
        result
    }
}
struct LossContext {
    release: String,
    process_instance: ProcessInstanceId,
}
#[derive(Default)]
struct Reported {
    queue_drops: u64,
    destination_failures: u64,
}
struct WorkerStatusGuard<'a>(&'a DeliveryState);
impl Drop for WorkerStatusGuard<'_> {
    fn drop(&mut self) {
        self.0.running.store(false, Ordering::Release);
    }
}
fn worker(
    receiver: mpsc::Receiver<Delivery>,
    destination: &mut impl Destination,
    health: &DeliveryState,
    context: &LossContext,
) {
    let _status = WorkerStatusGuard(health);
    let mut reported = Reported::default();
    for record in receiver {
        deliver(&record, destination, health, context, &mut reported);
    }
}
fn deliver(
    record: &Delivery,
    destination: &mut impl Destination,
    health: &DeliveryState,
    context: &LossContext,
    reported: &mut Reported,
) {
    if destination.write(record).is_err() {
        DeliveryState::increment(&health.destination_failures);
        return;
    }
    let current = health.snapshot();
    let queue_drops = current.queue_drops.saturating_sub(reported.queue_drops);
    let destination_failures = current
        .destination_failures
        .saturating_sub(reported.destination_failures);
    if queue_drops == 0 && destination_failures == 0 {
        return;
    }
    let summary = Delivery::Loss(DiagnosticLossRecord {
        severity: Severity::Warn,
        fact: DiagnosticLossObserved {
            event: EventIdentity::DiagnosticLossObserved,
            queue_drops,
            destination_failures,
            worker_status: current.worker_status,
            release: context.release.clone(),
            process_instance: context.process_instance,
        },
    });
    if destination.write(&summary).is_ok() {
        reported.queue_drops = current.queue_drops;
        reported.destination_failures = current.destination_failures;
    } else {
        DeliveryState::increment(&health.destination_failures);
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Persistence,
    Checking,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Confirmed,
    Unconfirmed,
}

#[derive(Clone, Serialize)]
struct Fact {
    category: Category,
    outcome: Outcome,
    duration_ms: f64,
    release: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
}

pub struct Diagnostics {
    sender: DeliverySender,
    release: String,
    process_instance: ProcessInstanceId,
    listener: Arc<dyn OperationListener>,
}

impl Diagnostics {
    pub fn start(release: String) -> Self {
        let (sender, receiver) = channel();
        let process_instance = ProcessInstanceId::fresh();
        let context = LossContext {
            release: release.clone(),
            process_instance,
        };
        let health = sender.health.clone();
        if std::thread::Builder::new()
            .name("book-smartz-diagnostics".into())
            .spawn(move || {
                worker(
                    receiver,
                    &mut StderrDestination {
                        writer: std::io::stderr(),
                        needs_boundary: false,
                    },
                    &health,
                    &context,
                )
            })
            .is_err()
        {
            sender.health.running.store(false, Ordering::Release);
        }
        Self {
            listener: Arc::new(StderrListener(sender.clone())),
            sender,
            release,
            process_instance,
        }
    }

    pub fn record(
        &self,
        category: Category,
        outcome: Outcome,
        started: Instant,
        action: Option<ActionId>,
    ) {
        self.sender.send(Delivery::Legacy(Fact {
            category,
            outcome,
            duration_ms: started.elapsed().as_secs_f64() * 1000.0,
            release: self.release.clone(),
            action: action.map(ActionId::text),
        }));
    }

    pub fn catalog_search(&self) -> Operation {
        self.operation(OperationKind::CatalogSearch)
    }

    pub fn operation(&self, kind: OperationKind) -> Operation {
        Operation {
            listener: self.listener.clone(),
            release: self.release.clone(),
            process_instance: self.process_instance,
            kind,
            id: OperationId::fresh(),
            started: Instant::now(),
            stage: Stage::Validation,
            terminal: None,
            original_action_id: None,
            persistence_uncertain: false,
        }
    }

    pub fn unconfirmed_save(&self, started: Instant, action: ActionId) {
        for category in [Category::Persistence, Category::Checking] {
            self.record(category, Outcome::Unconfirmed, started, Some(action));
        }
    }
}

/// Dropping incomplete work reports interruption at its last known stage.
pub struct Operation {
    listener: Arc<dyn OperationListener>,
    release: String,
    process_instance: ProcessInstanceId,
    kind: OperationKind,
    id: OperationId,
    started: Instant,
    stage: Stage,
    terminal: Option<(OperationOutcome, Reason)>,
    original_action_id: Option<OriginalActionId>,
    persistence_uncertain: bool,
}
impl Operation {
    pub const fn original_action(&mut self, action: OriginalActionId) {
        self.original_action_id = Some(action);
    }
    pub const fn persistence_started(&mut self) {
        self.persistence_uncertain = true;
    }
    pub fn action_failure(&mut self, error: &anyhow::Error) {
        if let Some(error) = error.downcast_ref::<book_smartz_storage::StoreError>() {
            if matches!(error, book_smartz_storage::StoreError::CommitUncertain(_)) {
                self.persistence_started();
                self.finish(
                    OperationOutcome::UncertainPersistence,
                    Reason::CommitUncertain,
                );
            } else {
                self.storage_failure(error);
            }
        } else if let Some(error) = error.downcast_ref::<sqlx::Error>() {
            self.database_failure(error);
        } else {
            self.finish(
                OperationOutcome::DependencyFailure,
                Reason::PersistenceFailure,
            );
        }
        if self.persistence_uncertain
            && let Some((_, reason)) = self.terminal
        {
            self.finish(OperationOutcome::UncertainPersistence, reason);
        }
    }
    pub const fn stage(&mut self, stage: Stage) {
        self.stage = stage;
    }
    pub fn database_failure(&mut self, error: &sqlx::Error) {
        let reason = if error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
            .is_some_and(|code| matches!(code.as_ref(), "42P01" | "42703" | "3F000"))
        {
            Reason::Schema
        } else {
            Reason::Database
        };
        self.finish(OperationOutcome::DependencyFailure, reason);
    }
    pub fn storage_failure(&mut self, error: &book_smartz_storage::StoreError) {
        match error {
            book_smartz_storage::StoreError::Database(error) => self.database_failure(error),
            book_smartz_storage::StoreError::CorruptHistory(_) => {
                self.finish(OperationOutcome::DependencyFailure, Reason::CorruptHistory);
            }
            _ => self.finish(
                OperationOutcome::DependencyFailure,
                Reason::PersistenceFailure,
            ),
        }
    }
    pub const fn finish(&mut self, outcome: OperationOutcome, reason: Reason) {
        self.terminal = Some((outcome, reason));
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        let (outcome, reason) = self.terminal.unwrap_or((
            if self.persistence_uncertain {
                OperationOutcome::UncertainPersistence
            } else {
                OperationOutcome::Interrupted
            },
            Reason::Interrupted,
        ));
        self.listener.observe(OperationFinished {
            event: EventIdentity::OperationFinished,
            operation: self.kind,
            outcome,
            stage: self.stage,
            reason,
            duration_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            release: self.release.clone(),
            process_instance: self.process_instance,
            operation_id: self.id,
            original_action_id: self.original_action_id,
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
        let (sender, _) = super::channel();
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
        for kind in [
            OperationKind::CatalogSearch,
            OperationKind::SignInStart,
            OperationKind::SignIn,
            OperationKind::SignOut,
            OperationKind::RankingRead,
        ] {
            let (diagnostics, capture) = capture();
            let (started, ready) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                let mut operation = diagnostics.operation(kind);
                operation.stage(Stage::Persistence);
                let _ = started.send(());
                std::future::pending::<()>().await;
            });
            ready.await?;
            task.abort();
            assert_that!(task.await.unwrap_err().is_cancelled(), eq(true));
            let events = capture.0.lock().unwrap();
            assert_that!(events.len(), eq(1));
            assert_that!(events[0].operation, eq(kind));
            assert_that!(events[0].stage, eq(Stage::Persistence));
            assert_that!(events[0].outcome, eq(OperationOutcome::Interrupted));
            assert_that!(events[0].reason, eq(Reason::Interrupted));
        }
        Ok(())
    }

    #[googletest::test]
    #[tokio::test]
    async fn cancelled_operation_preserves_uncertain_original_action() -> anyhow::Result<()> {
        for kind in [
            OperationKind::Add,
            OperationKind::Preference,
            OperationKind::Pause,
            OperationKind::Resume,
            OperationKind::OriginalSaveCheck,
        ] {
            let (diagnostics, capture) = capture();
            let action = crate::diagnostic_events::OriginalActionId::from(uuid::Uuid::new_v4());
            let (started, ready) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                let mut operation = diagnostics.operation(kind);
                operation.original_action(action);
                operation.stage(Stage::Persistence);
                operation.persistence_started();
                let _ = started.send(());
                std::future::pending::<()>().await;
            });
            ready.await?;
            task.abort();
            assert_that!(task.await.unwrap_err().is_cancelled(), eq(true));
            let events = capture.0.lock().unwrap();
            assert_that!(events.len(), eq(1));
            let record = super::operation_record(events[0].clone());
            assert_that!(record.fact.operation, eq(kind));
            assert_that!(record.fact.original_action_id, eq(Some(action)));
            assert_that!(
                record.fact.outcome,
                eq(OperationOutcome::UncertainPersistence)
            );
            assert_that!(record.fact.reason, eq(Reason::Interrupted));
            assert_that!(record.fact.stage, eq(Stage::Persistence));
            assert_that!(record.severity, eq(Severity::Warn));
        }
        Ok(())
    }

    #[googletest::test]
    fn action_failure_facts_distinguish_uncertain_commit_from_known_failures() {
        const PRIVATE_CAUSE: &str = "private-parser-or-database-cause";
        for (error, outcome, reason, severity) in [
            (
                anyhow::Error::from(book_smartz_storage::StoreError::CommitUncertain(
                    sqlx::Error::Protocol(PRIVATE_CAUSE.into()),
                )),
                OperationOutcome::UncertainPersistence,
                Reason::CommitUncertain,
                Severity::Warn,
            ),
            (
                anyhow::Error::from(sqlx::Error::Protocol(PRIVATE_CAUSE.into())),
                OperationOutcome::DependencyFailure,
                Reason::Database,
                Severity::Error,
            ),
            (
                anyhow::anyhow!(PRIVATE_CAUSE),
                OperationOutcome::DependencyFailure,
                Reason::PersistenceFailure,
                Severity::Error,
            ),
        ] {
            let (diagnostics, capture) = capture();
            let action = crate::diagnostic_events::OriginalActionId::from(uuid::Uuid::new_v4());
            {
                let mut operation = diagnostics.operation(OperationKind::Add);
                operation.original_action(action);
                operation.stage(Stage::Persistence);
                operation.action_failure(&error);
            }
            let events = capture.0.lock().unwrap();
            assert_that!(events.len(), eq(1));
            let record = super::operation_record(events[0].clone());
            assert_that!(record.fact.outcome, eq(outcome));
            assert_that!(record.fact.reason, eq(reason));
            assert_that!(record.fact.stage, eq(Stage::Persistence));
            assert_that!(record.fact.original_action_id, eq(Some(action)));
            assert_that!(&record.fact.release, eq(&"a".repeat(64)));
            assert_that!(record.severity, eq(severity));
        }
    }

    #[googletest::test]
    fn storage_failures_preserve_stage_and_remove_private_causes() {
        use book_smartz_storage::StoreError;

        const PRIVATE_CAUSE: &str = "private-reader-history-and-database-address";
        for (error, reason) in [
            (
                StoreError::Database(sqlx::Error::Protocol(PRIVATE_CAUSE.into())),
                Reason::Database,
            ),
            (
                StoreError::CorruptHistory(PRIVATE_CAUSE),
                Reason::CorruptHistory,
            ),
            (
                StoreError::CommitUncertain(sqlx::Error::Protocol(PRIVATE_CAUSE.into())),
                Reason::PersistenceFailure,
            ),
            (StoreError::InvalidComposition, Reason::PersistenceFailure),
        ] {
            let (diagnostics, capture) = capture();
            {
                let mut operation = diagnostics.operation(OperationKind::RankingRead);
                operation.stage(Stage::Persistence);
                operation.storage_failure(&error);
            }
            let events = capture.0.lock().unwrap();
            assert_that!(events.len(), eq(1));
            let record = super::operation_record(events[0].clone());
            assert_that!(record.fact.operation, eq(OperationKind::RankingRead));
            assert_that!(record.fact.stage, eq(Stage::Persistence));
            assert_that!(record.fact.outcome, eq(OperationOutcome::DependencyFailure));
            assert_that!(record.fact.reason, eq(reason));
            assert_that!(record.severity, eq(Severity::Error));
            assert_that!(&record.fact.release, eq(&"a".repeat(64)));
        }
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
            original_action_id: None,
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
            let (sender, receiver) = super::channel();
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
        let (sender, receiver) = super::channel();
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
        let (sender, receiver) = super::channel();
        drop(receiver);
        StderrListener(sender).observe(fact(OperationOutcome::Confirmed, Reason::None));
        Ok(())
    }
    struct PrefixFailure {
        bytes: Vec<u8>,
        calls: usize,
        fail_on: usize,
        error_pending: bool,
    }
    impl std::io::Write for PrefixFailure {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            if self.error_pending {
                self.error_pending = false;
                return Err(std::io::Error::other("controlled partial output failure"));
            }
            let accepted = if self.calls == self.fail_on {
                self.error_pending = true;
                bytes.len().min(7)
            } else {
                bytes.len()
            };
            self.bytes.extend_from_slice(&bytes[..accepted]);
            Ok(accepted)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[googletest::test]
    fn partial_output_failure_preserves_subsequent_record_boundaries() {
        for (fail_on, summary_failure) in [(1, false), (2, true)] {
            let (sender, receiver) = super::channel();
            let health = sender.health.clone();
            if summary_failure {
                super::DeliveryState::increment(&health.queue_drops);
            }
            let events: [_; 2] =
                std::array::from_fn(|_| fact(OperationOutcome::Confirmed, Reason::None));
            let listener = StderrListener(sender);
            for event in &events {
                listener.observe(event.clone());
            }
            drop(listener);
            let mut destination = super::StderrDestination {
                needs_boundary: false,
                writer: PrefixFailure {
                    bytes: Vec::new(),
                    calls: 0,
                    fail_on,
                    error_pending: false,
                },
            };
            super::worker(receiver, &mut destination, &health, &loss_context());
            let output = String::from_utf8(destination.writer.bytes).unwrap();
            let lines: Vec<_> = output.lines().collect();
            let fragment = usize::from(summary_failure);
            assert_that!(
                serde_json::from_str::<serde_json::Value>(lines[fragment]).is_err(),
                eq(true)
            );
            let recovered: crate::diagnostic_events::OperationRecord =
                serde_json::from_str(lines[fragment + 1]).unwrap();
            assert_that!(&recovered.fact, eq(&events[1]));
            let summary: crate::diagnostic_events::DiagnosticLossRecord =
                serde_json::from_str(lines[fragment + 2]).unwrap();
            assert_that!(summary.fact.destination_failures, eq(1));
            assert_that!(summary.fact.queue_drops, eq(u64::from(summary_failure)));
            assert_that!(lines.len(), eq(fragment + 3));
            assert_that!(health.snapshot().destination_failures, eq(1));
            if summary_failure {
                let first: crate::diagnostic_events::OperationRecord =
                    serde_json::from_str(lines[0]).unwrap();
                assert_that!(&first.fact, eq(&events[0]));
            }
        }
    }

    #[derive(Default)]
    struct ScriptDestination {
        decisions: std::collections::VecDeque<bool>,
        attempts: Vec<Delivery>,
        successes: Vec<Delivery>,
    }
    impl super::Destination for ScriptDestination {
        fn write(&mut self, record: &Delivery) -> std::io::Result<()> {
            self.attempts.push(record.clone());
            if !self.decisions.pop_front().unwrap_or(true) {
                return Err(std::io::Error::other("controlled destination failure"));
            }
            self.successes.push(record.clone());
            Ok(())
        }
    }
    fn loss_context() -> super::LossContext {
        super::LossContext {
            release: "c".repeat(64),
            process_instance: ProcessInstanceId::fresh(),
        }
    }

    #[googletest::test]
    fn failures_remain_pending_until_a_new_record_and_summary_succeed() {
        let (sender, receiver) = super::channel();
        let health = sender.health.clone();
        let events: [_; 3] =
            std::array::from_fn(|_| fact(OperationOutcome::Confirmed, Reason::None));
        let listener = StderrListener(sender);
        for event in &events {
            listener.observe(event.clone());
        }
        drop(listener);
        let mut destination = ScriptDestination {
            decisions: [false, true, false, true, true].into(),
            ..ScriptDestination::default()
        };
        let context = loss_context();
        super::worker(receiver, &mut destination, &health, &context);
        assert_that!(destination.attempts.len(), eq(5));
        assert_that!(destination.successes.len(), eq(3));
        for (index, expected) in [events[1].clone(), events[2].clone()].iter().enumerate() {
            let Delivery::Operation(record) = &destination.successes[index] else {
                panic!("The successful operation is absent.")
            };
            assert_that!(&record.fact, eq(expected));
        }
        let Delivery::Loss(record) = &destination.successes[2] else {
            panic!("The loss summary is absent.")
        };
        assert_that!(record.fact.event, eq(EventIdentity::DiagnosticLossObserved));
        assert_that!(record.fact.queue_drops, eq(0));
        assert_that!(record.fact.destination_failures, eq(2));
        assert_that!(
            record.fact.worker_status,
            eq(crate::diagnostic_events::WorkerStatus::Running)
        );
        assert_that!(record.severity, eq(Severity::Warn));
        assert_that!(&record.fact.release, eq(&context.release));
        assert_that!(record.fact.process_instance, eq(context.process_instance));
        assert_that!(health.snapshot().destination_failures, eq(2));
        assert_that!(
            health.snapshot().worker_status,
            eq(crate::diagnostic_events::WorkerStatus::Stopped)
        );
    }

    #[googletest::test]
    fn complete_output_failure_does_not_report_recursively_or_retry() {
        let (sender, receiver) = super::channel();
        let health = sender.health.clone();
        let listener = StderrListener(sender);
        for _ in 0..3 {
            listener.observe(fact(OperationOutcome::Confirmed, Reason::None));
        }
        drop(listener);
        let mut destination = ScriptDestination {
            decisions: [false, false, false].into(),
            ..ScriptDestination::default()
        };
        super::worker(receiver, &mut destination, &health, &loss_context());
        assert_that!(destination.attempts.len(), eq(3));
        assert_that!(destination.successes.is_empty(), eq(true));
        assert_that!(health.snapshot().destination_failures, eq(3));
        assert_that!(health.snapshot().queue_drops, eq(0));
        assert_that!(
            health.snapshot().worker_status,
            eq(crate::diagnostic_events::WorkerStatus::Stopped)
        );
    }

    struct DuringSummary {
        producer: Option<super::DeliverySender>,
        accepted: OperationFinished,
        discarded: OperationFinished,
        records: Vec<Delivery>,
    }
    impl super::Destination for DuringSummary {
        fn write(&mut self, record: &Delivery) -> std::io::Result<()> {
            if matches!(record, Delivery::Loss(_))
                && let Some(producer) = self.producer.take()
            {
                producer.send(Delivery::Operation(super::operation_record(
                    self.accepted.clone(),
                )));
                producer.send(Delivery::Operation(super::operation_record(
                    self.discarded.clone(),
                )));
            }
            self.records.push(record.clone());
            Ok(())
        }
    }

    #[googletest::test]
    fn saturation_and_drops_during_summary_delivery_remain_visible() {
        let (sender, receiver) = super::channel();
        let health = sender.health.clone();
        for _ in 0..256 {
            sender.send(Delivery::Operation(super::operation_record(fact(
                OperationOutcome::Confirmed,
                Reason::None,
            ))));
        }
        let discarded = fact(OperationOutcome::Confirmed, Reason::None);
        sender.send(Delivery::Operation(super::operation_record(
            discarded.clone(),
        )));
        assert_that!(health.snapshot().queue_drops, eq(1));
        let mut destination = DuringSummary {
            producer: Some(sender.clone()),
            accepted: fact(OperationOutcome::Confirmed, Reason::None),
            discarded: fact(OperationOutcome::Confirmed, Reason::None),
            records: Vec::new(),
        };
        drop(sender);
        super::worker(receiver, &mut destination, &health, &loss_context());
        let operations: Vec<_> = destination
            .records
            .iter()
            .filter_map(|record| {
                if let Delivery::Operation(record) = record {
                    Some(&record.fact)
                } else {
                    None
                }
            })
            .collect();
        let summaries: Vec<_> = destination
            .records
            .iter()
            .filter_map(|record| {
                if let Delivery::Loss(record) = record {
                    Some(&record.fact)
                } else {
                    None
                }
            })
            .collect();
        assert_that!(operations.len(), eq(257));
        assert_that!(
            operations
                .iter()
                .any(|record| record.operation_id == discarded.operation_id
                    || record.operation_id == destination.discarded.operation_id),
            eq(false)
        );
        assert_that!(summaries.len(), eq(2));
        assert_that!(summaries[0].queue_drops, eq(1));
        assert_that!(summaries[1].queue_drops, eq(1));
        assert_that!(health.snapshot().queue_drops, eq(2));
        assert_that!(health.snapshot().destination_failures, eq(0));
        assert_that!(
            health.snapshot().worker_status,
            eq(crate::diagnostic_events::WorkerStatus::Stopped)
        );
    }
}
