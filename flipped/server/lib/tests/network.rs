use std::io::{Cursor, Write};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use base64::Engine;
use examination_proto::flipped::examination::v1 as pb;
use examination_proto::flipped::examination::v1::examination_service_client::ExaminationServiceClient;
use examination_proto::flipped::examination::v1::examination_service_server::ExaminationServiceServer;
use flipped_server::credentials::{CredentialService, OAuthTokenResponse};
use flipped_server::grpc::ExaminationGrpcService;
use flipped_server::oauth::{OAuthState, router};
use flipped_server::observability::{EventDispatcher, RecordingEventListener, ServiceIdentity};
use flipped_server::store::InMemoryStore;
use flipped_server::{Application, Config};
use rand_08::rngs::OsRng;
use rsa::RsaPrivateKey;
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rusqlite::{Connection, params};
use tempfile::NamedTempFile;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, transport::Server};
use uuid::Uuid;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const CLIENT_ID: &str = "nuxt-gateway";
const CLIENT_SECRET: &str = "integration-client-secret";
const AUDIENCE: &str = "flipped-session";

#[tokio::test]
async fn real_network_happy_path_preserves_role_projection_and_shutdown() {
    let grpc_listener = TcpListener::bind("127.0.0.1:0").await.expect("gRPC bind");
    let grpc_addr = grpc_listener.local_addr().expect("gRPC address");
    let http_listener = TcpListener::bind("127.0.0.1:0").await.expect("HTTP bind");
    let http_addr = http_listener.local_addr().expect("HTTP address");
    let config = test_config(grpc_addr, http_addr);
    let credentials = Arc::new(CredentialService::new(&config).expect("credential service"));
    let events = EventDispatcher::new(
        ServiceIdentity {
            name: "flipped-network-test".to_owned(),
            version: "test".to_owned(),
            environment: "test".to_owned(),
            instance_id: "network-test".to_owned(),
        },
        vec![Arc::new(RecordingEventListener::default())],
    );
    let application = Arc::new(Application::new(
        InMemoryStore::default(),
        Arc::clone(&credentials),
        events,
        config.import_limits.clone(),
        config.oauth_client_id.clone(),
        config.oauth_audience.clone(),
        config.session_ttl,
        config.redemption_retry,
        config.command_result_capacity,
        config.event_stream_capacity,
        config.max_sessions,
        config.max_concurrent_imports,
        config.max_global_watches,
        config.max_watches_per_session,
        config.tombstone_retention,
        config.observability_hmac_key,
    ));

    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    let grpc_service =
        ExaminationServiceServer::new(ExaminationGrpcService::new(Arc::clone(&application)));
    health_reporter
        .set_serving::<ExaminationServiceServer<ExaminationGrpcService>>()
        .await;
    let (grpc_shutdown_tx, grpc_shutdown_rx) = oneshot::channel();
    let grpc_task = tokio::spawn(async move {
        Server::builder()
            .add_service(health_service)
            .add_service(grpc_service)
            .serve_with_incoming_shutdown(TcpListenerStream::new(grpc_listener), async {
                let _ = grpc_shutdown_rx.await;
            })
            .await
    });

    let oauth = router(OAuthState {
        application: Arc::clone(&application),
        issuer: config.oauth_issuer.clone(),
        client_id: config.oauth_client_id.clone(),
        client_secret: config.oauth_client_secret.clone(),
        readiness: Arc::new(AtomicBool::new(true)),
    });
    let (http_shutdown_tx, http_shutdown_rx) = oneshot::channel();
    let http_task = tokio::spawn(async move {
        axum::serve(http_listener, oauth)
            .with_graceful_shutdown(async {
                let _ = http_shutdown_rx.await;
            })
            .await
    });

    let grpc_endpoint = format!("http://{grpc_addr}");
    let health_channel = tonic::transport::Endpoint::from_shared(grpc_endpoint.clone())
        .expect("health endpoint")
        .connect()
        .await
        .expect("health channel");
    let mut health = tonic_health::pb::health_client::HealthClient::new(health_channel);
    let health_response = health
        .check(tonic_health::pb::HealthCheckRequest {
            service: "flipped.examination.v1.ExaminationService".to_owned(),
        })
        .await
        .expect("health check")
        .into_inner();
    assert_eq!(
        health_response.status,
        tonic_health::pb::health_check_response::ServingStatus::Serving as i32
    );

    let metadata = raw_http(
        http_addr,
        "GET /.well-known/oauth-authorization-server HTTP/1.1\r\nHost: flipped-server\r\nConnection: close\r\n\r\n".to_owned(),
    )
    .await;
    assert_eq!(metadata.status, 200);

    let mut client = ExaminationServiceClient::connect(grpc_endpoint)
        .await
        .expect("examination client");
    let package = ordinary_package();
    let upload = tokio_stream::iter(vec![
        pb::UploadChunk {
            chunk: Some(pb::upload_chunk::Chunk::Metadata(pb::UploadMetadata {
                package_extension: ".apkg".to_owned(),
                declared_size_bytes: package.len() as u64,
            })),
        },
        pb::UploadChunk {
            chunk: Some(pb::upload_chunk::Chunk::Data(
                package[..package.len() / 2].to_vec(),
            )),
        },
        pb::UploadChunk {
            chunk: Some(pb::upload_chunk::Chunk::Data(
                package[package.len() / 2..].to_vec(),
            )),
        },
    ]);
    let created = client
        .create_session(Request::new(upload))
        .await
        .expect("create transport")
        .into_inner();
    let created = match created.result.expect("create result") {
        pb::create_session_response::Result::Success(success) => success,
        pb::create_session_response::Result::Error(error) => {
            panic!("supported APKG rejected with {}", error.code)
        }
    };
    assert_eq!(created.card_count, 2);
    let initial = created.initial_snapshot.as_ref().expect("initial snapshot");
    assert!(initial.current_card.is_none());

    let mut test_taker_stream = client
        .watch_test_taker_session(authenticated(
            pb::WatchSessionRequest {
                session_id: created.session_id.clone(),
                after_revision: 0,
            },
            &created.test_taker_access_token,
        ))
        .await
        .expect("test-taker watch")
        .into_inner();
    let first_test_taker = test_taker_stream
        .message()
        .await
        .expect("test-taker stream")
        .expect("initial test-taker item");
    assert!(matches!(
        first_test_taker.result,
        Some(pb::test_taker_watch_response::Result::Snapshot(_))
    ));

    let redemption_id = Uuid::now_v7().to_string();
    let token_body = format!(
        "grant_type=urn:ietf:params:oauth:grant-type:token-exchange&subject_token={}&subject_token_type=urn:flipped:params:oauth:token-type:examiner-invitation&requested_token_type=urn:ietf:params:oauth:token-type:access_token&audience={AUDIENCE}&scope=session:examine&flipped_redemption_id={redemption_id}",
        created.examiner_invitation,
    );
    let basic =
        base64::engine::general_purpose::STANDARD.encode(format!("{CLIENT_ID}:{CLIENT_SECRET}"));
    let token_response = raw_http(
        http_addr,
        format!(
            "POST /oauth/token HTTP/1.1\r\nHost: flipped-server\r\nAuthorization: Basic {basic}\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{token_body}",
            token_body.len(),
        ),
    )
    .await;
    assert_eq!(token_response.status, 200);
    assert_eq!(token_response.header("cache-control"), Some("no-store"));
    let examiner_token = serde_json::from_slice::<OAuthTokenResponse>(&token_response.body)
        .expect("token response")
        .access_token;

    let mut examiner_stream = client
        .watch_examiner_session(authenticated(
            pb::WatchSessionRequest {
                session_id: created.session_id.clone(),
                after_revision: 0,
            },
            &examiner_token,
        ))
        .await
        .expect("examiner watch")
        .into_inner();
    let examiner_snapshot = examiner_stream
        .message()
        .await
        .expect("examiner stream")
        .expect("initial examiner item");
    assert!(matches!(
        examiner_snapshot.result,
        Some(pb::examiner_watch_response::Result::Snapshot(_))
    ));

    let started = client
        .start_session(authenticated(command(&created.session_id), &examiner_token))
        .await
        .expect("start transport")
        .into_inner();
    let started = match started.result.expect("start result") {
        pb::start_session_response::Result::Success(success) => success,
        pb::start_session_response::Result::Error(error) => {
            panic!("start rejected with {}", error.code)
        }
    };
    let first_card = started
        .snapshot
        .expect("start snapshot")
        .current_card
        .expect("first examiner card");
    assert_eq!(first_card.front, "question 1");
    assert_eq!(first_card.back, "answer 1");

    let first_front = next_test_taker_card(&mut test_taker_stream).await;
    assert_eq!(first_front.front, "question 1");
    assert_eq!(first_front.position, 1);

    let advanced = client
        .advance_session(authenticated(command(&created.session_id), &examiner_token))
        .await
        .expect("advance transport")
        .into_inner();
    let advanced = match advanced.result.expect("advance result") {
        pb::advance_session_response::Result::Success(success) => success,
        pb::advance_session_response::Result::Error(error) => {
            panic!("advance rejected with {}", error.code)
        }
    };
    let second_card = advanced
        .snapshot
        .expect("advance snapshot")
        .current_card
        .expect("second examiner card");
    assert_eq!(second_card.front, "question 2");
    assert_eq!(second_card.back, "answer 2");
    let second_front = next_test_taker_card(&mut test_taker_stream).await;
    assert_eq!(second_front.front, "question 2");
    assert_eq!(second_front.position, 2);

    let completed = client
        .advance_session(authenticated(command(&created.session_id), &examiner_token))
        .await
        .expect("completion transport")
        .into_inner();
    let completed = match completed.result.expect("completion result") {
        pb::advance_session_response::Result::Success(success) => success,
        pb::advance_session_response::Result::Error(error) => {
            panic!("completion rejected with {}", error.code)
        }
    };
    assert_eq!(
        completed.snapshot.expect("completion snapshot").status,
        pb::SessionStatus::Completed as i32
    );

    let ended = client
        .end_session(authenticated(command(&created.session_id), &examiner_token))
        .await
        .expect("end transport")
        .into_inner();
    let ended = match ended.result.expect("end result") {
        pb::end_session_response::Result::Success(success) => success,
        pb::end_session_response::Result::Error(error) => {
            panic!("end rejected with {}", error.code)
        }
    };
    assert_eq!(
        ended.snapshot.expect("end snapshot").status,
        pb::SessionStatus::Terminated as i32
    );
    assert_stream_reaches_terminated(&mut test_taker_stream).await;

    drop(examiner_stream);
    drop(test_taker_stream);
    let _ = grpc_shutdown_tx.send(());
    let _ = http_shutdown_tx.send(());
    tokio::time::timeout(Duration::from_secs(5), grpc_task)
        .await
        .expect("gRPC shutdown deadline")
        .expect("gRPC task join")
        .expect("gRPC shutdown");
    tokio::time::timeout(Duration::from_secs(5), http_task)
        .await
        .expect("HTTP shutdown deadline")
        .expect("HTTP task join")
        .expect("HTTP shutdown");
}

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

async fn raw_http(address: std::net::SocketAddr, request: String) -> HttpResponse {
    let mut connection = TcpStream::connect(address).await.expect("HTTP connect");
    connection
        .write_all(request.as_bytes())
        .await
        .expect("HTTP request write");
    let mut bytes = Vec::new();
    connection
        .read_to_end(&mut bytes)
        .await
        .expect("HTTP response read");
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("HTTP header terminator");
    let head = std::str::from_utf8(&bytes[..split]).expect("HTTP response headers");
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .expect("HTTP status line")
        .split_whitespace()
        .nth(1)
        .expect("HTTP status")
        .parse()
        .expect("numeric HTTP status");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_owned(), value.trim().to_owned()))
        .collect();
    HttpResponse {
        status,
        headers,
        body: bytes[split + 4..].to_vec(),
    }
}

fn authenticated<T>(message: T, token: &str) -> Request<T> {
    let mut request = Request::new(message);
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}")
            .parse()
            .expect("authorization metadata"),
    );
    request
}

fn command(session_id: &str) -> pb::SessionCommandRequest {
    pb::SessionCommandRequest {
        session_id: session_id.to_owned(),
        command_id: Uuid::now_v7().to_string(),
    }
}

async fn next_test_taker_card(
    stream: &mut tonic::Streaming<pb::TestTakerWatchResponse>,
) -> pb::CardFront {
    loop {
        let item = tokio::time::timeout(Duration::from_secs(5), stream.message())
            .await
            .expect("test-taker event deadline")
            .expect("test-taker event transport")
            .expect("test-taker event");
        let Some(pb::test_taker_watch_response::Result::Event(event)) = item.result else {
            continue;
        };
        match event.payload {
            Some(pb::test_taker_session_event::Payload::Started(started)) => {
                return started.current_card.expect("started card");
            }
            Some(pb::test_taker_session_event::Payload::CardChanged(changed)) => {
                return changed.current_card.expect("changed card");
            }
            _ => {}
        }
    }
}

async fn assert_stream_reaches_terminated(
    stream: &mut tonic::Streaming<pb::TestTakerWatchResponse>,
) {
    let mut previous_revision = 0;
    loop {
        let item = tokio::time::timeout(Duration::from_secs(5), stream.message())
            .await
            .expect("terminal event deadline")
            .expect("terminal event transport")
            .expect("terminal event");
        let Some(pb::test_taker_watch_response::Result::Event(event)) = item.result else {
            continue;
        };
        assert!(event.revision > previous_revision);
        previous_revision = event.revision;
        if matches!(
            event.payload,
            Some(pb::test_taker_session_event::Payload::Ended(pb::SessionEnded {
                status
            })) if status == pb::SessionStatus::Terminated as i32
        ) {
            return;
        }
    }
}

fn test_config(grpc_addr: std::net::SocketAddr, http_addr: std::net::SocketAddr) -> Config {
    let private = RsaPrivateKey::new(&mut OsRng, 2_048).expect("test RSA key");
    let private_pem = private
        .to_pkcs8_pem(LineEnding::LF)
        .expect("test RSA PEM")
        .as_bytes()
        .to_vec();
    Config {
        grpc_addr,
        http_addr,
        oauth_issuer: format!("http://{http_addr}"),
        oauth_audience: AUDIENCE.to_owned(),
        oauth_client_id: CLIENT_ID.to_owned(),
        oauth_client_secret: CLIENT_SECRET.to_owned(),
        jwt_active_private_key: private_pem,
        jwt_active_kid: "integration-key".to_owned(),
        jwt_previous_public_key: None,
        jwt_previous_kid: None,
        invitation_hmac_key: [7; 32],
        observability_hmac_key: [8; 32],
        environment: "test".to_owned(),
        instance_id: "network-test".to_owned(),
        service_version: "test".to_owned(),
        otlp_endpoint: None,
        otel_resource_attributes: None,
        otel_traces_sampler: None,
        otel_traces_sampler_arg: None,
        import_limits: flipped_anki::ImportLimits::default(),
        invitation_ttl: Duration::from_secs(900),
        session_ttl: Duration::from_secs(14_400),
        jwt_ttl: Duration::from_secs(14_400),
        redemption_retry: Duration::from_secs(60),
        command_result_capacity: 16,
        event_queue_capacity: 1_024,
        event_stream_capacity: 64,
        max_concurrent_imports: 2,
        max_global_watches: 8,
        max_watches_per_session: 4,
        max_sessions: 4,
        tombstone_retention: Duration::from_secs(60),
        observability_flush_timeout: Duration::from_secs(5),
        cleanup_interval: Duration::from_secs(60),
    }
}

fn ordinary_package() -> Vec<u8> {
    let database = NamedTempFile::new().expect("temporary database");
    let connection = Connection::open(database.path()).expect("open database");
    connection
        .execute_batch(
            "CREATE TABLE col (ver INTEGER NOT NULL, models TEXT NOT NULL);\n\
             CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, flds BLOB NOT NULL);\n\
             CREATE TABLE cards (id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, ord INTEGER NOT NULL, type INTEGER NOT NULL, queue INTEGER NOT NULL, due INTEGER NOT NULL, ivl INTEGER NOT NULL, factor INTEGER NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL);",
        )
        .expect("schema");
    let models = serde_json::json!({
        "1": {
            "type": 0,
            "flds": [{"name": "Front", "ord": 0}, {"name": "Back", "ord": 1}],
            "tmpls": [{
                "name": "Card 1",
                "ord": 0,
                "qfmt": "{{Front}}",
                "afmt": "{{FrontSide}}<hr id=answer>{{Back}}"
            }],
            "css": ""
        }
    })
    .to_string();
    connection
        .execute("INSERT INTO col(ver, models) VALUES (11, ?1)", [&models])
        .expect("collection row");
    for id in 1..=2_i64 {
        let fields = format!("question {id}\u{1f}answer {id}");
        connection
            .execute(
                "INSERT INTO notes(id, mid, flds) VALUES (?1, 1, ?2)",
                params![id, fields.as_bytes()],
            )
            .expect("note row");
        connection
            .execute(
                "INSERT INTO cards(id, nid, ord, type, queue, due, ivl, factor, reps, lapses) VALUES (?1, ?1, 0, 0, 0, 0, 0, 0, 0, 0)",
                [id],
            )
            .expect("card row");
    }
    drop(connection);

    let cursor = Cursor::new(Vec::new());
    let mut archive = ZipWriter::new(cursor);
    archive
        .start_file("collection.anki21", SimpleFileOptions::default())
        .expect("database entry");
    archive
        .write_all(&std::fs::read(database.path()).expect("database bytes"))
        .expect("database content");
    archive.finish().expect("finish ZIP").into_inner()
}
