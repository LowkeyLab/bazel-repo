use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tracing::{
    Event, Level, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context, prelude::*};
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(String),
    Number(u64),
    Float(f64),
}
#[derive(Clone, Debug)]
pub struct Record {
    pub level: Level,
    pub fields: BTreeMap<String, Value>,
}
impl Visit for Record {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().into(), Value::Text(format!("{value:?}")));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name().into(), Value::Text(value.into()));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().into(), Value::Number(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.insert(field.name().into(), Value::Float(value));
    }
}
#[derive(Clone, Default)]
pub struct Capture(pub Arc<Mutex<Vec<Record>>>);
impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target() != "book_smartz::storage" {
            return;
        }
        let mut record = Record {
            level: *event.metadata().level(),
            fields: BTreeMap::new(),
        };
        event.record(&mut record);
        self.0.lock().unwrap().push(record);
    }
}
pub fn subscriber(capture: Capture) -> impl Subscriber + Send + Sync {
    tracing_subscriber::registry().with(capture)
}
