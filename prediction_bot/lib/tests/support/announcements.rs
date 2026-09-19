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

fn guild_json(permissions: u64) -> serde_json::Value {
    serde_json::json!({
        "id": "10",
        "name": "Test guild",
        "icon": null,
        "icon_hash": null,
        "splash": null,
        "discovery_splash": null,
        "owner_id": "7",
        "afk_channel_id": null,
        "afk_timeout": 60,
        "widget_enabled": false,
        "widget_channel_id": null,
        "verification_level": 0,
        "default_message_notifications": 0,
        "explicit_content_filter": 0,
        "roles": [{
            "id": "10",
            "name": "@everyone",
            "color": 0,
            "colors": {"primary_color": 0, "secondary_color": null, "tertiary_color": null},
            "hoist": false,
            "managed": false,
            "mentionable": false,
            "permissions": permissions.to_string(),
            "position": 0,
            "tags": {},
            "icon": null,
            "unicode_emoji": null
        }],
        "emojis": [],
        "features": [],
        "mfa_level": 0,
        "application_id": null,
        "system_channel_id": null,
        "system_channel_flags": 0,
        "rules_channel_id": null,
        "max_presences": null,
        "max_members": null,
        "vanity_url_code": null,
        "description": null,
        "banner": null,
        "premium_tier": 0,
        "premium_subscription_count": null,
        "preferred_locale": "en-US",
        "public_updates_channel_id": null,
        "max_video_channel_users": null,
        "max_stage_video_channel_users": null,
        "approximate_member_count": null,
        "approximate_presence_count": null,
        "welcome_screen": null,
        "nsfw_level": 0,
        "stickers": [],
        "premium_progress_bar_enabled": false,
        "safety_alerts_channel_id": null,
        "incidents_data": null
    })
}

fn channel_json(guild: u64, kind: u8, deny: u64) -> serde_json::Value {
    serde_json::json!({
        "id": "55",
        "guild_id": guild.to_string(),
        "type": kind,
        "name": "announcements",
        "position": 0,
        "permission_overwrites": [{
            "id": "10",
            "type": 0,
            "allow": "0",
            "deny": deny.to_string()
        }]
    })
}

fn bot_member_json() -> serde_json::Value {
    let mut user = serenity::all::User::default();
    user.id = serenity::all::UserId::new(99);
    user.bot = true;
    serde_json::json!({
        "user": user,
        "nick": null,
        "avatar": null,
        "banner": null,
        "roles": [],
        "joined_at": null,
        "premium_since": null,
        "deaf": false,
        "mute": false,
        "flags": 0,
        "pending": false,
        "permissions": null,
        "communication_disabled_until": null,
        "unusual_dm_activity_until": null,
        "avatar_decoration_data": null
    })
}

pub async fn mount_replayed_destination_validation(server: &wiremock::MockServer) {
    use wiremock::{
        Mock, ResponseTemplate,
        matchers::{method, path},
    };
    let channel = channel_json(10, 0, 0);
    let permissions = 1024 | 2048;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/55"))
        .respond_with(ResponseTemplate::new(200).set_body_json(channel))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(guild_json(permissions)))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/10/members/99"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bot_member_json()))
        .expect(1)
        .mount(server)
        .await;
}

/// A complete incoming interaction envelope; only the command/form data varies.
pub fn interaction_json(id: u64, data: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": id.to_string(), "application_id": "42", "guild_id": "10", "channel_id": "20",
        "token": "test-interaction-token", "version": 1, "locale": "en-US", "entitlements": [],
        "attachment_size_limit": 1000, "data": data,
        "member": {"permissions": "32", "roles": [], "deaf": false, "mute": false,
            "flags": 0, "joined_at": null, "premium_since": null,
            "user": {"id": "7", "username": "admin", "discriminator": "0", "avatar": null}},
        "user": { "id": "7", "username": "admin", "discriminator": "0", "avatar": null },
        "message": serenity::all::Message::default(),
    })
}
