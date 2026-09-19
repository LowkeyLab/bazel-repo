use std::{env::VarError, future::pending, time::Duration};

use anyhow::{Result, anyhow};
use googletest::{
    assert_that,
    matchers::{contains_substring, eq},
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
    time::timeout,
};

use super::{address, run, supervise};

#[googletest::test]
fn health_address_defaults_and_accepts_explicit_addresses() {
    assert_that!(
        address(Err(VarError::NotPresent)).unwrap().to_string(),
        eq("0.0.0.0:8080")
    );
    for value in ["127.0.0.1:8081", "[::1]:8080"] {
        assert_that!(address(Ok(value.into())).unwrap().to_string(), eq(value));
    }
}

#[googletest::test]
fn health_address_rejects_invalid_values() {
    for value in ["", "localhost:8080", "127.0.0.1", "127.0.0.1:65536"] {
        assert_that!(address(Ok(value.into())).is_err(), eq(true));
    }
    assert_that!(
        address(Err(VarError::NotUnicode("invalid".into()))).is_err(),
        eq(true)
    );
}

// A server started only after initialization, a wrong route, or a leaked listener
// must fail this test. The controlled future substitutes only bot progress.
#[tokio::test]
async fn serves_during_initialization_and_closes_after_bot_completion() {
    for outcome in [Ok(()), Err(anyhow!("bot initialization failed"))] {
        timeout(Duration::from_secs(10), async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (finish, finished) = oneshot::channel::<Result<()>>();
            let task = tokio::spawn(run(listener, async { finished.await.unwrap() }));
            let response = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .get(format!("http://{address}/healthz"))
                .send()
                .await
                .unwrap();
            assert_that!(response.status().as_u16(), eq(200));
            assert_that!(
                response.headers()["content-type"].to_str().unwrap(),
                eq("text/plain; charset=utf-8")
            );
            assert_that!(response.text().await.unwrap(), eq("ok"));
            let expected = outcome.as_ref().err().map(ToString::to_string);
            finish.send(outcome).unwrap();
            let actual = task.await.unwrap().err().map(|error| error.to_string());
            assert_that!(actual, eq(&expected));
            assert_that!(TcpStream::connect(address).await.is_err(), eq(true));
        })
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn unexpected_server_exit_is_fatal() {
    for outcome in [Ok(()), Err(std::io::Error::other("listener failed"))] {
        let (shutdown, _receiver) = oneshot::channel();
        let result = supervise(pending(), async { outcome }, shutdown).await;
        assert_that!(result.is_err(), eq(true));
        if let Err(error) = result {
            assert_that!(error.to_string(), contains_substring("health server"));
        }
    }
}

#[tokio::test]
async fn server_drain_failure_is_reported_after_bot_success() {
    let (shutdown, stopped) = oneshot::channel();
    let server = async {
        stopped.await.unwrap();
        Err(std::io::Error::other("drain failed"))
    };
    let result = supervise(async { Ok(()) }, server, shutdown).await;
    assert_that!(
        result.unwrap_err().to_string(),
        contains_substring("health server shutdown failed")
    );
}

#[tokio::test]
async fn unresponsive_http_drain_cannot_prevent_process_exit() {
    let (shutdown, _stopped) = oneshot::channel();
    let result = timeout(
        Duration::from_secs(10),
        supervise(async { Ok(()) }, pending(), shutdown),
    )
    .await
    .unwrap();
    assert_that!(
        result.unwrap_err().to_string(),
        contains_substring("health server shutdown timed out")
    );
}
