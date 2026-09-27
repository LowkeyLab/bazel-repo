//! OpenTelemetry composition and best-effort OTLP stdout export.
use std::{
    future::Future,
    io::Write,
    process::ExitCode,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use opentelemetry_proto::{
    tonic::logs::v1::LogsData,
    transform::{
        common::tonic::ResourceAttributesWithSchema, logs::tonic::group_logs_by_resource_and_scope,
    },
};
use opentelemetry_sdk::{
    Resource,
    error::{OTelSdkError, OTelSdkResult},
    logs::{BatchConfigBuilder, BatchLogProcessor, LogBatch, LogExporter, SdkLoggerProvider},
};

pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
static OUTPUT: Mutex<()> = Mutex::new(());

/// Emits a fixed diagnostic without entering the telemetry pipeline or waiting
/// for a stalled telemetry writer. A broken stdout cannot diagnose itself.
pub fn diagnostic(message: &'static str) {
    if let Ok(_guard) = OUTPUT.try_lock() {
        let _ = writeln!(std::io::stdout().lock(), "{message}");
    }
}

/// Builds an instance-local provider; global subscriber registration belongs to main.
#[must_use]
pub fn logger_provider(
    service_name: String,
    exporter: impl LogExporter + 'static,
) -> SdkLoggerProvider {
    let batch = BatchConfigBuilder::default()
        .with_max_queue_size(2048)
        .with_max_export_batch_size(512)
        .with_scheduled_delay(Duration::from_secs(1))
        .build();
    SdkLoggerProvider::builder()
        .with_resource(
            Resource::builder_empty()
                .with_service_name(service_name)
                .build(),
        )
        .with_log_processor(
            BatchLogProcessor::builder(exporter)
                .with_batch_config(batch)
                .build(),
        )
        .build()
}

/// Drains telemetry after application cleanup on both successful and failed exits.
/// Export and shutdown failures never replace the application's exit status.
pub async fn run_with_telemetry(
    provider: SdkLoggerProvider,
    timeout: Duration,
    application: impl Future<Output = ExitCode>,
) -> ExitCode {
    let outcome = application.await;
    // SDK shutdown drains its queue and bounds waiting for the exporter thread.
    // Do not call force_flush first: it has a separate, longer timeout.
    if provider.shutdown_with_timeout(timeout).is_err() {
        diagnostic("telemetry shutdown failed or timed out");
    }
    outcome
}

#[derive(Debug, Default)]
pub struct StdoutExporter {
    resource: ResourceAttributesWithSchema,
    failure_reported: AtomicBool,
}

impl StdoutExporter {
    fn write_batch(&self, batch: &LogBatch<'_>) -> OTelSdkResult {
        let data = LogsData {
            resource_logs: group_logs_by_resource_and_scope(batch, &self.resource),
        };
        let mut bytes = serde_json::to_vec(&data)
            .map_err(|_| OTelSdkError::InternalFailure("OTLP serialization failed".into()))?;
        bytes.push(b'\n');
        let _guard = OUTPUT
            .lock()
            .map_err(|_| OTelSdkError::InternalFailure("stdout writer unavailable".into()))?;
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|_| OTelSdkError::InternalFailure("stdout write failed".into()))
    }
}

impl LogExporter for StdoutExporter {
    async fn export(&self, batch: LogBatch<'_>) -> OTelSdkResult {
        let result = self.write_batch(&batch);
        if result.is_err() && !self.failure_reported.swap(true, Ordering::Relaxed) {
            diagnostic("telemetry stdout export failed");
        }
        result
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.resource = resource.into();
    }
}
