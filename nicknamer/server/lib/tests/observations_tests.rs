use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use googletest::prelude::*;
use nicknamer_server::observations::{
    AuthenticationOutcome, BulkOperation, BulkOutcome, Channel, DeliveryError, Dispatcher, Fact,
    FailureCategory, LoggingListener, MutationKind, MutationOrigin, MutationOutcome, Observation,
    ObservationContext, ObservationListener, ObservationSink, Severity,
};

fn observed(fact: Fact) -> Observation {
    Observation {
        context: ObservationContext::new(),
        fact,
    }
}

#[googletest::test]
fn committed_mutation_is_info_with_safe_fields() {
    let event = observed(Fact::NameMutationFinished {
        kind: MutationKind::Create,
        origin: MutationOrigin::Standalone,
        outcome: MutationOutcome::Committed,
        category: None,
    });
    let record = LoggingListener::record_for(&event).unwrap();
    expect_that!(record.severity, eq(Severity::Info));
    expect_that!(record.event, eq("name_mutation_finished"));
    expect_that!(record.fields.get("outcome"), some(eq(&"committed".into())));
    expect_that!(record.fields.contains_key("name"), eq(false));
    expect_that!(record.fields.contains_key("server_id"), eq(false));
    expect_that!(record.fields.contains_key("error"), eq(false));
    expect_that!(
        record.fields.get("occurred_at"),
        some(eq(
            &nicknamer_server::observations::logging::FieldValue::Timestamp(
                event.context.occurred_at
            )
        ))
    );
}

#[googletest::test]
fn routine_authentication_rejection_is_info() {
    let event = observed(Fact::AuthenticationFinished {
        channel: Channel::Api,
        outcome: AuthenticationOutcome::Rejected,
        category: Some(FailureCategory::InvalidCredentials),
    });
    let record = LoggingListener::record_for(&event).unwrap();
    expect_that!(record.severity, eq(Severity::Info));
    expect_that!(
        record.fields.get("category"),
        some(eq(&"invalid_credentials".into()))
    );
}

#[googletest::test]
fn partial_bulk_failure_is_warning() {
    let event = observed(Fact::BulkOperationFinished {
        operation: BulkOperation::Import,
        attempted: 3,
        succeeded: 1,
        skipped: 1,
        failed: 1,
        input_count: Some(3),
        outcome: BulkOutcome::Partial,
        categories: vec![(FailureCategory::Database, 1)],
    });
    let record = LoggingListener::record_for(&event).unwrap();
    expect_that!(record.severity, eq(Severity::Warn));
    expect_that!(record.fields.get("failed"), some(eq(&1_u64.into())));
}

#[googletest::test]
fn operational_failure_is_error() {
    let event = observed(Fact::NameMutationFinished {
        kind: MutationKind::Delete,
        origin: MutationOrigin::Bulk,
        outcome: MutationOutcome::Failed,
        category: Some(FailureCategory::Database),
    });
    let record = LoggingListener::record_for(&event).unwrap();
    expect_that!(record.severity, eq(Severity::Error));
}

#[googletest::test]
fn successful_health_request_has_no_record() {
    let event = observed(Fact::RequestFinished {
        route: nicknamer_server::observations::Route::Health,
        method: nicknamer_server::observations::Method::Get,
        status: 200,
        outcome: nicknamer_server::observations::RequestOutcome::Completed,
    });
    expect_that!(LoggingListener::record_for(&event).is_none(), eq(true));
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

struct RecordingListener(Mutex<Vec<Observation>>);

impl ObservationListener for RecordingListener {
    fn on_event(&self, event: &Observation) -> Result<(), DeliveryError> {
        self.0.lock().unwrap().push(event.clone());
        Ok(())
    }

    fn flush(&self) -> Result<(), DeliveryError> {
        Ok(())
    }
}

#[googletest::test]
fn listener_failure_does_not_prevent_later_delivery() {
    let recorder = Arc::new(RecordingListener(Mutex::new(Vec::new())));
    let dispatcher = Dispatcher::new(vec![Arc::new(FailingListener), recorder.clone()]);
    let event = observed(Fact::ApplicationReady);
    dispatcher.record(&event);
    expect_that!(&*recorder.0.lock().unwrap(), elements_are![eq(&event)]);
    dispatcher.flush();
}

#[googletest::test]
fn delivery_diagnostic_is_rate_limited_across_record_and_flush() {
    let count = Arc::new(AtomicUsize::new(0));
    let count_for_callback = count.clone();
    let dispatcher = Dispatcher::with_diagnostic(
        vec![Arc::new(FailingListener)],
        Arc::new(move || {
            count_for_callback.fetch_add(1, Ordering::SeqCst);
        }),
    );
    dispatcher.record(&observed(Fact::ApplicationReady));
    dispatcher.record(&observed(Fact::ApplicationReady));
    dispatcher.flush();
    expect_that!(count.load(Ordering::SeqCst), eq(1));
}

#[googletest::test]
fn diagnostic_panic_does_not_change_application_result() {
    let recorder = Arc::new(RecordingListener(Mutex::new(Vec::new())));
    let dispatcher = Dispatcher::with_diagnostic(
        vec![Arc::new(FailingListener), recorder.clone()],
        Arc::new(|| panic!("diagnostic failed")),
    );
    let event = observed(Fact::ApplicationReady);
    dispatcher.record(&event);
    expect_that!(&*recorder.0.lock().unwrap(), elements_are![eq(&event)]);
}
