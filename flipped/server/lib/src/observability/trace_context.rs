use std::str::FromStr;

use opentelemetry::Context;
use opentelemetry::trace::{SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState};
use tonic::metadata::{AsciiMetadataValue, MetadataMap};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

use super::EventContext;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub parent_span_id: Option<String>,
    pub traceparent: Option<String>,
    pub tracestate: Option<String>,
    pub request_id: Option<String>,
}

impl TraceContext {
    pub fn extract_metadata(metadata: &MetadataMap) -> Self {
        let traceparent = one_ascii(metadata, "traceparent");
        let tracestate = one_ascii(metadata, "tracestate").filter(|value| valid_tracestate(value));
        let request_id = one_ascii(metadata, "x-request-id")
            .filter(|value| !value.is_empty() && value.len() <= 128);
        Self::from_values(traceparent.as_deref(), tracestate, request_id)
    }

    pub fn from_headers(headers: &axum::http::HeaderMap) -> Self {
        let one = |name: &str| {
            let mut values = headers.get_all(name).iter();
            let value = values.next()?.to_str().ok()?.to_owned();
            values.next().is_none().then_some(value)
        };
        Self::from_values(
            one("traceparent").as_deref(),
            one("tracestate").filter(|value| valid_tracestate(value)),
            one("x-request-id").filter(|value| !value.is_empty() && value.len() <= 128),
        )
    }

    fn from_values(
        traceparent: Option<&str>,
        tracestate: Option<String>,
        request_id: Option<String>,
    ) -> Self {
        let Some((trace_id, parent_span_id, flags)) =
            parse_traceparent(traceparent.unwrap_or_default())
        else {
            return Self {
                request_id,
                ..Self::default()
            };
        };
        let span_id = Uuid::now_v7().simple().to_string()[..16].to_owned();
        Self {
            traceparent: Some(format!("00-{trace_id}-{span_id}-{flags}")),
            trace_id: Some(trace_id),
            span_id: Some(span_id),
            parent_span_id: Some(parent_span_id),
            tracestate,
            request_id,
        }
    }

    pub fn inject_metadata(&self, metadata: &mut MetadataMap) {
        for (name, value) in [
            ("traceparent", self.traceparent.as_deref()),
            ("tracestate", self.tracestate.as_deref()),
            ("x-request-id", self.request_id.as_deref()),
        ] {
            if let Some(value) = value.and_then(|value| value.parse::<AsciiMetadataValue>().ok()) {
                metadata.insert(name, value);
            }
        }
    }

    pub fn event_context(&self) -> EventContext {
        EventContext {
            trace_id: self.trace_id.clone(),
            span_id: self.span_id.clone(),
            request_id: self.request_id.clone(),
            ..EventContext::default()
        }
    }

    fn remote_parent(&self) -> Context {
        let (Some(trace_id), Some(parent_span_id), Some(traceparent)) = (
            self.trace_id.as_deref(),
            self.parent_span_id.as_deref(),
            self.traceparent.as_deref(),
        ) else {
            return Context::new();
        };
        let (Ok(trace_id), Ok(parent_span_id), Ok(flags)) = (
            TraceId::from_hex(trace_id),
            SpanId::from_hex(parent_span_id),
            u8::from_str_radix(&traceparent[53..55], 16),
        ) else {
            return Context::new();
        };
        let trace_state = self
            .tracestate
            .as_deref()
            .and_then(|value| TraceState::from_str(value).ok())
            .unwrap_or_default();
        Context::new().with_remote_span_context(SpanContext::new(
            trace_id,
            parent_span_id,
            TraceFlags::new(flags),
            true,
            trace_state,
        ))
    }
}

fn one_ascii(metadata: &MetadataMap, name: &'static str) -> Option<String> {
    let mut values = metadata.get_all(name).iter();
    let value = values.next()?.to_str().ok()?.to_owned();
    values.next().is_none().then_some(value)
}

fn parse_traceparent(value: &str) -> Option<(String, String, String)> {
    let bytes = value.as_bytes();
    if !value.is_ascii()
        || bytes.len() != 55
        || &bytes[..3] != b"00-"
        || bytes[35] != b'-'
        || bytes[52] != b'-'
    {
        return None;
    }
    let trace_id = &value[3..35];
    let parent_id = &value[36..52];
    let flags = &value[53..55];
    if ![trace_id, parent_id, flags].iter().all(|part| {
        part.bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }) || trace_id.bytes().all(|byte| byte == b'0')
        || parent_id.bytes().all(|byte| byte == b'0')
    {
        return None;
    }
    Some((trace_id.to_owned(), parent_id.to_owned(), flags.to_owned()))
}

fn valid_tracestate(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

tokio::task_local! {
    static CURRENT_TRACE_CONTEXT: TraceContext;
}

pub async fn scope_trace<F: std::future::Future>(
    mut context: TraceContext,
    future: F,
) -> F::Output {
    let span = tracing::info_span!(
        target: "flipped_server::transport",
        "flipped.transport",
        otel.kind = "server"
    );
    let _ = span.set_parent(context.remote_parent());
    let otel_context = span.context();
    let span_context = otel_context.span().span_context().clone();
    if span_context.is_valid() {
        context.trace_id = Some(span_context.trace_id().to_string());
        context.span_id = Some(span_context.span_id().to_string());
        context.traceparent = Some(format!(
            "00-{}-{}-{:02x}",
            span_context.trace_id(),
            span_context.span_id(),
            span_context.trace_flags().to_u8()
        ));
    }
    CURRENT_TRACE_CONTEXT
        .scope(context, future.instrument(span))
        .await
}

pub fn current_event_context() -> EventContext {
    CURRENT_TRACE_CONTEXT
        .try_with(TraceContext::event_context)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extracts_preserves_parent_and_injects_valid_w3c_context() {
        let mut incoming = MetadataMap::new();
        incoming.insert(
            "traceparent",
            "00-0123456789abcdef0123456789abcdef-0123456789abcdef-01"
                .parse()
                .expect("metadata"),
        );
        incoming.insert("tracestate", "vendor=value".parse().expect("metadata"));
        incoming.insert("x-request-id", "request-1".parse().expect("metadata"));
        let extracted = TraceContext::extract_metadata(&incoming);
        assert_eq!(
            extracted.trace_id.as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
        assert_eq!(
            extracted.parent_span_id.as_deref(),
            Some("0123456789abcdef")
        );
        assert_ne!(
            extracted.span_id.as_deref(),
            extracted.parent_span_id.as_deref()
        );

        let scoped = scope_trace(extracted.clone(), async { current_event_context() }).await;
        assert_eq!(scoped.trace_id, extracted.trace_id);
        assert_eq!(scoped.request_id.as_deref(), Some("request-1"));

        let mut outgoing = MetadataMap::new();
        extracted.inject_metadata(&mut outgoing);
        assert_eq!(
            outgoing
                .get("tracestate")
                .and_then(|value| value.to_str().ok()),
            Some("vendor=value")
        );
        assert!(outgoing.get("traceparent").is_some());

        incoming.insert("traceparent", "00-invalid".parse().expect("metadata"));
        assert!(TraceContext::extract_metadata(&incoming).trace_id.is_none());
    }
}
