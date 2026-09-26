use nicknamer_server::observations::{
    Dispatcher, LoggingListener, ShutdownOutcome,
    lifecycle::{DRAIN_TIMEOUT, RequestWork, SignalFailure, load_configuration, serve_until},
};
use std::{process::ExitCode, sync::Arc};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    if tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .try_init()
        .is_err()
    {
        eprintln!("application initialization failed: stage=logging category=internal");
        return ExitCode::FAILURE;
    }
    let dispatcher = Arc::new(Dispatcher::new(vec![Arc::new(LoggingListener)]));
    let Ok(config) = load_configuration(&dispatcher, nicknamer_server::config::Config::from_env)
    else {
        dispatcher.flush();
        return ExitCode::FAILURE;
    };
    let Ok(prepared) = nicknamer_server::web::prepare_server(config, dispatcher.clone()).await
    else {
        dispatcher.flush();
        return ExitCode::FAILURE;
    };
    let outcome = serve_until(
        prepared.listener,
        prepared.app,
        RequestWork::default(),
        dispatcher,
        shutdown_signal(),
        async { tokio::time::sleep(DRAIN_TIMEOUT).await },
    )
    .await;
    if outcome == ShutdownOutcome::Failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

async fn shutdown_signal() -> Result<(), SignalFailure> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|_| SignalFailure)?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result.map_err(|_| SignalFailure),
            result = terminate.recv() => result.ok_or(SignalFailure),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await.map_err(|_| SignalFailure)
    }
}
