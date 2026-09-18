use std::sync::Arc;

use prediction_bot::{
    domain::{Actor, Policy},
    store::{Store, migrate},
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, runners::AsyncRunner},
};

const RUNTIME_PASSWORD: &str = "announcement-test-password";

pub async fn fixture() -> (ContainerAsync<Postgres>, Arc<Store>, PgPool) {
    let container = test_images::postgres().await.start().await.unwrap();
    let host = container.get_host().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let owner_url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let owner = PgPoolOptions::new()
        .max_connections(8)
        .connect(&owner_url)
        .await
        .unwrap();
    migrate(&owner, RUNTIME_PASSWORD).await.unwrap();

    let runtime_url =
        format!("postgres://prediction_bot_app:{RUNTIME_PASSWORD}@{host}:{port}/postgres");
    let store = Store::connect(
        &runtime_url,
        42,
        Policy {
            amount: 100,
            interval: 86_400,
        },
    )
    .await
    .unwrap();

    (container, Arc::new(store), owner)
}

pub fn admin() -> Actor {
    Actor {
        user_id: 7,
        moderator: true,
        bot: false,
    }
}
