//! Startup diagnostics and coordination of observable request work at shutdown.
use super::{
    Dispatcher, Fact, FailureCategory, Observation, ObservationContext, ObservationSink, Outcome,
    ShutdownOutcome, StartupStage,
};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::sync::watch;

/// Signal registration or waiting failed; provider errors never enter observations.
#[derive(Clone, Copy, Debug)]
pub struct SignalFailure;

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
    fn track(&self) -> WorkGuard {
        let mut state = self.0.state.lock().unwrap();
        state.1 += 1;
        self.0.active.send_replace(state.1);
        WorkGuard(self.clone())
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

// Added outside the application's layers so cancellation also waits for requests
// that entered the router but have not yet reached observation middleware.
async fn track_request(
    axum::extract::State(work): axum::extract::State<RequestWork>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let _active = work.track();
    // This nested future is destroyed before _active on cancellation.
    next.run(request).await
}

// Axum owns detached connection tasks. Interrupt their IO at the deadline, then
// finish connections through graceful shutdown and await tracked request cleanup.
struct CancellableListener {
    listener: tokio::net::TcpListener,
    work: RequestWork,
}
impl axum::serve::Listener for CancellableListener {
    type Io = CancellableIo;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let (stream, address) = axum::serve::Listener::accept(&mut self.listener).await;
        let work = self.work.clone();
        (
            CancellableIo {
                stream,
                cancelled: Box::pin(async move { work.cancelled().await }),
                is_cancelled: false,
            },
            address,
        )
    }
    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}
struct CancellableIo {
    stream: tokio::net::TcpStream,
    cancelled: Pin<Box<dyn Future<Output = ()> + Send>>,
    is_cancelled: bool,
}
impl CancellableIo {
    fn check_cancelled(&mut self, cx: &mut Context<'_>) -> std::io::Result<()> {
        if !self.is_cancelled {
            self.is_cancelled = self.cancelled.as_mut().poll(cx).is_ready();
        }
        if self.is_cancelled {
            Err(std::io::ErrorKind::ConnectionAborted.into())
        } else {
            Ok(())
        }
    }
}
impl tokio::io::AsyncRead for CancellableIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.check_cancelled(cx)?;
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}
impl tokio::io::AsyncWrite for CancellableIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.check_cancelled(cx)?;
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.check_cancelled(cx)?;
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.check_cancelled(cx)?;
        Pin::new(&mut self.stream).poll_shutdown(cx)
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
    S: Future<Output = Result<(), SignalFailure>>,
    D: Future<Output = ()>,
{
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let listener = CancellableListener {
        listener,
        work: work.clone(),
    };
    let app = app
        .layer(axum::Extension(work.clone()))
        .layer(axum::middleware::from_fn_with_state(
            work.clone(),
            track_request,
        ));
    let server = axum::serve(listener, app).with_graceful_shutdown(async {
        let _ = stopped.await;
    });
    let mut server = Box::pin(std::future::IntoFuture::into_future(server));
    dispatcher.record(&Observation {
        context: ObservationContext::new(),
        fact: Fact::ApplicationReady,
    });
    let started;
    let mut server_finished = false;
    let outcome = tokio::select! {
        result = &mut server => { server_finished = true; started = Instant::now(); if result.is_ok() { ShutdownOutcome::Drained } else { ShutdownOutcome::Failed } },
        signal_result = shutdown => {
            started = Instant::now();
            let _ = stop.send(());
            let drain_outcome = tokio::select! {
                result = &mut server => { server_finished = true; if result.is_ok() { ShutdownOutcome::Drained } else { ShutdownOutcome::Failed } },
                () = deadline => ShutdownOutcome::TimedOut,
            };
            if signal_result.is_err() { ShutdownOutcome::Failed } else { drain_outcome }
        }
    };
    work.cancel();
    // Stop acceptance and connection processing before awaiting tracked request cleanup.
    if !server_finished {
        let _ = server.await;
    }
    // Axum signals connection completion before destroying its connection future.
    // Tracked outer request guards cover that final destruction and all observations.
    work.idle().await;
    dispatcher.record(&Observation {
        context: ObservationContext::new().with_duration(started.elapsed()),
        fact: Fact::ShutdownFinished { outcome },
    });
    dispatcher.flush();
    outcome
}
