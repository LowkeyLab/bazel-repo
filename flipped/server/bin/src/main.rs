use std::future::IntoFuture;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use examination_proto::flipped::examination::v1::examination_service_server::ExaminationServiceServer;
use flipped_server::grpc::ExaminationGrpcService;
use flipped_server::oauth::{OAuthState, router};
use flipped_server::observability::{
    EventContext, EventDispatcher, OpenTelemetryConfig, OpenTelemetryMetricsListener,
    OpenTelemetryTraceListener, Outcome, ServiceEvent, ServiceEventName, ServiceIdentity, Severity,
    StructuredTracingListener,
};
use flipped_server::store::InMemoryStore;
use flipped_server::{Application, Config};
use tokio::net::TcpListener;
use tokio::sync::watch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env().map_err(anyhow::Error::msg)?;
    let service_identity = ServiceIdentity {
        name: "flipped-server".to_owned(),
        version: config.service_version.clone(),
        environment: config.environment.clone(),
        instance_id: config.instance_id.clone(),
    };
    let structured = Arc::new(StructuredTracingListener::new(config.event_queue_capacity));
    let mut listeners: Vec<Arc<dyn flipped_server::observability::EventListener>> =
        vec![structured.clone()];
    let (otlp_traces, otlp_metrics) = if let Some(endpoint) = config.otlp_endpoint.as_deref() {
        let otel_config = OpenTelemetryConfig {
            endpoint,
            service: &service_identity,
            resource_attributes: config.otel_resource_attributes.as_deref(),
            traces_sampler: config.otel_traces_sampler.as_deref(),
            traces_sampler_arg: config.otel_traces_sampler_arg.as_deref(),
            queue_capacity: config.event_queue_capacity,
        };
        let traces =
            Arc::new(OpenTelemetryTraceListener::new(&otel_config).map_err(anyhow::Error::msg)?);
        let metrics =
            Arc::new(OpenTelemetryMetricsListener::new(&otel_config).map_err(anyhow::Error::msg)?);
        listeners.push(traces.clone());
        listeners.push(metrics.clone());
        (Some(traces), Some(metrics))
    } else {
        (None, None)
    };
    let dispatcher = EventDispatcher::new(service_identity, listeners);
    emit_lifecycle(&dispatcher, ServiceEventName::ServiceStarted);

    let credentials = Arc::new(flipped_server::credentials::CredentialService::new(
        &config,
    )?);
    let application = Arc::new(Application::new(
        InMemoryStore::default(),
        credentials,
        dispatcher.clone(),
        config.import_limits.clone(),
        config.oauth_client_id.clone(),
        config.oauth_audience.clone(),
        config.session_ttl,
        config.redemption_retry,
        config.command_result_capacity,
        config.event_stream_capacity,
        config.max_sessions,
        config.max_concurrent_imports,
        config.max_global_watches,
        config.max_watches_per_session,
        config.tombstone_retention,
        config.observability_hmac_key,
    ));
    let readiness = Arc::new(AtomicBool::new(false));
    let oauth = router(OAuthState {
        application: Arc::clone(&application),
        issuer: config.oauth_issuer.clone(),
        client_id: config.oauth_client_id.clone(),
        client_secret: config.oauth_client_secret.clone(),
        readiness: Arc::clone(&readiness),
    });
    let grpc_service =
        ExaminationServiceServer::new(ExaminationGrpcService::new(Arc::clone(&application)));
    let (health_reporter, health_service) = tonic_health::server::health_reporter();

    let http_listener = TcpListener::bind(config.http_addr).await?;
    let grpc_listener = TcpListener::bind(config.grpc_addr).await?;
    let grpc_incoming = tokio_stream::wrappers::TcpListenerStream::new(grpc_listener);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut http_shutdown = shutdown_rx.clone();
    let mut grpc_shutdown = shutdown_rx.clone();
    let mut cleanup_shutdown = shutdown_rx;
    let cleanup_interval = config.cleanup_interval;
    let cleanup_application = Arc::clone(&application);
    let cleanup = tokio::spawn(async move {
        let mut interval = tokio::time::interval(cleanup_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    cleanup_application.expire_sessions(std::time::SystemTime::now()).await;
                }
                changed = cleanup_shutdown.changed() => {
                    if changed.is_err() || *cleanup_shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    });

    health_reporter
        .set_serving::<ExaminationServiceServer<ExaminationGrpcService>>()
        .await;
    readiness.store(true, Ordering::Release);
    emit_lifecycle(&dispatcher, ServiceEventName::ServiceReady);

    let http = axum::serve(http_listener, oauth)
        .with_graceful_shutdown(async move {
            while !*http_shutdown.borrow_and_update() {
                if http_shutdown.changed().await.is_err() {
                    break;
                }
            }
        })
        .into_future();
    let grpc = tonic::transport::Server::builder()
        .timeout(std::time::Duration::from_secs(30))
        .concurrency_limit_per_connection(128)
        .add_service(health_service)
        .add_service(grpc_service)
        .serve_with_incoming_shutdown(grpc_incoming, async move {
            while !*grpc_shutdown.borrow_and_update() {
                if grpc_shutdown.changed().await.is_err() {
                    break;
                }
            }
        });
    tokio::pin!(http);
    tokio::pin!(grpc);

    enum Exit<T, U> {
        Http(T),
        Grpc(U),
        Signal,
    }
    let exit = tokio::select! {
        result = &mut http => Exit::Http(result),
        result = &mut grpc => Exit::Grpc(result),
        _ = shutdown_signal() => Exit::Signal,
    };

    readiness.store(false, Ordering::Release);
    health_reporter
        .set_not_serving::<ExaminationServiceServer<ExaminationGrpcService>>()
        .await;
    emit_lifecycle(&dispatcher, ServiceEventName::ServiceStopping);
    let _ = shutdown_tx.send(true);
    let transport_result: anyhow::Result<()> = match exit {
        Exit::Http(result) => {
            let http_result = result.map_err(anyhow::Error::from);
            let grpc_result = grpc.await.map_err(anyhow::Error::from);
            http_result.and(grpc_result)
        }
        Exit::Grpc(result) => {
            let grpc_result = result.map_err(anyhow::Error::from);
            let http_result = http.await.map_err(anyhow::Error::from);
            grpc_result.and(http_result)
        }
        Exit::Signal => {
            let (http_result, grpc_result) = tokio::join!(http, grpc);
            http_result
                .map_err(anyhow::Error::from)
                .and(grpc_result.map_err(anyhow::Error::from))
        }
    };
    let _ = cleanup.await;
    emit_lifecycle(&dispatcher, ServiceEventName::ServiceStopped);
    let _ = structured.shutdown(config.observability_flush_timeout);
    if let Some(listener) = otlp_traces {
        let _ = listener.shutdown(config.observability_flush_timeout);
    }
    if let Some(listener) = otlp_metrics {
        let _ = listener.shutdown(config.observability_flush_timeout);
    }
    transport_result
}

fn emit_lifecycle(dispatcher: &EventDispatcher, name: ServiceEventName) {
    dispatcher.emit(
        Severity::Info,
        EventContext::default(),
        ServiceEvent {
            name,
            outcome: Outcome::Success,
            error_code: None,
            duration_ms: None,
        },
    );
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM listener initialization");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
