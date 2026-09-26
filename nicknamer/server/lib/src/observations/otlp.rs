//! OTLP JSON log serialization for the existing synchronous tracing pipeline.
use std::{collections::BTreeMap, fmt};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    fmt::{FmtContext, FormatEvent, FormatFields, format::Writer},
    registry::LookupSpan,
};

/// Writes one OTLP `LogsData` object per tracing event, without a network exporter.
pub struct OtlpJson {
    service_name: String,
}

impl OtlpJson {
    #[must_use]
    pub fn new(service_name: String) -> Self {
        Self { service_name }
    }
}

#[derive(Default)]
struct Attributes(BTreeMap<String, Value>);

impl Visit for Attributes {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .insert(field.name().into(), json!({"stringValue": value}));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0
            .insert(field.name().into(), json!({"boolValue": value}));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0
            .insert(field.name().into(), json!({"intValue": value.to_string()}));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        // OTLP integers are signed. Preserve larger values as decimal strings.
        if let Ok(value) = i64::try_from(value) {
            self.record_i64(field, value);
        } else {
            self.record_str(field, &value.to_string());
        }
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        let value = if value.is_finite() {
            json!(value)
        } else {
            json!(if value.is_nan() {
                "NaN"
            } else if value.is_sign_positive() {
                "Infinity"
            } else {
                "-Infinity"
            })
        };
        self.0
            .insert(field.name().into(), json!({"doubleValue": value}));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.record_str(field, &format!("{value:?}"));
    }
}

fn unix_nanos(time: DateTime<Utc>) -> String {
    let nanos =
        i128::from(time.timestamp()) * 1_000_000_000 + i128::from(time.timestamp_subsec_nanos());
    u64::try_from(nanos).unwrap_or_default().to_string()
}

impl<S, N> FormatEvent<S, N> for OtlpJson
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _context: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let observed_at = unix_nanos(Utc::now());
        let mut attributes = Attributes::default();
        event.record(&mut attributes);
        let occurred_at = attributes
            .0
            .get("occurred_at")
            .and_then(|value| value.get("stringValue"))
            .and_then(Value::as_str)
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| unix_nanos(value.with_timezone(&Utc)));
        let body = attributes
            .0
            .get("event")
            .or_else(|| attributes.0.get("message"))
            .cloned()
            .unwrap_or_else(|| json!({"stringValue": event.metadata().name()}));
        let severity = match *event.metadata().level() {
            tracing::Level::TRACE => 1,
            tracing::Level::DEBUG => 5,
            tracing::Level::INFO => 9,
            tracing::Level::WARN => 13,
            tracing::Level::ERROR => 17,
        };
        let mut record = json!({
            "observedTimeUnixNano": observed_at,
            "severityNumber": severity,
            "severityText": event.metadata().level().as_str(),
            "body": body,
            "attributes": attributes.0.into_iter()
                .map(|(key, value)| json!({"key": key, "value": value}))
                .collect::<Vec<_>>(),
        });
        if let Some(occurred_at) = occurred_at {
            record["timeUnixNano"] = json!(occurred_at);
        }
        // Application operation/request IDs are attributes, not fabricated trace IDs.
        let data = json!({"resourceLogs": [{
            "resource": {"attributes": [{
                "key": "service.name", "value": {"stringValue": self.service_name}
            }]},
            "scopeLogs": [{
                "scope": {"name": event.metadata().target()},
                "logRecords": [record]
            }]
        }]});
        writeln!(writer, "{data}")
    }
}
