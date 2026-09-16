//! `PostgreSQL` fixtures whose image bytes are supplied by Bazel.

use std::{env, fs};

use futures_util::TryStreamExt;
use runfiles::{Runfiles, rlocation};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{
        ContainerRequest, ImageExt, bollard::query_parameters::ImportImageOptionsBuilder,
        core::client::docker_client_instance,
    },
};
use tokio::sync::OnceCell;
use tokio_util::io::ReaderStream;

/// Load the declared image once per test process, then preserve Postgres defaults.
///
/// # Panics
///
/// Panics if Bazel image inputs are missing or invalid, Docker cannot be reached,
/// or the archive cannot be loaded. There is no fallback image.
pub async fn postgres() -> ContainerRequest<Postgres> {
    static IMAGE: OnceCell<String> = OnceCell::const_new();
    let image = IMAGE
        .get_or_init(|| async {
            let runfiles = Runfiles::create().expect("initialize Bazel image runfiles");
            let resolve = |key| {
                let path =
                    env::var(key).unwrap_or_else(|_| panic!("missing Bazel test input {key}"));
                rlocation!(runfiles, path)
                    .unwrap_or_else(|| panic!("resolve Bazel test input {key}"))
            };
            let image = fs::read_to_string(resolve("POSTGRES_IMAGE_REF"))
                .expect("read Bazel PostgreSQL image reference");
            let archive = tokio::fs::File::open(resolve("POSTGRES_IMAGE_TAR"))
                .await
                .expect("open Bazel PostgreSQL image archive");
            // Use the same daemon configuration as the container runner, including
            // Testcontainers properties and Docker TLS settings.
            let docker = docker_client_instance()
                .await
                .expect("connect to test Docker daemon");
            let mut progress = docker.import_image_stream(
                ImportImageOptionsBuilder::default().build(),
                ReaderStream::new(archive),
                None,
            );
            // Bollard converts errorDetail.message into DockerStreamError.
            while progress
                .try_next()
                .await
                .expect("import Bazel PostgreSQL image")
                .is_some()
            {}
            image.trim().to_owned()
        })
        .await;
    let (name, tag) = image
        .rsplit_once(':')
        .expect("Bazel image reference has a tag");
    Postgres::default().with_name(name).with_tag(tag)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Method, Request, header},
    };
    use futures_util::TryStreamExt;
    use testcontainers_modules::testcontainers::bollard::{
        API_DEFAULT_VERSION, Docker, errors::Error, models::BuildInfo,
        query_parameters::ImportImageOptionsBuilder,
    };
    use tokio::net::TcpListener;
    use tokio_util::io::ReaderStream;

    // Exercise the pinned client's wire behavior, including Docker's HTTP-200
    // error responses, without requiring a daemon or downloading an image.
    async fn import_response(response: &'static str) -> Result<Vec<BuildInfo>, Error> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().fallback(move |request: Request<Body>| async move {
            assert_eq!(request.method(), Method::POST);
            assert!(request.uri().path().ends_with("/images/load"));
            let body = to_bytes(request.into_body(), 1024).await.unwrap();
            assert_eq!(body.as_ref(), b"archive bytes");
            ([(header::CONTENT_TYPE, "application/json")], response)
        });
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let docker =
            Docker::connect_with_http(&format!("http://{address}"), 5, API_DEFAULT_VERSION)
                .unwrap();
        let result = docker
            .import_image_stream(
                ImportImageOptionsBuilder::default().build(),
                ReaderStream::new(Cursor::new(b"archive bytes")),
                None,
            )
            .try_collect()
            .await;
        server.abort();
        result
    }

    #[tokio::test]
    async fn loads_docker_archives_through_images_load() {
        let messages = import_response(r#"{"stream":"Loaded image: fixture:tag\n"}"#)
            .await
            .unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].stream.as_deref(),
            Some("Loaded image: fixture:tag\n")
        );
    }

    #[tokio::test]
    async fn rejects_daemon_error_in_http_success_response() {
        let error = import_response(r#"{"errorDetail":{"message":"archive/tar: invalid tar header"},"error":"archive/tar: invalid tar header"}"#).await.unwrap_err();
        assert!(
            matches!(error, Error::DockerStreamError { error } if error == "archive/tar: invalid tar header")
        );
    }

    #[tokio::test]
    async fn rejects_error_detail_without_legacy_error_field() {
        let error = import_response(r#"{"errorDetail":{"message":"no space left on device"}}"#)
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::DockerStreamError { error } if error == "no space left on device")
        );
    }
}
