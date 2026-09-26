use googletest::prelude::*;
use nicknamer_server::observations::{
    DeliveryError, Dispatcher, Fact, Observation, ObservationListener, Outcome, StartupStage,
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Recorder(Mutex<Vec<Fact>>);
impl ObservationListener for Recorder {
    fn on_event(&self, event: &Observation) -> Result<(), DeliveryError> {
        self.0.lock().unwrap().push(event.fact.clone());
        Ok(())
    }
    fn flush(&self) -> Result<(), DeliveryError> {
        Ok(())
    }
}

#[googletest::test]
#[tokio::test]
async fn bind_failure_is_observed_before_database_connection() {
    let occupied = tokio::net::TcpListener::bind("0.0.0.0:0").await.unwrap();
    let recorder = Arc::new(Recorder::default());
    let dispatcher = Arc::new(Dispatcher::new(vec![recorder.clone()]));
    let config = nicknamer_server::config::Config {
        db_url: "invalid-secret-url".into(),
        port: occupied.local_addr().unwrap().port(),
        admin_username: "admin".into(),
        admin_password: "secret".into(),
        jwt_secret: "secret".into(),
    };
    let result = nicknamer_server::web::prepare_server(config, dispatcher).await;
    assert_that!(result.is_err(), eq(true));
    let events = recorder.0.lock().unwrap();
    assert_that!(
        events.iter().any(|fact| matches!(
            fact,
            Fact::StartupStageFinished {
                stage: StartupStage::Binding,
                outcome: Outcome::Failed,
                ..
            }
        )),
        eq(true)
    );
    assert_that!(
        events.iter().any(|fact| matches!(
            fact,
            Fact::StartupStageFinished {
                stage: StartupStage::Connection,
                ..
            }
        )),
        eq(false)
    );
}

#[googletest::test]
fn configuration_failure_has_one_bounded_diagnostic() {
    let recorder = Arc::new(Recorder::default());
    let dispatcher = Dispatcher::new(vec![recorder.clone()]);
    let result = nicknamer_server::observations::lifecycle::load_configuration(&dispatcher, || {
        Err(anyhow::anyhow!("secret configuration value"))
    });
    assert_that!(result.is_err(), eq(true));
    assert_that!(
        recorder.0.lock().unwrap().as_slice(),
        eq(&[Fact::StartupStageFinished {
            stage: StartupStage::Configuration,
            outcome: Outcome::Failed,
            category: Some(nicknamer_server::observations::FailureCategory::Configuration)
        }])
    );
}

#[googletest::test]
#[tokio::test]
async fn connection_failure_does_not_announce_readiness() {
    let recorder = Arc::new(Recorder::default());
    let dispatcher = Arc::new(Dispatcher::new(vec![recorder.clone()]));
    let config = config("unsupported-secret-url".into());
    let result = nicknamer_server::web::prepare_server(config, dispatcher).await;
    assert_that!(result.is_err(), eq(true));
    let events = recorder.0.lock().unwrap();
    assert_that!(
        events.iter().any(|fact| matches!(
            fact,
            Fact::StartupStageFinished {
                stage: StartupStage::Connection,
                outcome: Outcome::Failed,
                ..
            }
        )),
        eq(true)
    );
    assert_that!(
        events
            .iter()
            .any(|fact| matches!(fact, Fact::ApplicationReady)),
        eq(false)
    );
}

fn config(db_url: String) -> nicknamer_server::config::Config {
    nicknamer_server::config::Config {
        db_url,
        port: 0,
        admin_username: "admin".into(),
        admin_password: "secret".into(),
        jwt_secret: "secret".into(),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Entry {
    Event(Fact),
    Dropped,
    Flushed,
}
struct OrderedRecorder(Arc<Mutex<Vec<Entry>>>);
impl ObservationListener for OrderedRecorder {
    fn on_event(&self, event: &Observation) -> Result<(), DeliveryError> {
        self.0
            .lock()
            .unwrap()
            .push(Entry::Event(event.fact.clone()));
        Ok(())
    }
    fn flush(&self) -> Result<(), DeliveryError> {
        self.0.lock().unwrap().push(Entry::Flushed);
        Ok(())
    }
}
struct Dropped(Arc<Mutex<Vec<Entry>>>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.lock().unwrap().push(Entry::Dropped);
    }
}
struct Failing;
impl ObservationListener for Failing {
    fn on_event(&self, _: &Observation) -> Result<(), DeliveryError> {
        Err(DeliveryError)
    }
    fn flush(&self) -> Result<(), DeliveryError> {
        Err(DeliveryError)
    }
}

async fn controlled_request_shutdown(timeout: bool) {
    use nicknamer_server::observations::{
        ShutdownOutcome,
        lifecycle::{RequestWork, serve_until},
    };
    let entries = Arc::new(Mutex::new(Vec::new()));
    let dispatcher = Arc::new(Dispatcher::with_diagnostic(
        vec![
            Arc::new(Failing),
            Arc::new(OrderedRecorder(entries.clone())),
        ],
        Arc::new(|| {}),
    ));
    let observer: nicknamer_server::observations::SharedObserver = dispatcher.clone();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let app = axum::Router::new()
        .route(
            "/",
            axum::routing::get({
                let entries = entries.clone();
                let started = started.clone();
                let release = release.clone();
                move || {
                    let entries = entries.clone();
                    let started = started.clone();
                    let release = release.clone();
                    async move {
                        let _dropped = Dropped(entries);
                        started.notify_one();
                        release.notified().await;
                        "OK"
                    }
                }
            }),
        )
        .layer(axum::middleware::from_fn_with_state(
            observer,
            nicknamer_server::observations::http::observe_request,
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let (expire, expired) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(serve_until(
        listener,
        app,
        RequestWork::default(),
        dispatcher,
        async {
            let _ = stopped.await;
        },
        async {
            let _ = expired.await;
        },
    ));
    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket.writable().await.unwrap();
    socket
        .try_write(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    started.notified().await;
    stop.send(()).unwrap();
    if timeout {
        expire.send(()).unwrap();
    } else {
        release.notify_one();
    }
    let outcome = server.await.unwrap();
    let mut response = Vec::new();
    loop {
        socket.readable().await.unwrap();
        let mut chunk = [0; 1024];
        match socket.try_read(&mut chunk) {
            Ok(0) => break,
            Ok(size) => response.extend_from_slice(&chunk[..size]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(error) => panic!("HTTP response read failed: {error}"),
        }
        if response.windows(2).any(|window| window == b"\r\n") {
            break;
        }
    }
    let response = std::str::from_utf8(&response).unwrap();
    assert_that!(
        response.split_whitespace().nth(1),
        eq(Some(if timeout { "503" } else { "200" }))
    );

    let expected = if timeout {
        ShutdownOutcome::TimedOut
    } else {
        ShutdownOutcome::Drained
    };
    assert_that!(outcome, eq(expected));
    assert_shutdown_order(&entries.lock().unwrap(), timeout, expected);
}

fn assert_shutdown_order(
    entries: &[Entry],
    timeout: bool,
    expected: nicknamer_server::observations::ShutdownOutcome,
) {
    use nicknamer_server::observations::RequestOutcome;
    assert_that!(
        entries.first(),
        eq(Some(&Entry::Event(Fact::ApplicationReady)))
    );
    assert_that!(entries.last(), eq(Some(&Entry::Flushed)));
    let dropped = entries
        .iter()
        .position(|entry| *entry == Entry::Dropped)
        .unwrap();
    let request = entries
        .iter()
        .position(|entry| matches!(entry, Entry::Event(Fact::RequestFinished { .. })))
        .unwrap();
    let shutdown = entries
        .iter()
        .position(|entry| *entry == Entry::Event(Fact::ShutdownFinished { outcome: expected }))
        .unwrap();
    assert_that!(dropped < request && request < shutdown, eq(true));
    let expected_status = if timeout { 503 } else { 200 };
    let expected_request = if timeout {
        RequestOutcome::Aborted
    } else {
        RequestOutcome::Completed
    };
    assert_that!(entries.iter().filter(|entry| matches!(entry, Entry::Event(Fact::RequestFinished { outcome, status, .. }) if *outcome == expected_request && *status == expected_status)).count(), eq(1));
}

#[googletest::test]
#[tokio::test]
async fn shutdown_drains_active_request_before_final_event_and_flush() {
    controlled_request_shutdown(false).await;
}
#[googletest::test]
#[tokio::test]
async fn deadline_drops_active_request_before_final_event_and_flush() {
    controlled_request_shutdown(true).await;
}

#[googletest::test]
#[tokio::test]
async fn real_postgres_startup_migration_and_failure_stages_are_truthful() {
    use nicknamer_server::observations::{
        ShutdownOutcome,
        lifecycle::{RequestWork, serve_until},
    };
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};
    use testcontainers_modules::testcontainers::runners::AsyncRunner;
    let container = test_images::postgres().await.start().await.unwrap();
    let url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    );
    let recorder = Arc::new(Recorder::default());
    let dispatcher = Arc::new(Dispatcher::new(vec![recorder.clone()]));
    let prepared = nicknamer_server::web::prepare_server(config(url.clone()), dispatcher.clone())
        .await
        .unwrap();
    assert_that!(
        recorder
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|fact| matches!(
                fact,
                Fact::StartupStageFinished {
                    outcome: Outcome::Succeeded,
                    ..
                }
            ))
            .count(),
        eq(4)
    );
    assert_that!(
        recorder
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|fact| matches!(fact, Fact::ApplicationReady)),
        eq(false)
    );
    let outcome = serve_until(
        prepared.listener,
        prepared.app,
        RequestWork::default(),
        dispatcher,
        std::future::ready(()),
        std::future::pending(),
    )
    .await;
    assert_that!(outcome, eq(ShutdownOutcome::Drained));
    assert_that!(
        recorder
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|fact| matches!(fact, Fact::ApplicationReady))
            .count(),
        eq(1)
    );
    let db = Database::connect(url.clone()).await.unwrap();
    for sql in [
        "DROP TABLE seaql_migrations",
        "CREATE TABLE seaql_migrations (broken integer)",
    ] {
        db.execute_raw(Statement::from_string(
            DatabaseBackend::Postgres,
            sql.to_string(),
        ))
        .await
        .unwrap();
    }
    let recorder = Arc::new(Recorder::default());
    let dispatcher = Arc::new(Dispatcher::new(vec![recorder.clone()]));
    assert_that!(
        nicknamer_server::web::prepare_server(config(url), dispatcher)
            .await
            .is_err(),
        eq(true)
    );
    let events = recorder.0.lock().unwrap();
    assert_that!(
        events.iter().any(|fact| matches!(
            fact,
            Fact::StartupStageFinished {
                stage: StartupStage::Migration,
                outcome: Outcome::Failed,
                ..
            }
        )),
        eq(true)
    );
    assert_that!(
        events
            .iter()
            .any(|fact| matches!(fact, Fact::ApplicationReady)),
        eq(false)
    );
}
