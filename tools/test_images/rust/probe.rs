//! Real-container probe invoked through the provenance test's Docker proxy.
use testcontainers_modules::testcontainers::runners::AsyncRunner;

#[tokio::main]
async fn main() {
    let _container = test_images::postgres()
        .await
        .start()
        .await
        .expect("start PostgreSQL from its declared Bazel archive");
}
