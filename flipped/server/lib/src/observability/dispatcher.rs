use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use chrono::{DateTime, SecondsFormat, Utc};
use uuid::Uuid;

use super::event::{EventContext, EventEnvelope, ServiceEvent, ServiceIdentity, Severity};

pub trait EventListener: Send + Sync + 'static {
    fn on_event(&self, event: Arc<EventEnvelope>);
}

pub trait EventSource: Send + Sync + 'static {
    fn now(&self) -> SystemTime;
    fn event_id(&self) -> String;
}

struct SystemEventSource;

impl EventSource for SystemEventSource {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn event_id(&self) -> String {
        Uuid::now_v7().to_string()
    }
}

#[derive(Clone)]
pub struct EventDispatcher {
    listeners: Arc<Vec<Arc<dyn EventListener>>>,
    service: ServiceIdentity,
    sequence: Arc<AtomicU64>,
    listener_panics: Arc<AtomicU64>,
    source: Arc<dyn EventSource>,
}

impl EventDispatcher {
    pub fn new(service: ServiceIdentity, listeners: Vec<Arc<dyn EventListener>>) -> Self {
        Self::new_with_source(service, listeners, Arc::new(SystemEventSource))
    }

    pub fn new_with_source(
        service: ServiceIdentity,
        listeners: Vec<Arc<dyn EventListener>>,
        source: Arc<dyn EventSource>,
    ) -> Self {
        assert!(
            !listeners.is_empty(),
            "at least one event listener is required"
        );
        Self {
            listeners: Arc::new(listeners),
            service,
            sequence: Arc::new(AtomicU64::new(0)),
            listener_panics: Arc::new(AtomicU64::new(0)),
            source,
        }
    }

    pub fn emit(&self, severity: Severity, context: EventContext, event: ServiceEvent) {
        let occurred_at: DateTime<Utc> = self.source.now().into();
        let envelope = Arc::new(EventEnvelope {
            schema_version: 1,
            event_name: event.name,
            event_id: self.source.event_id(),
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed) + 1,
            occurred_at: occurred_at.to_rfc3339_opts(SecondsFormat::Nanos, true),
            severity,
            service: self.service.clone(),
            trace_id: context.trace_id,
            span_id: context.span_id,
            request_id: context.request_id,
            command_id: context.command_id,
            causation_id: context.causation_id,
            session_ref: context.session_ref,
            event,
        });
        for listener in self.listeners.iter() {
            if catch_unwind(AssertUnwindSafe(|| {
                listener.on_event(Arc::clone(&envelope))
            }))
            .is_err()
            {
                self.listener_panics.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn listener_panics(&self) -> u64 {
        self.listener_panics.load(Ordering::Relaxed)
    }
}

#[derive(Default)]
pub struct RecordingEventListener {
    events: Mutex<Vec<Arc<EventEnvelope>>>,
}

impl RecordingEventListener {
    pub fn events(&self) -> Vec<Arc<EventEnvelope>> {
        self.events.lock().expect("recording listener lock").clone()
    }
}

impl EventListener for RecordingEventListener {
    fn on_event(&self, event: Arc<EventEnvelope>) {
        self.events
            .lock()
            .expect("recording listener lock")
            .push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::{Outcome, ServiceEvent, ServiceEventName};

    struct PanickingListener;

    impl EventListener for PanickingListener {
        fn on_event(&self, _event: Arc<EventEnvelope>) {
            panic!("listener failure");
        }
    }

    #[test]
    fn dispatcher_assigns_sequences_and_isolates_listener_panics() {
        let recording = Arc::new(RecordingEventListener::default());
        let dispatcher = EventDispatcher::new(
            ServiceIdentity {
                name: "test".to_owned(),
                version: "1".to_owned(),
                environment: "test".to_owned(),
                instance_id: "instance".to_owned(),
            },
            vec![Arc::new(PanickingListener), recording.clone()],
        );
        for name in [
            ServiceEventName::SessionCreated,
            ServiceEventName::SessionStarted,
        ] {
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
        let events = recording.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence, 1);
        assert_eq!(events[1].sequence, 2);
        assert_eq!(dispatcher.listener_panics(), 2);
        assert!(events.iter().all(|event| event.session_ref.is_none()));
        assert!(events[0].occurred_at.starts_with("20"));
        assert!(events[0].occurred_at.ends_with('Z'));
        assert!(chrono::DateTime::parse_from_rfc3339(&events[0].occurred_at).is_ok());
    }
}
