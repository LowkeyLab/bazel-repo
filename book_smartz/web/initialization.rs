//! Initialization facts and bounded output before normal listener registration.
use std::{
    io::{self, Write},
    sync::mpsc,
    time::{Duration, Instant},
};

use crate::diagnostic_events::{
    EventIdentity, InitializationFinished, InitializationOutcome, InitializationReason,
    InitializationRecord, InitializationStage, ProcessInstanceId, Severity,
};

pub struct Initialization {
    event: EventIdentity,
    started: Instant,
    stage: InitializationStage,
    reason: InitializationReason,
    finished: bool,
    release: Option<String>,
    process_instance: ProcessInstanceId,
}
impl Initialization {
    pub fn new(event: EventIdentity) -> Self {
        Self {
            event,
            started: Instant::now(),
            stage: InitializationStage::Configuration,
            reason: InitializationReason::Configuration,
            finished: false,
            release: None,
            process_instance: ProcessInstanceId::fresh(),
        }
    }
    pub fn verified_release(&mut self, release: String) {
        self.release = Some(release);
    }
    pub const fn instance(&self) -> ProcessInstanceId {
        self.process_instance
    }
    pub const fn phase(&mut self, stage: InitializationStage, reason: InitializationReason) {
        self.stage = stage;
        self.reason = reason;
    }
    pub fn failure(&mut self, error: &anyhow::Error) {
        if let Some(error) = error
            .chain()
            .find_map(|source| source.downcast_ref::<sqlx::Error>())
        {
            self.reason = match error {
                sqlx::Error::Configuration(_) => InitializationReason::Configuration,
                sqlx::Error::Migrate(_) => InitializationReason::Schema,
                sqlx::Error::Database(error)
                    if error
                        .code()
                        .is_some_and(|code| code.starts_with("42") || code.starts_with("3F")) =>
                {
                    InitializationReason::Schema
                }
                _ => InitializationReason::Database,
            };
        }
    }
    pub fn finish(&mut self, outcome: InitializationOutcome) -> Option<InitializationFinished> {
        if self.finished {
            return None;
        }
        self.finished = true;
        if outcome == InitializationOutcome::Succeeded {
            self.phase(InitializationStage::Complete, InitializationReason::None);
        }
        Some(InitializationFinished {
            event: self.event,
            outcome,
            stage: self.stage,
            reason: self.reason,
            duration_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            release: self.release.clone(),
            process_instance: self.process_instance,
        })
    }
}

pub const FALLBACK_LIMIT: Duration = Duration::from_millis(250);

pub fn fallback(fact: InitializationFinished) {
    let _ = bounded(move || {
        let record = record(fact);
        let mut encoded = serde_json::to_vec(&record).map_err(io::Error::other)?;
        encoded.push(b'\n');
        io::stderr().write_all(&encoded)
    });
}
fn bounded(output: impl FnOnce() -> io::Result<()> + Send + 'static) -> io::Result<()> {
    let started = Instant::now();
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("initialization-diagnostic".into())
        .spawn(move || {
            let _ = sender.send(output());
        })?;
    receiver
        .recv_timeout(FALLBACK_LIMIT.saturating_sub(started.elapsed()))
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "diagnostic output did not finish"))?
}
pub const fn record(fact: InitializationFinished) -> InitializationRecord {
    let severity = match fact.outcome {
        InitializationOutcome::Succeeded => Severity::Info,
        InitializationOutcome::Failed | InitializationOutcome::DeadlineExceeded => Severity::Error,
    };
    InitializationRecord { severity, fact }
}

pub async fn migrate(
    observation: &mut Initialization,
    limit: Duration,
    work: impl AsyncFnOnce(&mut Initialization) -> anyhow::Result<()>,
    report: impl FnOnce(InitializationFinished),
) -> anyhow::Result<()> {
    let (result, outcome) = match tokio::time::timeout(limit, work(observation)).await {
        Ok(Ok(())) => (Ok(()), InitializationOutcome::Succeeded),
        Ok(Err(error)) => {
            observation.failure(&error);
            (Err(error), InitializationOutcome::Failed)
        }
        Err(error) => {
            observation.reason = InitializationReason::Timeout;
            (Err(error.into()), InitializationOutcome::DeadlineExceeded)
        }
    };
    if let Some(fact) = observation.finish(outcome) {
        report(fact);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{Initialization, migrate};
    use crate::diagnostic_events::{
        EventIdentity, InitializationOutcome, InitializationReason, InitializationStage,
    };
    use googletest::{assert_that, matchers::eq};
    use std::time::Duration;

    #[googletest::test]
    #[tokio::test]
    async fn migration_deadline_preserves_failure_and_the_known_stage() -> anyhow::Result<()> {
        let mut observation = Initialization::new(EventIdentity::MigrationFinished);
        let mut terminal = None;
        let result = migrate(
            &mut observation,
            Duration::from_millis(10),
            async |observation| {
                observation.phase(InitializationStage::Schema, InitializationReason::Schema);
                std::future::pending::<anyhow::Result<()>>().await
            },
            |fact| terminal = Some(fact),
        )
        .await;
        assert_that!(result.is_err(), eq(true));
        let fact =
            terminal.ok_or_else(|| anyhow::anyhow!("The migration deadline fact is absent."))?;
        assert_that!(fact.event, eq(EventIdentity::MigrationFinished));
        assert_that!(fact.outcome, eq(InitializationOutcome::DeadlineExceeded));
        assert_that!(fact.stage, eq(InitializationStage::Schema));
        assert_that!(fact.reason, eq(InitializationReason::Timeout));
        assert_that!(fact.duration_ms >= 5, eq(true));
        assert_that!(fact.release.is_none(), eq(true));
        assert_that!(
            observation.finish(InitializationOutcome::Failed).is_none(),
            eq(true)
        );
        Ok(())
    }
    #[googletest::test]
    fn stalled_fallback_expires_and_releases_the_caller() -> anyhow::Result<()> {
        let (release, blocked) = std::sync::mpsc::sync_channel::<()>(1);
        let (completed, completion) = std::sync::mpsc::sync_channel(1);
        let started = std::time::Instant::now();
        let result = super::bounded(move || {
            blocked.recv().map_err(std::io::Error::other)?;
            completed.send(()).map_err(std::io::Error::other)?;
            Ok(())
        });
        let elapsed = started.elapsed();
        release.send(())?;
        completion.recv_timeout(Duration::from_secs(2))?;
        assert_that!(result.unwrap_err().kind(), eq(std::io::ErrorKind::TimedOut));
        assert_that!(elapsed >= super::FALLBACK_LIMIT, eq(true));
        assert_that!(elapsed < Duration::from_secs(2), eq(true));
        Ok(())
    }

    #[googletest::test]
    #[tokio::test]
    async fn broken_fallback_preserves_migration_results_and_typed_database_causes()
    -> anyhow::Result<()> {
        for failed in [false, true] {
            let mut observation = Initialization::new(EventIdentity::MigrationFinished);
            let mut record = None;
            let result = migrate(
                &mut observation,
                Duration::from_secs(1),
                async |observation| {
                    observation.phase(InitializationStage::Schema, InitializationReason::Schema);
                    if failed {
                        return Err(sqlx::Error::Io(std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "private database response",
                        ))
                        .into());
                    }
                    Ok(())
                },
                |fact| {
                    record = Some(super::record(fact));
                    let output = super::bounded(|| {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::WriteZero,
                            "private destination failure",
                        ))
                    });
                    assert_that!(
                        output.unwrap_err().kind(),
                        eq(std::io::ErrorKind::WriteZero)
                    );
                },
            )
            .await;
            assert_that!(result.is_err(), eq(failed));
            let record = record.unwrap();
            assert_that!(record.fact.event, eq(EventIdentity::MigrationFinished));
            assert_that!(record.fact.release.is_none(), eq(true));
            if failed {
                let error = result.unwrap_err();
                let Some(sqlx::Error::Io(source)) = error.downcast_ref::<sqlx::Error>() else {
                    panic!("The original migration error is absent.");
                };
                assert_that!(source.kind(), eq(std::io::ErrorKind::BrokenPipe));
                assert_that!(record.fact.outcome, eq(InitializationOutcome::Failed));
                assert_that!(record.fact.stage, eq(InitializationStage::Schema));
                assert_that!(record.fact.reason, eq(InitializationReason::Database));
                assert_that!(
                    record.severity,
                    eq(crate::diagnostic_events::Severity::Error)
                );
            } else {
                assert_that!(record.fact.outcome, eq(InitializationOutcome::Succeeded));
                assert_that!(record.fact.stage, eq(InitializationStage::Complete));
                assert_that!(record.fact.reason, eq(InitializationReason::None));
                assert_that!(
                    record.severity,
                    eq(crate::diagnostic_events::Severity::Info)
                );
            }
        }
        Ok(())
    }
}
