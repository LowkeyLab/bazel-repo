use std::{process::ExitCode, sync::Arc, time::Duration};

use googletest::prelude::*;
use nicknamer_server::observations::{
    Dispatcher, LoggingListener, ShutdownOutcome,
    lifecycle::{RequestWork, load_configuration, serve_until},
    otlp::{logger_provider, run_with_telemetry},
};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_sdk::{
    error::{OTelSdkError, OTelSdkResult},
    logs::{LogBatch, LogExporter},
};
use tracing::instrument::WithSubscriber;
use tracing_subscriber::prelude::*;

#[derive(Debug)]
struct UnavailableExporter;
impl LogExporter for UnavailableExporter {
    async fn export(&self, _: LogBatch<'_>) -> OTelSdkResult {
        Err(OTelSdkError::InternalFailure("unavailable".into()))
    }
    fn shutdown_with_timeout(&self, _: Duration) -> OTelSdkResult {
        Err(OTelSdkError::InternalFailure("unavailable".into()))
    }
}

#[googletest::test]
#[tokio::test]
async fn exporter_failure_preserves_drained_server_outcome() {
    let provider = logger_provider("test".into(), UnavailableExporter);
    let subscriber =
        tracing_subscriber::registry().with(OpenTelemetryTracingBridge::new(&provider));
    let result = run_with_telemetry(provider, Duration::from_secs(2), async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let outcome = serve_until(
            listener,
            axum::Router::new(),
            RequestWork::default(),
            Arc::new(Dispatcher::new(vec![Arc::new(LoggingListener)])),
            async { Ok(()) },
            std::future::pending(),
        )
        .await;
        assert_that!(outcome, eq(ShutdownOutcome::Drained));
        ExitCode::SUCCESS
    })
    .with_subscriber(subscriber)
    .await;
    assert_that!(result, eq(ExitCode::SUCCESS));
}

#[googletest::test]
#[tokio::test]
async fn exporter_failure_preserves_configuration_failure() {
    let provider = logger_provider("test".into(), UnavailableExporter);
    let subscriber =
        tracing_subscriber::registry().with(OpenTelemetryTracingBridge::new(&provider));
    let result = run_with_telemetry(provider, Duration::from_secs(2), async {
        let dispatcher = Dispatcher::new(vec![Arc::new(LoggingListener)]);
        let config = load_configuration(&dispatcher, || {
            Err(anyhow::anyhow!("invalid configuration"))
        });
        assert_that!(config.is_err(), eq(true));
        ExitCode::FAILURE
    })
    .with_subscriber(subscriber)
    .await;
    assert_that!(result, eq(ExitCode::FAILURE));
}

#[derive(Debug)]
struct BlockedShutdown {
    started: std::sync::mpsc::Sender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}
impl LogExporter for BlockedShutdown {
    async fn export(&self, _: LogBatch<'_>) -> OTelSdkResult {
        Ok(())
    }
    fn shutdown_with_timeout(&self, _: Duration) -> OTelSdkResult {
        let _ = self.started.send(());
        // Dropping the test's sender also releases this wait if an assertion fails.
        let _ = self.release.lock().unwrap().recv();
        Ok(())
    }
}

#[googletest::test]
#[tokio::test]
async fn stalled_exporter_cleanup_does_not_hold_application_exit() {
    let (started, waiting) = std::sync::mpsc::channel();
    let (release, blocked) = std::sync::mpsc::channel();
    let provider = logger_provider(
        "test".into(),
        BlockedShutdown {
            started,
            release: std::sync::Mutex::new(blocked),
        },
    );
    let result = run_with_telemetry(provider, Duration::ZERO, async { ExitCode::SUCCESS }).await;
    assert_that!(result, eq(ExitCode::SUCCESS));
    // Application cleanup has returned while exporter cleanup is still blocked.
    assert_that!(
        waiting.recv_timeout(Duration::from_secs(5)).is_ok(),
        eq(true)
    );
    let _ = release.send(());
}
