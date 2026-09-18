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

pub fn discord_http(server: &wiremock::MockServer) -> serenity::http::Http {
    serenity::http::HttpBuilder::new("test-token")
        .application_id(42.into())
        .proxy(server.uri())
        .ratelimiter_disabled(true)
        .build()
}

pub fn delivered() -> wiremock::ResponseTemplate {
    let mut message = serenity::all::Message::default();
    message.id = 99.into();
    message.channel_id = 20.into();
    wiremock::ResponseTemplate::new(200).set_body_json(message)
}

pub fn clock(now: i64) -> prediction_bot::announcements::Clock {
    Arc::new(move || now)
}

pub fn restart(store: &Store) -> Arc<Store> {
    Arc::new(Store::new(
        store.pool.clone(),
        42,
        Policy {
            amount: 100,
            interval: 86_400,
        },
    ))
}

pub async fn queued(store: &Store, guild: u64, channel: u64) {
    use prediction_bot::{announcements::ConfigurationChange, domain::Command};
    store
        .execute_at(guild, "discord:join", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .configure_announcements(
            guild,
            "discord:enable",
            admin(),
            ConfigurationChange::Set {
                channel_id: channel,
            },
        )
        .await
        .unwrap();
    store
        .execute_at(
            guild,
            "discord:create",
            admin(),
            &super::create(super::FIXTURE_MARKET),
            1000,
        )
        .await
        .unwrap();
}

/// A real HTTP request remains blocked until the test releases its response.
pub struct ResponseBarrier {
    pub arrived: Arc<tokio::sync::Notify>,
    released: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
}

impl ResponseBarrier {
    pub async fn mount(
        server: &wiremock::MockServer,
        response: wiremock::ResponseTemplate,
    ) -> Self {
        let arrived = Arc::new(tokio::sync::Notify::new());
        let released = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let signal = arrived.clone();
        let gate = released.clone();
        wiremock::Mock::given(wiremock::matchers::path("/api/v10/channels/20/messages"))
            .respond_with(move |_: &wiremock::Request| {
                signal.notify_one();
                let (lock, condition) = &*gate;
                let (released, timeout) = condition
                    .wait_timeout_while(
                        lock.lock().unwrap(),
                        std::time::Duration::from_secs(20),
                        |released| !*released,
                    )
                    .unwrap();
                assert!(
                    *released && !timeout.timed_out(),
                    "response barrier was not released"
                );
                response.clone()
            })
            .mount(server)
            .await;
        Self { arrived, released }
    }

    pub fn release(&self) {
        *self.released.0.lock().unwrap() = true;
        self.released.1.notify_all();
    }
}

impl Drop for ResponseBarrier {
    fn drop(&mut self) {
        self.release();
    }
}
