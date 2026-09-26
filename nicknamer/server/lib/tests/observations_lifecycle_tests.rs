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

async fn controlled_request_shutdown(timeout: bool, signal_failed: bool) {
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
        async move {
            let _ = stopped.await;
            if signal_failed {
                Err(nicknamer_server::observations::lifecycle::SignalFailure)
            } else {
                Ok(())
            }
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
    let response = read_response_head(&socket).await;
    let response = std::str::from_utf8(&response).unwrap();
    if timeout {
        assert_that!(
            response.is_empty() || response.split_whitespace().nth(1) == Some("503"),
            eq(true)
        );
    } else {
        assert_that!(response.split_whitespace().nth(1), eq(Some("200")));
    }

    let expected = if signal_failed {
        ShutdownOutcome::Failed
    } else if timeout {
        ShutdownOutcome::TimedOut
    } else {
        ShutdownOutcome::Drained
    };
    assert_that!(outcome, eq(expected));
    assert_shutdown_order(&entries.lock().unwrap(), timeout, expected);
}

async fn read_response_head(socket: &tokio::net::TcpStream) -> Vec<u8> {
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
    response
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
    assert_that!(entries.iter().filter(|entry| matches!(entry, Entry::Event(Fact::RequestFinished { outcome, status, .. }) if *outcome == expected_request && (*status == expected_status || (timeout && *status == 0)))).count(), eq(1));
}

#[googletest::test]
#[tokio::test]
async fn shutdown_drains_active_request_before_final_event_and_flush() {
    controlled_request_shutdown(false, false).await;
}
#[googletest::test]
#[tokio::test]
async fn deadline_drops_active_request_before_final_event_and_flush() {
    controlled_request_shutdown(true, false).await;
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
        std::future::ready(Ok(())),
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

#[googletest::test]
#[tokio::test]
async fn signal_failure_still_drains_and_flushes_but_reports_failure() {
    controlled_request_shutdown(false, true).await;
}

#[googletest::test]
#[tokio::test]
async fn signal_failure_remains_failed_after_deadline_cancellation() {
    controlled_request_shutdown(true, true).await;
}

#[googletest::test]
#[tokio::test]
async fn deadline_joins_request_delayed_before_observation_middleware() {
    use nicknamer_server::observations::{
        ShutdownOutcome,
        lifecycle::{RequestWork, serve_until},
    };
    let entries = Arc::new(Mutex::new(Vec::new()));
    let dispatcher = Arc::new(Dispatcher::new(vec![Arc::new(OrderedRecorder(
        entries.clone(),
    ))]));
    let observer: nicknamer_server::observations::SharedObserver = dispatcher.clone();
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let app = axum::Router::new()
        .route("/health", axum::routing::get(|| async { "OK" }))
        .layer(axum::middleware::from_fn_with_state(
            observer,
            nicknamer_server::observations::http::observe_request,
        ))
        .layer(axum::middleware::from_fn({
            let started = started.clone();
            let release = release.clone();
            let entries = entries.clone();
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let started = started.clone();
                let release = release.clone();
                let entries = entries.clone();
                async move {
                    let _dropped = Dropped(entries);
                    started.notify_one();
                    release.notified().await;
                    next.run(request).await
                }
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(serve_until(
        listener,
        app,
        RequestWork::default(),
        dispatcher,
        async {
            stopped.await.unwrap();
            Ok(())
        },
        std::future::ready(()),
    ));
    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket.writable().await.unwrap();
    socket
        .try_write(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    started.notified().await;
    stop.send(()).unwrap();
    assert_that!(
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap(),
        eq(ShutdownOutcome::TimedOut)
    );
    // The connection must have closed before final lifecycle delivery.
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        read_response_head(&socket),
    )
    .await
    .unwrap();
    assert_that!(
        entries.lock().unwrap().as_slice(),
        eq(&[
            Entry::Event(Fact::ApplicationReady),
            Entry::Dropped,
            Entry::Event(Fact::ShutdownFinished {
                outcome: ShutdownOutcome::TimedOut
            }),
            Entry::Flushed,
        ])
    );
}

#[googletest::test]
#[tokio::test]
async fn deadline_closes_connection_with_unread_response_body() {
    use nicknamer_server::observations::{
        ShutdownOutcome,
        lifecycle::{RequestWork, serve_until},
    };
    let entries = Arc::new(Mutex::new(Vec::new()));
    let dispatcher = Arc::new(Dispatcher::new(vec![Arc::new(OrderedRecorder(
        entries.clone(),
    ))]));
    let observer: nicknamer_server::observations::SharedObserver = dispatcher.clone();
    let prepared = Arc::new(tokio::sync::Notify::new());
    let app = axum::Router::new()
        .route(
            "/",
            axum::routing::get({
                let prepared = prepared.clone();
                move || {
                    let prepared = prepared.clone();
                    async move {
                        prepared.notify_one();
                        vec![b'x'; 32 * 1024 * 1024]
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
    let server = tokio::spawn(serve_until(
        listener,
        app,
        RequestWork::default(),
        dispatcher,
        async {
            stopped.await.unwrap();
            Ok(())
        },
        std::future::ready(()),
    ));
    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket.writable().await.unwrap();
    socket
        .try_write(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    prepared.notified().await;
    stop.send(()).unwrap();
    assert_that!(
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap(),
        eq(ShutdownOutcome::TimedOut)
    );
    assert_that!(entries.lock().unwrap().last(), eq(Some(&Entry::Flushed)));
    // The server must close the accepted connection, not detach a task still writing its body.
    let received = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut total = 0;
        loop {
            socket.readable().await.unwrap();
            let mut chunk = [0; 8192];
            match socket.try_read(&mut chunk) {
                Ok(0) => break,
                Ok(size) => total += size,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("read failed: {error}"),
            }
        }
        total
    })
    .await
    .unwrap();
    assert_that!(received < 32 * 1024 * 1024, eq(true));
}

#[derive(Default)]
struct RequestRecorder(Mutex<Vec<Observation>>);
impl ObservationListener for RequestRecorder {
    fn on_event(&self, event: &Observation) -> Result<(), DeliveryError> {
        self.0.lock().unwrap().push(event.clone());
        Ok(())
    }
    fn flush(&self) -> Result<(), DeliveryError> {
        Ok(())
    }
}

#[googletest::test]
#[tokio::test]
async fn request_entering_after_cancellation_records_one_aborted_response() {
    use nicknamer_server::observations::{
        Method, RequestOutcome, Route,
        lifecycle::{RequestWork, serve_until},
    };
    use tower::ServiceExt;
    let work = RequestWork::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    serve_until(
        listener,
        axum::Router::new(),
        work.clone(),
        Arc::new(Dispatcher::new(vec![])),
        std::future::ready(Ok(())),
        std::future::ready(()),
    )
    .await;
    let recorder = Arc::new(RequestRecorder::default());
    let observer: nicknamer_server::observations::SharedObserver =
        Arc::new(Dispatcher::new(vec![recorder.clone()]));
    let app = axum::Router::new()
        .route("/health", axum::routing::get(|| async { "OK" }))
        .layer(axum::middleware::from_fn_with_state(
            observer,
            nicknamer_server::observations::http::observe_request,
        ))
        .layer(axum::Extension(work));
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_that!(
        response.status(),
        eq(axum::http::StatusCode::SERVICE_UNAVAILABLE)
    );
    let events = recorder.0.lock().unwrap();
    assert_that!(events.len(), eq(1));
    assert_that!(events[0].context.request_id.is_some(), eq(true));
    assert_that!(events[0].context.duration.is_some(), eq(true));
    assert_that!(
        &events[0].fact,
        eq(&Fact::RequestFinished {
            route: Route::Health,
            method: Method::Get,
            status: 503,
            outcome: RequestOutcome::Aborted
        })
    );
}

struct BlockingCompletionRecorder {
    entries: Arc<Mutex<Vec<Entry>>>,
    entered: tokio::sync::Notify,
    released: (Mutex<bool>, std::sync::Condvar),
}
impl ObservationListener for BlockingCompletionRecorder {
    fn on_event(&self, event: &Observation) -> Result<(), DeliveryError> {
        if matches!(event.fact, Fact::RequestFinished { .. }) {
            self.entered.notify_one();
            let (released, changed) = &self.released;
            let mut released = released.lock().unwrap();
            while !*released {
                released = changed.wait(released).unwrap();
            }
        }
        self.entries
            .lock()
            .unwrap()
            .push(Entry::Event(event.fact.clone()));
        Ok(())
    }
    fn flush(&self) -> Result<(), DeliveryError> {
        self.entries.lock().unwrap().push(Entry::Flushed);
        Ok(())
    }
}

#[googletest::test]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deadline_waits_for_completion_delivery_on_another_runtime_worker() {
    use nicknamer_server::observations::{
        ShutdownOutcome,
        lifecycle::{RequestWork, serve_until},
    };
    let entries = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::new(BlockingCompletionRecorder {
        entries: entries.clone(),
        entered: tokio::sync::Notify::new(),
        released: (Mutex::new(false), std::sync::Condvar::new()),
    });
    let dispatcher = Arc::new(Dispatcher::new(vec![recorder.clone()]));
    let observer: nicknamer_server::observations::SharedObserver = dispatcher.clone();
    let started = Arc::new(tokio::sync::Notify::new());
    let app = axum::Router::new()
        .route(
            "/",
            axum::routing::get({
                let started = started.clone();
                move || {
                    let started = started.clone();
                    async move {
                        started.notify_one();
                        std::future::pending::<&'static str>().await
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
    let mut server = tokio::spawn(serve_until(
        listener,
        app,
        RequestWork::default(),
        dispatcher,
        async {
            stopped.await.unwrap();
            Ok(())
        },
        std::future::ready(()),
    ));
    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket.writable().await.unwrap();
    socket
        .try_write(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    started.notified().await;
    stop.send(()).unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        recorder.entered.notified(),
    )
    .await
    .unwrap();
    let early = tokio::time::timeout(std::time::Duration::from_millis(100), &mut server).await;
    // Release before assertions so a failure cannot strand the runtime worker.
    *recorder.released.0.lock().unwrap() = true;
    recorder.released.1.notify_one();
    assert_that!(early.is_err(), eq(true));
    assert_that!(
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap(),
        eq(ShutdownOutcome::TimedOut)
    );
    let entries = entries.lock().unwrap();
    assert_that!(entries.len(), eq(4));
    assert_that!(
        matches!(entries[1], Entry::Event(Fact::RequestFinished { .. })),
        eq(true)
    );
    assert_that!(
        &entries[2],
        eq(&Entry::Event(Fact::ShutdownFinished {
            outcome: ShutdownOutcome::TimedOut
        }))
    );
    assert_that!(entries.last(), eq(Some(&Entry::Flushed)));
}
