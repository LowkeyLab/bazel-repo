use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use googletest::prelude::*;
use nicknamer_server::observations::{
    BulkOperation, BulkOutcome, DeliveryError, Dispatcher, Fact, FailureCategory, LoggingListener,
    Observation, ObservationContext, ObservationListener, ObservationSink, Outcome, RequestId,
};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{Layer, Registry};

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Text(String),
    Count(u64),
    Flag(bool),
    Debug,
}

#[derive(Default)]
struct Fields(BTreeMap<String, Value>);

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .insert(field.name().into(), Value::Text(value.into()));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().into(), Value::Count(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().into(), Value::Flag(value));
    }
    fn record_debug(&mut self, field: &Field, _value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().into(), Value::Debug);
    }
}

type Records = Arc<Mutex<Vec<(Level, BTreeMap<String, Value>)>>>;
struct Capture(Records);
impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        self.0
            .lock()
            .unwrap()
            .push((*event.metadata().level(), fields.0));
    }
}

#[googletest::test]
fn bulk_counts_and_correlation_reach_subscriber_as_typed_fields() {
    let records = Records::default();
    let subscriber = Registry::default().with(Capture(records.clone()));
    let context =
        ObservationContext::for_request(RequestId::new()).with_duration(Duration::from_millis(42));
    let observation = Observation {
        context: context.clone(),
        fact: Fact::BulkOperationFinished {
            operation: BulkOperation::Delete,
            attempted: 3,
            succeeded: 1,
            skipped: 0,
            failed: 2,
            input_count: Some(3),
            outcome: BulkOutcome::Partial,
            categories: vec![(FailureCategory::MissingEntry, 2)],
        },
    };
    tracing::subscriber::with_default(subscriber, || {
        LoggingListener.on_event(&observation).unwrap();
    });
    let records = records.lock().unwrap();
    assert_that!(records.len(), eq(1));
    let (level, fields) = &records[0];
    assert_that!(*level, eq(Level::WARN));
    for (name, expected) in [
        ("event", Value::Text("bulk_operation_finished".into())),
        ("operation", Value::Text("delete".into())),
        ("outcome", Value::Text("partial".into())),
        ("attempted", Value::Count(3)),
        ("succeeded", Value::Count(1)),
        ("failed", Value::Count(2)),
        ("missing_entry_count", Value::Count(2)),
        ("duration_ms", Value::Count(42)),
        (
            "operation_id",
            Value::Text(context.operation_id.to_string()),
        ),
        (
            "request_id",
            Value::Text(context.request_id.unwrap().to_string()),
        ),
    ] {
        assert_that!(fields.get(name), some(eq(&expected)));
    }
    assert_that!(
        fields.get("occurred_at"),
        some(matches_pattern!(Value::Text(_)))
    );
    assert_that!(
        fields.values().any(|v| matches!(v, Value::Debug)),
        eq(false)
    );
    assert_that!(fields.contains_key("fields"), eq(false));
}

#[googletest::test]
fn read_filter_and_count_remain_typed_without_optional_error_fields() {
    let records = Records::default();
    let subscriber = Registry::default().with(Capture(records.clone()));
    tracing::subscriber::with_default(subscriber, || {
        LoggingListener
            .on_event(&Observation {
                context: ObservationContext::new(),
                fact: Fact::NamesReadFinished {
                    filter_present: true,
                    result_count: Some(7),
                    outcome: Outcome::Succeeded,
                    category: None,
                },
            })
            .unwrap();
    });
    let records = records.lock().unwrap();
    assert_that!(records.len(), eq(1));
    let (_, fields) = &records[0];
    assert_that!(fields.get("filter_present"), some(eq(&Value::Flag(true))));
    assert_that!(fields.get("result_count"), some(eq(&Value::Count(7))));
    assert_that!(fields.contains_key("category"), eq(false));
    assert_that!(fields.contains_key("request_id"), eq(false));
}

struct FailingListener;
impl ObservationListener for FailingListener {
    fn on_event(&self, _: &Observation) -> Result<(), DeliveryError> {
        Err(DeliveryError)
    }
    fn flush(&self) -> Result<(), DeliveryError> {
        Err(DeliveryError)
    }
}

#[googletest::test]
fn default_delivery_failure_does_not_reenter_tracing_subscriber() {
    let records = Records::default();
    let subscriber = Registry::default().with(Capture(records.clone()));
    tracing::subscriber::with_default(subscriber, || {
        let dispatcher = Dispatcher::new(vec![Arc::new(FailingListener)]);
        dispatcher.record(&Observation {
            context: ObservationContext::new(),
            fact: Fact::ApplicationReady,
        });
        dispatcher.flush();
    });
    assert_that!(records.lock().unwrap().is_empty(), eq(true));
}
