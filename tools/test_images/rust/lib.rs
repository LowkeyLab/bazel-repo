//! PostgreSQL fixtures whose image bytes are supplied by Bazel.

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
/// Missing Bazel inputs or a failed import are fatal; there is no fallback image.
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
