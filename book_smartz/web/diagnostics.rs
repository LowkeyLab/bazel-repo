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
    OperationRecord, OriginalActionId, ProcessInstanceId, Reason, Severity, Stage,
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

enum Delivery {
    Legacy(Fact),
    Operation(OperationRecord),
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
    async fn cancelled_mutation_preserves_uncertain_original_action() -> anyhow::Result<()> {
        for kind in [
            OperationKind::Add,
            OperationKind::Preference,
            OperationKind::Pause,
            OperationKind::Resume,
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
