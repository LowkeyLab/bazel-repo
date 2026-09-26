//! Startup diagnostics and coordination of observable request work at shutdown.
use super::{
    Dispatcher, Fact, FailureCategory, Observation, ObservationContext, ObservationSink, Outcome,
    ShutdownOutcome, StartupStage,
};
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::watch;

pub const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct RequestWork(Arc<WorkState>);
struct WorkState {
    state: Mutex<(bool, usize)>,
    cancelled: watch::Sender<bool>,
    active: watch::Sender<usize>,
}
impl Default for RequestWork {
    fn default() -> Self {
        Self(Arc::new(WorkState {
            state: Mutex::new((false, 0)),
            cancelled: watch::channel(false).0,
            active: watch::channel(0).0,
        }))
    }
}
impl RequestWork {
    pub(crate) fn enter(&self) -> Option<WorkGuard> {
        let mut state = self.0.state.lock().unwrap();
        if state.0 {
            return None;
        }
        state.1 += 1;
        self.0.active.send_replace(state.1);
        Some(WorkGuard(self.clone()))
    }
    pub(crate) async fn cancelled(&self) {
        let mut receiver = self.0.cancelled.subscribe();
        let _ = receiver.wait_for(|value| *value).await;
    }
    fn cancel(&self) {
        self.0.state.lock().unwrap().0 = true;
        self.0.cancelled.send_replace(true);
    }
    async fn idle(&self) {
        let mut receiver = self.0.active.subscribe();
        let _ = receiver.wait_for(|count| *count == 0).await;
    }
}
pub(crate) struct WorkGuard(RequestWork);
impl Drop for WorkGuard {
    fn drop(&mut self) {
        let mut state = self.0.0.state.lock().unwrap();
        state.1 -= 1;
        self.0.0.active.send_replace(state.1);
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("application startup failed")]
pub struct StartupFailure;

pub(crate) fn stage_result<T, E>(
    dispatcher: &Dispatcher,
    stage: StartupStage,
    category: FailureCategory,
    started: Instant,
    result: Result<T, E>,
) -> Result<T, StartupFailure> {
    dispatcher.record(&Observation {
        context: ObservationContext::new().with_duration(started.elapsed()),
        fact: Fact::StartupStageFinished {
            stage,
            outcome: if result.is_ok() {
                Outcome::Succeeded
            } else {
                Outcome::Failed
            },
            category: result.as_ref().err().map(|_| category),
        },
    });
    result.map_err(|_| StartupFailure)
}

/// Load config after listener registration; only bounded diagnostics cross this boundary.
///
/// # Errors
/// Returns a sanitized startup failure if configuration loading fails.
pub fn load_configuration(
    dispatcher: &Dispatcher,
    load: impl FnOnce() -> anyhow::Result<crate::config::Config>,
) -> Result<crate::config::Config, StartupFailure> {
    stage_result(
        dispatcher,
        StartupStage::Configuration,
        FailureCategory::Configuration,
        Instant::now(),
        load(),
    )
}

/// Stop accepting on notification, drain until deadline, then drop observed handler futures.
/// The deadline future is first polled after notification; construct its timer lazily.
pub async fn serve_until<S, D>(
    listener: tokio::net::TcpListener,
    app: axum::Router,
    work: RequestWork,
    dispatcher: Arc<Dispatcher>,
    shutdown: S,
    deadline: D,
) -> ShutdownOutcome
where
    S: Future<Output = ()>,
    D: Future<Output = ()>,
{
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = axum::serve(listener, app.layer(axum::Extension(work.clone())))
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        });
    let mut server = Box::pin(std::future::IntoFuture::into_future(server));
    dispatcher.record(&Observation {
        context: ObservationContext::new(),
        fact: Fact::ApplicationReady,
    });
    let started;
    let outcome = tokio::select! {
        result = &mut server => { started = Instant::now(); if result.is_ok() { ShutdownOutcome::Drained } else { ShutdownOutcome::Failed } },
        () = shutdown => {
            started = Instant::now();
            let _ = stop.send(());
            tokio::select! {
                result = &mut server => if result.is_ok() { ShutdownOutcome::Drained } else { ShutdownOutcome::Failed },
                () = deadline => ShutdownOutcome::TimedOut,
            }
        }
    };
    // Close the listening socket even when the deadline wins before graceful shutdown polls.
    drop(server);
    work.cancel();
    // Cancellation is cooperative: wait for handlers' drops (including their observations),
    // not merely the detached connection task's parent serve future.
    work.idle().await;
    dispatcher.record(&Observation {
        context: ObservationContext::new().with_duration(started.elapsed()),
        fact: Fact::ShutdownFinished { outcome },
    });
    dispatcher.flush();
    outcome
}
