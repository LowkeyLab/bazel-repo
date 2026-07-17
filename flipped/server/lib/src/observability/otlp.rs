use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use opentelemetry::metrics::{Counter, Gauge, Histogram, MeterProvider as _};
use opentelemetry::trace::{
    Span, SpanContext, SpanId, SpanKind, TraceContextExt, TraceFlags, TraceId, TraceState, Tracer,
    TracerProvider as _,
};
use opentelemetry::{Context, KeyValue};
use opentelemetry_otlp::{MetricExporter, Protocol, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracer, SdkTracerProvider,
};
use tracing_subscriber::prelude::*;

use super::{EventEnvelope, EventListener, Outcome, ServiceEventName, ServiceIdentity};

pub struct OpenTelemetryConfig<'a> {
    pub endpoint: &'a str,
    pub service: &'a ServiceIdentity,
    pub resource_attributes: Option<&'a str>,
    pub traces_sampler: Option<&'a str>,
    pub traces_sampler_arg: Option<&'a str>,
    pub queue_capacity: usize,
}

pub struct OpenTelemetryTraceListener {
    provider: SdkTracerProvider,
    tracer: SdkTracer,
    failures: AtomicU64,
    healthy: AtomicBool,
}

impl OpenTelemetryTraceListener {
    pub fn new(config: &OpenTelemetryConfig<'_>) -> Result<Self, String> {
        let exporter = SpanExporter::builder()
            .with_http()
            .with_protocol(Protocol::HttpBinary)
            .with_endpoint(signal_endpoint(config.endpoint, "v1/traces")?)
            .build()
            .map_err(|error| format!("OTLP trace exporter configuration failed: {error}"))?;
        let batch = BatchSpanProcessor::builder(exporter)
            .with_batch_config(
                BatchConfigBuilder::default()
                    .with_max_queue_size(config.queue_capacity)
                    .build(),
            )
            .build();
        let provider = SdkTracerProvider::builder()
            .with_span_processor(batch)
            .with_resource(resource(config)?)
            .with_sampler(sampler(config.traces_sampler, config.traces_sampler_arg)?)
            .build();
        let tracer = provider.tracer("flipped-server.events");
        opentelemetry::global::set_tracer_provider(provider.clone());
        tracing_subscriber::registry()
            .with(
                tracing_opentelemetry::layer()
                    .with_tracer(provider.tracer("flipped-server.transport")),
            )
            .try_init()
            .map_err(|error| format!("OpenTelemetry tracing subscriber failed: {error}"))?;
        Ok(Self {
            provider,
            tracer,
            failures: AtomicU64::new(0),
            healthy: AtomicBool::new(true),
        })
    }

    pub fn shutdown(&self, timeout: Duration) -> bool {
        let result = self.provider.shutdown_with_timeout(timeout).is_ok();
        self.healthy.store(result, Ordering::Relaxed);
        if !result {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    pub fn dropped(&self) -> u64 {
        0
    }

    pub fn failures(&self) -> u64 {
        self.failures.load(Ordering::Relaxed)
    }

    pub fn is_healthy(&self) -> bool {
        self.healthy.load(Ordering::Relaxed)
    }
}

impl EventListener for OpenTelemetryTraceListener {
    fn on_event(&self, event: std::sync::Arc<EventEnvelope>) {
        let parent = parent_context(event.trace_id.as_deref(), event.span_id.as_deref());
        let mut span = self
            .tracer
            .span_builder(event_name(event.event_name))
            .with_kind(SpanKind::Internal)
            .with_attributes(event_attributes(event.as_ref()))
            .start_with_context(&self.tracer, &parent);
        span.end();
    }
}

pub struct OpenTelemetryMetricsListener {
    provider: SdkMeterProvider,
    events: Counter<u64>,
    requests: Counter<u64>,
    duration: Histogram<f64>,
    imports: Counter<u64>,
    sessions: Counter<u64>,
    active_sessions: Gauge<u64>,
    active_session_count: AtomicU64,
    stream_rejections: Counter<u64>,
    dropped: Counter<u64>,
    queue_utilization: Gauge<f64>,
    healthy: Gauge<u64>,
    failures: AtomicU64,
    exporter_healthy: AtomicBool,
}

impl OpenTelemetryMetricsListener {
    pub fn new(config: &OpenTelemetryConfig<'_>) -> Result<Self, String> {
        let exporter = MetricExporter::builder()
            .with_http()
            .with_protocol(Protocol::HttpBinary)
            .with_endpoint(signal_endpoint(config.endpoint, "v1/metrics")?)
            .build()
            .map_err(|error| format!("OTLP metric exporter configuration failed: {error}"))?;
        let provider = SdkMeterProvider::builder()
            .with_periodic_exporter(exporter)
            .with_resource(resource(config)?)
            .build();
        let meter = provider.meter("flipped-server.events");
        Ok(Self {
            events: meter.u64_counter("flipped_events_total").build(),
            requests: meter.u64_counter("flipped_requests_total").build(),
            duration: meter
                .f64_histogram("flipped_request_duration_ms")
                .with_unit("ms")
                .build(),
            imports: meter.u64_counter("flipped_imports_total").build(),
            sessions: meter.u64_counter("flipped_sessions_total").build(),
            active_sessions: meter.u64_gauge("flipped_active_sessions").build(),
            active_session_count: AtomicU64::new(0),
            stream_rejections: meter.u64_counter("flipped_watch_rejections_total").build(),
            dropped: meter
                .u64_counter("flipped_listener_dropped_events_total")
                .build(),
            queue_utilization: meter
                .f64_gauge("flipped_listener_queue_utilization")
                .build(),
            healthy: meter.u64_gauge("flipped_listener_healthy").build(),
            provider,
            failures: AtomicU64::new(0),
            exporter_healthy: AtomicBool::new(true),
        })
    }

    pub fn shutdown(&self, timeout: Duration) -> bool {
        let result = self.provider.shutdown_with_timeout(timeout).is_ok();
        self.exporter_healthy.store(result, Ordering::Relaxed);
        if !result {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
        result
    }

    pub fn dropped(&self) -> u64 {
        0
    }

    pub fn failures(&self) -> u64 {
        self.failures.load(Ordering::Relaxed)
    }

    pub fn is_healthy(&self) -> bool {
        self.exporter_healthy.load(Ordering::Relaxed)
    }
}

impl EventListener for OpenTelemetryMetricsListener {
    fn on_event(&self, event: std::sync::Arc<EventEnvelope>) {
        let attributes = event_attributes(event.as_ref());
        self.events.add(1, &attributes);
        if matches!(
            event.event_name,
            ServiceEventName::GrpcRequestCompleted | ServiceEventName::OAuthRequestCompleted
        ) {
            self.requests.add(1, &attributes);
        }
        if let Some(duration_ms) = event.event.duration_ms {
            self.duration.record(duration_ms as f64, &attributes);
        }
        match event.event_name {
            ServiceEventName::ImportStarted
            | ServiceEventName::ImportCompleted
            | ServiceEventName::ImportRejected
            | ServiceEventName::ImportCapacityRejected => self.imports.add(1, &attributes),
            ServiceEventName::SessionCreated
            | ServiceEventName::SessionStarted
            | ServiceEventName::SessionAdvanced
            | ServiceEventName::SessionEnded
            | ServiceEventName::SessionExpired => self.sessions.add(1, &attributes),
            ServiceEventName::GrpcWatchRejected => self.stream_rejections.add(1, &attributes),
            _ => {}
        }
        match event.event_name {
            ServiceEventName::SessionCreated => {
                self.active_session_count.fetch_add(1, Ordering::Relaxed);
            }
            ServiceEventName::SessionEnded | ServiceEventName::SessionExpired => {
                let _ = self.active_session_count.fetch_update(
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                    |value| Some(value.saturating_sub(1)),
                );
            }
            _ => {}
        }
        self.active_sessions
            .record(self.active_session_count.load(Ordering::Relaxed), &[]);
        self.dropped.add(0, &attributes);
        self.queue_utilization.record(0.0, &[]);
        self.healthy.record(
            u64::from(self.exporter_healthy.load(Ordering::Relaxed)),
            &[],
        );
    }
}

fn signal_endpoint(endpoint: &str, signal: &str) -> Result<String, String> {
    let endpoint = format!("{}/{signal}", endpoint.trim_end_matches('/'));
    url::Url::parse(&endpoint).map_err(|_| "FLIPPED_OTLP_ENDPOINT is invalid".to_owned())?;
    Ok(endpoint)
}

fn resource(config: &OpenTelemetryConfig<'_>) -> Result<Resource, String> {
    let mut attributes = vec![
        KeyValue::new("service.name", config.service.name.clone()),
        KeyValue::new("service.version", config.service.version.clone()),
        KeyValue::new(
            "deployment.environment.name",
            config.service.environment.clone(),
        ),
        KeyValue::new("service.instance.id", config.service.instance_id.clone()),
    ];
    if let Some(configured) = config.resource_attributes {
        for item in configured.split(',').filter(|item| !item.is_empty()) {
            let (key, value) = item
                .split_once('=')
                .filter(|(key, _)| !key.is_empty())
                .ok_or_else(|| "OTEL_RESOURCE_ATTRIBUTES is invalid".to_owned())?;
            attributes.push(KeyValue::new(key.to_owned(), value.to_owned()));
        }
    }
    Ok(Resource::builder_empty()
        .with_attributes(attributes)
        .build())
}

fn sampler(name: Option<&str>, argument: Option<&str>) -> Result<Sampler, String> {
    let root = match name.unwrap_or("parentbased_always_on") {
        "always_on" => return Ok(Sampler::AlwaysOn),
        "always_off" => return Ok(Sampler::AlwaysOff),
        "traceidratio" => Sampler::TraceIdRatioBased(ratio(argument)?),
        "parentbased_always_on" => Sampler::AlwaysOn,
        "parentbased_always_off" => Sampler::AlwaysOff,
        "parentbased_traceidratio" => Sampler::TraceIdRatioBased(ratio(argument)?),
        _ => return Err("OTEL_TRACES_SAMPLER is unsupported".to_owned()),
    };
    Ok(Sampler::ParentBased(Box::new(root)))
}

fn ratio(argument: Option<&str>) -> Result<f64, String> {
    let ratio = argument
        .unwrap_or("1.0")
        .parse::<f64>()
        .map_err(|_| "OTEL_TRACES_SAMPLER_ARG is invalid".to_owned())?;
    if !(0.0..=1.0).contains(&ratio) {
        return Err("OTEL_TRACES_SAMPLER_ARG must be between zero and one".to_owned());
    }
    Ok(ratio)
}

fn parent_context(trace_id: Option<&str>, span_id: Option<&str>) -> Context {
    let (Some(trace_id), Some(span_id)) = (trace_id, span_id) else {
        return Context::new();
    };
    let (Ok(trace_id), Ok(span_id)) = (TraceId::from_hex(trace_id), SpanId::from_hex(span_id))
    else {
        return Context::new();
    };
    Context::new().with_remote_span_context(SpanContext::new(
        trace_id,
        span_id,
        TraceFlags::SAMPLED,
        true,
        TraceState::from_str("").unwrap_or_default(),
    ))
}

fn event_name(name: ServiceEventName) -> String {
    serde_json::to_string(&name)
        .unwrap_or_else(|_| "unknown".to_owned())
        .trim_matches('"')
        .to_owned()
}

fn event_attributes(event: &EventEnvelope) -> Vec<KeyValue> {
    let error_code = event
        .event
        .error_code
        .and_then(|code| serde_json::to_string(&code).ok())
        .unwrap_or_else(|| "none".to_owned())
        .trim_matches('"')
        .to_owned();
    vec![
        KeyValue::new("event.name", event_name(event.event_name)),
        KeyValue::new(
            "event.outcome",
            match event.event.outcome {
                Outcome::Success => "success",
                Outcome::Rejected => "rejected",
                Outcome::Cancelled => "cancelled",
                Outcome::Failure => "failure",
            },
        ),
        KeyValue::new("event.error_code", error_code),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_supported_sampler_configuration() {
        assert!(sampler(Some("always_on"), None).is_ok());
        assert!(sampler(Some("parentbased_traceidratio"), Some("0.25")).is_ok());
        assert!(sampler(Some("traceidratio"), Some("1.1")).is_err());
        assert!(sampler(Some("unknown"), None).is_err());
    }

    #[test]
    fn forms_signal_endpoints_without_double_slashes() {
        assert_eq!(
            signal_endpoint("http://collector:4318/", "v1/traces").expect("endpoint"),
            "http://collector:4318/v1/traces"
        );
    }
}
