use std::{
    env::VarError,
    future::{Future, IntoFuture},
    net::SocketAddr,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use tokio::{net::TcpListener, sync::oneshot, time::timeout};

/// Parse the optional health listener configuration.
///
/// # Errors
/// Rejects invalid environment values and addresses.
pub fn address(value: Result<String, VarError>) -> Result<SocketAddr> {
    let value = match value {
        Ok(value) => value,
        Err(VarError::NotPresent) => "0.0.0.0:8080".to_owned(),
        Err(VarError::NotUnicode(_)) => {
            return Err(anyhow!("HEALTH_BIND_ADDRESS is not valid Unicode"));
        }
    };
    value
        .parse()
        .map_err(|_| anyhow!("HEALTH_BIND_ADDRESS must be an IP address and port"))
}

/// Serve liveness while the supplied bot initializes and runs.
///
/// # Errors
/// Returns bot failures, unexpected server termination, and shutdown failures.
pub async fn run(listener: TcpListener, bot: impl Future<Output = Result<()>>) -> Result<()> {
    let router = axum::Router::new().route("/healthz", axum::routing::get(|| async { "ok" }));
    let (shutdown, stopped) = oneshot::channel();
    let server = axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .into_future();
    supervise(bot, server, shutdown).await
}

async fn supervise(
    bot: impl Future<Output = Result<()>>,
    server: impl Future<Output = std::io::Result<()>>,
    shutdown: oneshot::Sender<()>,
) -> Result<()> {
    tokio::pin!(bot, server);
    tokio::select! {
        outcome = &mut bot => {
            let _ = shutdown.send(());
            let drained = timeout(Duration::from_secs(5), &mut server).await
                .context("health server shutdown timed out")
                .and_then(|result| result.context("health server shutdown failed"));
            outcome.and(drained)
        }
        outcome = &mut server => {
            outcome.context("health server failed")?;
            Err(anyhow!("health server stopped unexpectedly"))
        }
    }
}

#[cfg(test)]
#[path = "health_tests.rs"]
mod tests;
