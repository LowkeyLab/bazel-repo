use nicknamer_server::observations::{AccessReason, Route};
mod common;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use common::observations::RecordingObserver;
use googletest::prelude::*;
use nicknamer_server::{
    auth::AuthState,
    config::Config,
    name::{NameService, web::NameState},
    observations::{AuthenticationOutcome, Channel, Fact, RequestOutcome},
    web::create_app,
};
use std::sync::Arc;
use tower::ServiceExt;

async fn setup() -> (
    Router,
    Arc<RecordingObserver>,
    Arc<sea_orm::DatabaseConnection>,
) {
    let db = Arc::new(common::setup_db_with_global_container().await.unwrap());
    let (recorder, observer) = RecordingObserver::shared();
    let config = Config {
        db_url: String::new(),
        port: 8080,
        admin_username: "admin".into(),
        admin_password: "password".into(),
        jwt_secret: "test-secret".into(),
    };
    let auth = Arc::new(AuthState::from_config(&config, observer.clone()));
    let names = Arc::new(NameState {
        db: db.clone(),
        observer: observer.clone(),
    });
    (create_app(auth, names, observer), recorder, db)
}
fn request(method: &str, path: &str) -> axum::http::request::Builder {
    Request::builder().method(method).uri(path)
}
#[googletest::test]
#[tokio::test]
async fn browser_cookie_reuse_is_not_another_credential_attempt() {
    let (app, recorder, _) = setup().await;
    let response = app
        .clone()
        .oneshot(
            request("POST", "/login")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("username=admin&password=password"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_that!(response.status().as_u16(), eq(200));
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    for path in ["/names", "/login"] {
        let response = app
            .clone()
            .oneshot(
                request("GET", path)
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_that!(response.status().as_u16(), eq(200));
    }
    let response = app
        .oneshot(
            request("POST", "/login")
                .header("cookie", cookie)
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("username=wrong&password=wrong"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_that!(response.status().as_u16(), eq(200));
    let events = recorder.events();
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(
                e.fact,
                Fact::AuthenticationFinished {
                    channel: Channel::Web,
                    outcome: AuthenticationOutcome::Accepted,
                    ..
                }
            ))
            .count(),
        eq(1)
    );
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(e.fact, Fact::AuthenticationFinished { .. }))
            .count(),
        eq(1)
    );
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(
                e.fact,
                Fact::RequestFinished {
                    outcome: RequestOutcome::Completed,
                    ..
                }
            ))
            .count(),
        eq(4)
    );
}
#[googletest::test]
#[tokio::test]
async fn api_token_reuse_correlates_committed_mutation_and_public_routes() {
    let (app, recorder, db) = setup().await;
    let login = app
        .clone()
        .oneshot(
            request("POST", "/api/v1/login")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"username":"admin","password":"password"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_that!(login.status().as_u16(), eq(200));
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(login.into_body(), usize::MAX).await.unwrap()).unwrap();
    let token = body["token"].as_str().unwrap();
    let response = app
        .clone()
        .oneshot(
            request("POST", "/api/v1/names")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"discord_id":1,"name":"secret-name","server_id":"secret-server"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_that!(response.status().as_u16(), eq(201));
    for (path, status) in [
        ("/api/v1/names", 200),
        ("/api/v1/names/export", 200),
        ("/health", 200),
        ("/api/names", 404),
    ] {
        let response = app
            .clone()
            .oneshot(
                request("GET", path)
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_that!(response.status().as_u16(), eq(status));
    }
    assert_that!(
        NameService::new(&db).get_all_names().await.unwrap().len(),
        eq(1)
    );
    let events = recorder.events();
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(
                e.fact,
                Fact::AuthenticationFinished {
                    channel: Channel::Api,
                    outcome: AuthenticationOutcome::Accepted,
                    ..
                }
            ))
            .count(),
        eq(1)
    );
    let routes: Vec<_> = events
        .iter()
        .filter_map(|e| match e.fact {
            Fact::RequestFinished { route, .. } => Some(route),
            _ => None,
        })
        .collect();
    assert_that!(
        routes,
        eq(&vec![
            Route::ApiLogin,
            Route::ApiNames,
            Route::ApiNames,
            Route::ApiExport,
            Route::Health,
            Route::Other
        ])
    );
    let mutation = events
        .iter()
        .find(|e| matches!(e.fact, Fact::NameMutationFinished { .. }))
        .unwrap();
    assert_that!(mutation.context.request_id.is_some(), eq(true));
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(e.fact, Fact::RequestFinished { .. }))
            .count(),
        eq(6)
    );
    assert_that!(
        events.iter().any(
            |e| matches!(e.fact, Fact::RequestFinished { status: 201, .. })
                && e.context.request_id == mutation.context.request_id
                && Some(e.context.operation_id) == mutation.context.parent_operation_id
        ),
        eq(true)
    );
}
#[googletest::test]
#[tokio::test]
async fn denied_mutations_leave_database_unchanged_and_record_denial() {
    let (app, recorder, db) = setup().await;
    for (path, status, header, value) in [
        ("/names", 303, "cookie", ""),
        ("/names", 303, "cookie", "auth_token=invalid"),
        ("/api/v1/names", 401, "authorization", ""),
        ("/api/v1/names", 401, "authorization", "Bearer invalid"),
    ] {
        let builder = request("POST", path);
        let builder = if value.is_empty() {
            builder
        } else {
            builder.header(header, value)
        };
        let response = app
            .clone()
            .oneshot(
                builder
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"discord_id":1,"name":"secret","server_id":"server"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_that!(response.status().as_u16(), eq(status));
    }
    assert_that!(
        NameService::new(&db).get_all_names().await.unwrap().len(),
        eq(0)
    );
    let events = recorder.events();
    for reason in [AccessReason::Missing, AccessReason::Invalid] {
        assert_that!(events.iter().filter(|e| matches!(e.fact, Fact::AccessDenied { reason: actual, .. } if actual == reason)).count(), eq(2));
    }

    assert_that!(
        events
            .iter()
            .filter(|e| matches!(e.fact, Fact::AccessDenied { .. }))
            .count(),
        eq(4)
    );
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(e.fact, Fact::AuthenticationFinished { .. }))
            .count(),
        eq(0)
    );
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(e.fact, Fact::RequestFinished { .. }))
            .count(),
        eq(4)
    );
}
#[googletest::test]
#[tokio::test]
async fn concurrent_requests_keep_context_across_await_and_cancellation_is_aborted() {
    use nicknamer_server::observations::http::{current_context, observe_request};
    let (recorder, observer) = RecordingObserver::shared();
    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let release = barrier.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let app = Router::new()
        .route(
            "/wait",
            axum::routing::get(move || {
                let barrier = barrier.clone();
                let tx = tx.clone();
                async move {
                    let before = current_context();
                    tx.send(before.clone()).unwrap();
                    barrier.wait().await;
                    assert_that!(current_context().request_id, eq(before.request_id));
                    "done"
                }
            }),
        )
        .layer(axum::middleware::from_fn_with_state(
            observer,
            observe_request,
        ));
    let first = tokio::spawn(
        app.clone()
            .oneshot(Request::builder().uri("/wait").body(Body::empty()).unwrap()),
    );
    let second = tokio::spawn(
        app.clone()
            .oneshot(Request::builder().uri("/wait").body(Body::empty()).unwrap()),
    );
    let first_context = rx.recv().await.unwrap();
    let second_context = rx.recv().await.unwrap();
    assert_that!(first_context.request_id.is_some(), eq(true));
    assert_that!(first_context.request_id, ne(second_context.request_id));
    release.wait().await;
    assert_that!(first.await.unwrap().unwrap().status().as_u16(), eq(200));
    assert_that!(second.await.unwrap().unwrap().status().as_u16(), eq(200));
    assert_that!(current_context().request_id.is_none(), eq(true));
    let cancelled =
        tokio::spawn(app.oneshot(Request::builder().uri("/wait").body(Body::empty()).unwrap()));
    let cancelled_context = rx.recv().await.unwrap();
    cancelled.abort();
    let _ = cancelled.await;
    let events = recorder.events();
    assert_that!(events.len(), eq(3));
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(
                e.fact,
                Fact::RequestFinished {
                    outcome: RequestOutcome::Completed,
                    status: 200,
                    ..
                }
            ))
            .count(),
        eq(2)
    );
    let aborted = events
        .iter()
        .find(|e| {
            matches!(
                e.fact,
                Fact::RequestFinished {
                    outcome: RequestOutcome::Aborted,
                    status: 0,
                    ..
                }
            )
        })
        .unwrap();
    assert_that!(aborted.context.request_id, eq(cancelled_context.request_id));
}

#[googletest::test]
#[tokio::test]
async fn invalid_credentials_and_unknown_routes_have_bounded_outcomes() {
    use nicknamer_server::observations::{FailureCategory, Method, Route};
    let (app, recorder, _) = setup().await;
    for (path, content_type, body, expected_status) in [
        (
            "/login",
            "application/x-www-form-urlencoded",
            "username=wrong&password=wrong",
            200,
        ),
        (
            "/api/v1/login",
            "application/json",
            r#"{"username":"wrong","password":"wrong"}"#,
            401,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                request("POST", path)
                    .header("content-type", content_type)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_that!(response.status().as_u16(), eq(expected_status));
    }
    let response = app
        .oneshot(
            request("SECRET", "/private-path?token=secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_that!(response.status().as_u16(), eq(404));
    let events = recorder.events();
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(
                e.fact,
                Fact::AuthenticationFinished {
                    outcome: AuthenticationOutcome::Rejected,
                    category: Some(FailureCategory::InvalidCredentials),
                    ..
                }
            ))
            .count(),
        eq(2)
    );
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(e.fact, Fact::RequestFinished { .. }))
            .count(),
        eq(3)
    );
    assert_that!(
        events.iter().any(|e| matches!(
            e.fact,
            Fact::RequestFinished {
                route: Route::Other,
                method: Method::Other,
                status: 404,
                ..
            }
        )),
        eq(true)
    );
}
