use googletest::{
    assert_that,
    matchers::{
        anything, contains, contains_substring, eq, err, is_empty, matches_pattern, not,
        starts_with,
    },
};
use prediction_bot::types::{ChannelId, EventRevision, GuildId, OutcomeIndex, Points, UserId};
use prediction_bot::{
    announcements::ConfigurationChange,
    domain::{Actor, Command, Policy},
    store::StoreError,
};
use serde_json::json;

#[path = "support/announcements.rs"]
mod support;

use support::{admin, fixture};

fn member() -> Actor {
    Actor {
        user_id: UserId(8),
        moderator: false,
        bot: false,
    }
}

const RESOLVED_MARKET: &str = "00000000-0000-4000-8000-000000000001";
const CANCELLED_MARKET: &str = "00000000-0000-4000-8000-000000000002";
const FIXTURE_MARKET: &str = "00000000-0000-4000-8000-000000000003";
const HISTORICAL_MARKET: &str = "00000000-0000-4000-8000-000000000004";
const PAUSED_MARKET: &str = "00000000-0000-4000-8000-000000000005";
const DISABLED_MARKET: &str = "00000000-0000-4000-8000-000000000006";
const CONCURRENT_MARKET: &str = "00000000-0000-4000-8000-000000000007";

#[googletest::test]
#[tokio::test]
async fn resolve_widget_settles_only_after_confirmation_and_recovers_redelivery() {
    use prediction_bot::{discord::handle_interaction, domain::Status};
    use serenity::all::Interaction;
    use support::interaction_json;

    let (_container, store, owner) = fixture().await;
    for actor in [admin(), member()] {
        store
            .execute_at(
                10.into(),
                &format!("discord:90{}", actor.user_id),
                actor,
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
    }
    // A moderator resolves someone else's market through the real gateway dispatcher.
    store
        .execute_at(
            10.into(),
            "discord:901",
            member(),
            &create(RESOLVED_MARKET),
            1000,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:902",
            admin(),
            &Command::Bet {
                id: RESOLVED_MARKET.into(),
                outcome: OutcomeIndex(1),
                amount: Points(10),
            },
            1001,
        )
        .await
        .unwrap();
    support::existing_destination(&store, 10, 20).await;

    let server = MockServer::start().await;
    let http = discord_http(&server);
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(support::delivered())
        .mount(&server)
        .await;
    let slash = Interaction::Command(
        serde_json::from_value(interaction_json(
            301,
            &json!({
                "id": "1", "name": "market", "type": 1,
                "options": [{"name": "resolve", "type": 1, "options": []}]
            }),
        ))
        .unwrap(),
    );
    handle_interaction(store.clone(), &http, 99.into(), slash).await;
    let picker = last_widget_response(&server).await;
    assert_that!(
        picker["components"][0]["components"][0]["options"][0]["value"],
        eq(RESOLVED_MARKET)
    );

    let selection =
        |id, custom_id: &str, values: Vec<&str>| {
            Interaction::Component(serde_json::from_value(interaction_json(id, &json!({
            "custom_id": custom_id, "component_type": if values.is_empty() { 2 } else { 3 },
            "values": values,
        }))).unwrap())
        };
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(
            302,
            picker["components"][0]["components"][0]["custom_id"]
                .as_str()
                .unwrap(),
            vec![RESOLVED_MARKET],
        ),
    )
    .await;
    let outcomes = last_widget_response(&server).await;
    assert_that!(outcomes["embeds"][0]["title"], eq("Will it rain?"));
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(
            303,
            outcomes["components"][0]["components"][0]["custom_id"]
                .as_str()
                .unwrap(),
            vec!["1"],
        ),
    )
    .await;
    let confirmation = last_widget_response(&server).await;
    assert_that!(
        confirmation["content"].as_str().unwrap(),
        contains_substring("No")
    );
    assert_that!(
        store.view(10.into()).await.unwrap().state.markets[RESOLVED_MARKET].status,
        eq(&Status::Open)
    );

    let confirm_id = confirmation["components"][0]["components"][0]["custom_id"]
        .as_str()
        .unwrap();
    let mut lost_permission = selection(306, confirm_id, vec![]);
    if let Interaction::Component(component) = &mut lost_permission {
        component.member.as_mut().unwrap().permissions = Some(serenity::all::Permissions::empty());
    }
    handle_interaction(store.clone(), &http, 99.into(), lost_permission).await;
    assert_that!(
        last_widget_response(&server).await["content"]
            .as_str()
            .unwrap(),
        contains_substring("creator or moderator required")
    );
    assert_that!(
        store.view(10.into()).await.unwrap().state.markets[RESOLVED_MARKET].status,
        eq(&Status::Open)
    );

    let confirm = selection(304, confirm_id, vec![]);
    for _ in 0..2 {
        handle_interaction(store.clone(), &http, 99.into(), confirm.clone()).await;
        let receipt = last_widget_response(&server).await;
        assert_that!(
            receipt["content"].as_str().unwrap(),
            contains_substring("Market resolved")
        );
        assert_that!(receipt["components"], eq(&json!([])));
        assert_that!(receipt["embeds"], eq(&json!([])));
    }
    // A second click has a different interaction ID and must not settle again either.
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(305, confirm_id, vec![]),
    )
    .await;
    let stale = last_widget_response(&server).await;
    assert_that!(
        stale["content"].as_str().unwrap(),
        contains_substring("not ready to resolve")
    );
    let view = store.view(10.into()).await.unwrap();
    assert_that!(
        view.state.markets[RESOLVED_MARKET].status,
        eq(&Status::Resolved {
            outcome: OutcomeIndex(1),
            refunded: false
        })
    );
    assert_that!(view.state.accounts[&UserId(7)].balance, eq(Points(100)));
    let announcements: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM prediction_announcement_outbox WHERE snapshot ? 'Resolved'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_that!(announcements, eq(1));

    // Another resolver settles a different market after this user selected its outcome.
    store
        .execute_at(
            10.into(),
            "discord:910",
            member(),
            &create(CONCURRENT_MARKET),
            1000,
        )
        .await
        .unwrap();
    let select_market = selection(
        310,
        picker["components"][0]["components"][0]["custom_id"]
            .as_str()
            .unwrap(),
        vec![CONCURRENT_MARKET],
    );
    handle_interaction(store.clone(), &http, 99.into(), select_market).await;
    let outcomes = last_widget_response(&server).await;
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(
            311,
            outcomes["components"][0]["components"][0]["custom_id"]
                .as_str()
                .unwrap(),
            vec!["1"],
        ),
    )
    .await;
    let confirmation = last_widget_response(&server).await;
    store
        .execute(
            10.into(),
            "discord:912",
            member(),
            &Command::Resolve {
                id: CONCURRENT_MARKET.into(),
                outcome: OutcomeIndex(0),
            },
        )
        .await
        .unwrap();
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(
            313,
            confirmation["components"][0]["components"][0]["custom_id"]
                .as_str()
                .unwrap(),
            vec![],
        ),
    )
    .await;
    assert_that!(
        last_widget_response(&server).await["content"]
            .as_str()
            .unwrap(),
        contains_substring("not ready to resolve")
    );
    assert_that!(
        store.view(10.into()).await.unwrap().state.markets[CONCURRENT_MARKET].status,
        eq(&Status::Resolved {
            outcome: OutcomeIndex(0),
            refunded: true
        })
    );

    let requests = server.received_requests().await.unwrap();
    let callbacks: Vec<_> = requests
        .iter()
        .filter(|r| r.method.as_str() == "POST")
        .map(|r| r.body_json::<serde_json::Value>().unwrap())
        .collect();
    assert_that!(callbacks[0]["data"]["flags"], eq(64));
    assert_that!(
        callbacks[1..].iter().all(|body| body["type"] == 6),
        eq(true)
    );
    for response in requests.iter().filter(|r| r.method.as_str() == "PATCH") {
        assert_that!(
            response.body_json::<serde_json::Value>().unwrap()["allowed_mentions"]["parse"],
            eq(&json!([]))
        );
    }
}

async fn last_widget_response(server: &wiremock::MockServer) -> serde_json::Value {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .rev()
        .find(|request| request.method.as_str() == "PATCH")
        .unwrap()
        .body_json()
        .unwrap()
}

fn create(id: &str) -> Command {
    Command::Create {
        id: id.into(),
        question: "Will it rain?".into(),
        options: vec!["Yes".into(), "No".into()],
        closes_at: 2000,
    }
}

#[googletest::test]
#[tokio::test]
async fn market_events_enqueue_durable_snapshots_once_at_their_original_revisions() {
    let (_container, store, owner) = fixture().await;
    store
        .execute_at(10.into(), "discord:100", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    support::existing_destination(&store, 10, 20).await;

    let resolved = create(RESOLVED_MARKET);
    let cancelled = create(CANCELLED_MARKET);
    store
        .execute_at(10.into(), "discord:102", admin(), &resolved, 1000)
        .await
        .unwrap();
    store
        .execute_at(10.into(), "discord:103", admin(), &cancelled, 1000)
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:104",
            admin(),
            &Command::Resolve {
                id: RESOLVED_MARKET.into(),
                outcome: OutcomeIndex(0),
            },
            2000,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:105",
            admin(),
            &Command::Cancel {
                id: CANCELLED_MARKET.into(),
            },
            2000,
        )
        .await
        .unwrap();
    store
        .execute_at(10.into(), "discord:102", admin(), &resolved, 2000)
        .await
        .unwrap();

    let snapshots: Vec<(i64, serde_json::Value, i64)> = sqlx::query_as(
        "SELECT revision, snapshot, next_attempt_at FROM prediction_announcement_outbox WHERE guild_id='10' ORDER BY revision",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_that!(
        snapshots,
        eq(&vec![
            (
                4,
                json!({"Created": {
                    "id": RESOLVED_MARKET,
                    "question": "Will it rain?",
                    "creator": 7,
                    "options": ["Yes", "No"],
                    "closes_at": 2000,
                    "occurred_at": 1000,
                }}),
                1000,
            ),
            (
                5,
                json!({"Created": {
                    "id": CANCELLED_MARKET,
                    "question": "Will it rain?",
                    "creator": 7,
                    "options": ["Yes", "No"],
                    "closes_at": 2000,
                    "occurred_at": 1000,
                }}),
                1000,
            ),
            (
                6,
                json!({"Resolved": {
                    "id": RESOLVED_MARKET,
                    "question": "Will it rain?",
                    "winner": "Yes",
                    "refunded": true,
                    "odds": [{"label": "Yes", "tenths_percent": null}, {"label": "No", "tenths_percent": null}],
                    "occurred_at": 2000,
                }}),
                2000,
            ),
            (
                7,
                json!({"Cancelled": {
                    "id": CANCELLED_MARKET,
                    "question": "Will it rain?",
                    "odds": [{"label": "Yes", "tenths_percent": null}, {"label": "No", "tenths_percent": null}],
                    "occurred_at": 2000,
                }}),
                2000,
            ),
        ])
    );
}

#[googletest::test]
#[tokio::test]
async fn an_outbox_failure_rolls_back_the_market_and_receipt() {
    let (_container, store, owner) = fixture().await;
    store
        .execute_at(10.into(), "discord:200", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    support::existing_destination(&store, 10, 20).await;
    sqlx::query("ALTER TABLE prediction_announcement_outbox ADD CONSTRAINT reject_fixture_guild CHECK (guild_id <> '10')")
        .execute(&owner)
        .await
        .unwrap();

    let result = store
        .execute_at(
            10.into(),
            "discord:202",
            admin(),
            &create(FIXTURE_MARKET),
            1000,
        )
        .await;

    assert_that!(result, err(anything()));
    assert_that!(
        store.view(10.into()).await.unwrap().state.markets,
        not(contains((
            eq(&prediction_bot::types::MarketId::from(FIXTURE_MARKET)),
            anything()
        )))
    );
    let receipts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prediction_commands WHERE guild_id='10' AND command_key='discord:202'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_that!(receipts, eq(0));
}

#[googletest::test]
#[tokio::test]
async fn a_receipt_failure_rolls_back_the_enqueued_snapshot() {
    let (_container, store, owner) = fixture().await;
    store
        .execute_at(10.into(), "discord:200", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    support::existing_destination(&store, 10, 20).await;
    sqlx::query("ALTER TABLE prediction_commands ADD CONSTRAINT reject_fixture_receipt CHECK (command_key <> 'discord:202')")
        .execute(&owner)
        .await
        .unwrap();

    let result = store
        .execute_at(
            10.into(),
            "discord:202",
            admin(),
            &create(FIXTURE_MARKET),
            1000,
        )
        .await;

    assert_that!(result, err(anything()));
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prediction_announcement_outbox WHERE guild_id='10'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_that!(pending, eq(0));
}

#[googletest::test]
#[tokio::test]
async fn paused_enabled_settings_enqueue_without_backfilling_disabled_or_historical_events() {
    let (_container, store, owner) = fixture().await;
    store
        .execute_at(10.into(), "discord:300", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:301",
            admin(),
            &create(HISTORICAL_MARKET),
            1000,
        )
        .await
        .unwrap();
    support::existing_destination(&store, 10, 20).await;
    let historical: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prediction_announcement_outbox WHERE guild_id='10'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_that!(historical, eq(0));

    sqlx::query("UPDATE prediction_announcement_settings SET pause_reason='provider delay' WHERE guild_id='10'")
        .execute(&owner)
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:303",
            admin(),
            &create(PAUSED_MARKET),
            1000,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:304",
            admin(),
            &Command::Bet {
                id: PAUSED_MARKET.into(),
                outcome: OutcomeIndex(0),
                amount: Points(10),
            },
            1000,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "grant:7:87400",
            Actor {
                user_id: UserId(0),
                moderator: false,
                bot: false,
            },
            &Command::Grant { user_id: UserId(7) },
            87_400,
        )
        .await
        .unwrap();
    let restarted = prediction_bot::store::Store::new(
        store.pool.clone(),
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86_400,
        },
    );
    restarted.view(10.into()).await.unwrap();
    let pending_after_restart: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prediction_announcement_outbox WHERE guild_id='10' AND state='pending'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_that!(pending_after_restart, eq(2));

    store
        .configure_announcements(
            10.into(),
            "discord:305",
            admin(),
            ConfigurationChange::Disable,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:306",
            admin(),
            &create(DISABLED_MARKET),
            1000,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:disabled-join",
            member(),
            &Command::Join,
            1001,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:disabled-bet",
            member(),
            &Command::Bet {
                id: DISABLED_MARKET.into(),
                outcome: OutcomeIndex(0),
                amount: Points(10),
            },
            1001,
        )
        .await
        .unwrap();
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT revision, state FROM prediction_announcement_outbox WHERE guild_id='10' ORDER BY revision",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_that!(
        rows,
        eq(&vec![(5, "discarded".into()), (6, "discarded".into())])
    );
}

#[googletest::test]
#[tokio::test]
async fn concurrent_disable_and_market_creation_leave_no_pending_announcement() {
    let (_container, store, owner) = fixture().await;
    store
        .execute_at(10.into(), "discord:400", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    support::existing_destination(&store, 10, 20).await;

    // Queue both operations behind the real guild lock and observe each waiter.
    // Removing either production lock makes this synchronization fail.
    let mut gate = owner.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(10)")
        .execute(&mut *gate)
        .await
        .unwrap();
    let disabling = {
        let store = store.clone();
        tokio::spawn(async move {
            store
                .configure_announcements(
                    10.into(),
                    "discord:402",
                    admin(),
                    ConfigurationChange::Disable,
                )
                .await
        })
    };
    wait_for_guild_lock_waiters(&owner, 1).await;
    let creating = {
        let store = store.clone();
        tokio::spawn(async move {
            store
                .execute_at(
                    10.into(),
                    "discord:403",
                    admin(),
                    &create(CONCURRENT_MARKET),
                    1000,
                )
                .await
        })
    };
    wait_for_guild_lock_waiters(&owner, 2).await;
    gate.commit().await.unwrap();
    disabling.await.unwrap().unwrap();
    creating.await.unwrap().unwrap();

    let states: Vec<String> = sqlx::query_scalar(
        "SELECT state FROM prediction_announcement_outbox WHERE guild_id='10' ORDER BY revision",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_that!(states, is_empty());
}

#[googletest::test]
#[tokio::test]
async fn old_configuration_receipt_cannot_restore_a_disabled_channel() {
    let (_container, store, _owner) = fixture().await;
    let change = ConfigurationChange::Set {
        channel_id: ChannelId(20),
    };
    let receipt = store
        .configure_announcements(10.into(), "discord:101", admin(), change)
        .await
        .unwrap();
    store
        .configure_announcements(
            10.into(),
            "discord:102",
            admin(),
            ConfigurationChange::Disable,
        )
        .await
        .unwrap();
    assert_that!(
        store
            .configure_announcements(10.into(), "discord:101", admin(), change)
            .await
            .unwrap(),
        eq(&receipt)
    );
    let status = store.announcement_status(10.into(), admin()).await.unwrap();
    assert_that!(status.enabled, eq(false));
    assert_that!(status.pending, eq(0));
}

#[googletest::test]
#[tokio::test]
async fn announcement_status_defaults_to_disabled_for_an_unconfigured_guild() {
    let (_container, store, _owner) = fixture().await;

    let status = store.announcement_status(10.into(), admin()).await.unwrap();

    assert_that!(status.channel_id, eq(None));
    assert_that!(status.enabled, eq(false));
    assert_that!(status.version.0, eq(0));
    assert_that!(status.pause_reason, eq(&None));
    assert_that!(status.pending, eq(0));
}

#[googletest::test]
#[tokio::test]
async fn announcement_configuration_requires_a_human_administrator() {
    let (_container, store, _owner) = fixture().await;
    let bot_administrator = Actor {
        user_id: UserId(9),
        moderator: true,
        bot: true,
    };

    for actor in [member(), bot_administrator] {
        assert_that!(
            store
                .configure_announcements(
                    10.into(),
                    "discord:101",
                    actor,
                    ConfigurationChange::Set {
                        channel_id: ChannelId(20)
                    },
                )
                .await,
            err(matches_pattern!(StoreError::Configuration(anything())))
        );
        assert_that!(
            store.announcement_status(10.into(), actor).await,
            err(matches_pattern!(StoreError::Configuration(anything())))
        );
    }
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .enabled,
        eq(false)
    );
}

#[googletest::test]
#[tokio::test]
async fn repeated_configuration_commands_replay_their_receipts_once() {
    let (_container, store, _owner) = fixture().await;
    let enabled = store
        .configure_announcements(
            10.into(),
            "discord:101",
            admin(),
            ConfigurationChange::Set {
                channel_id: ChannelId(20),
            },
        )
        .await
        .unwrap();
    assert_that!(
        store
            .configure_announcements(
                10.into(),
                "discord:101",
                admin(),
                ConfigurationChange::Set {
                    channel_id: ChannelId(20)
                },
            )
            .await
            .unwrap(),
        eq(&enabled)
    );
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .version
            .0,
        eq(1)
    );

    let disabled = store
        .configure_announcements(
            10.into(),
            "discord:102",
            admin(),
            ConfigurationChange::Disable,
        )
        .await
        .unwrap();
    assert_that!(
        store
            .configure_announcements(
                10.into(),
                "discord:102",
                admin(),
                ConfigurationChange::Disable,
            )
            .await
            .unwrap(),
        eq(&disabled)
    );
    let status = store.announcement_status(10.into(), admin()).await.unwrap();
    assert_that!(status.enabled, eq(false));
    assert_that!(status.version.0, eq(2));
}

use prediction_bot::announcements::deliver_due;
use std::sync::Arc;
use support::{clock, delivered, discord_http, queued, restart};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[googletest::test]
#[tokio::test]
async fn delivery_posts_saved_content_without_pings_and_records_the_message() {
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/20/messages"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(1000))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_that!(requests.len(), eq(1));
    let payload: serde_json::Value = requests[0].body_json().unwrap();
    assert_that!(
        payload["content"].as_str().unwrap(),
        contains_substring("Market created")
    );
    assert_that!(
        payload["content"].as_str().unwrap(),
        contains_substring("Will it rain?")
    );
    assert_that!(
        payload["content"].as_str().unwrap(),
        contains_substring("Creator: <@7>")
    );
    assert_that!(payload["allowed_mentions"]["parse"], eq(&json!([])));
    assert_that!(payload["allowed_mentions"]["replied_user"], eq(false));
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(0)
    );
    let receipt: (String, String, String) = sqlx::query_as("SELECT state,delivered_channel_id,delivered_message_id FROM prediction_announcement_outbox").fetch_one(&owner).await.unwrap();
    assert_that!(receipt, eq(&("delivered".into(), "20".into(), "99".into())));
}

#[googletest::test]
#[tokio::test]
async fn delivery_retries_after_restart_at_the_persisted_deadline() {
    assert_delivery_recovers_after_restart(500).await;
}

#[googletest::test]
#[tokio::test]
async fn delivery_recovers_after_credentials_are_repaired_without_reconfiguration() {
    assert_delivery_recovers_after_restart(401).await;
}

#[googletest::test]
#[tokio::test]
async fn delivery_recovers_after_http_request_timeout_without_reconfiguration() {
    assert_delivery_recovers_after_restart(408).await;
}

async fn assert_delivery_recovers_after_restart(status: u16) {
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(status).set_body_json(json!({"code":0,"message":"temporary"})),
        )
        .mount(&server)
        .await;
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(1000))
        .await
        .unwrap();
    let retry: (i64, i64) =
        sqlx::query_as("SELECT attempts,next_attempt_at FROM prediction_announcement_outbox")
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_that!(retry, eq((1, 1005)));
    let settings = store.announcement_status(10.into(), admin()).await.unwrap();
    assert_that!(settings.pause_reason, eq(&None));
    assert_that!(settings.version.0, eq(1));
    assert_that!(settings.channel_id, eq(Some(ChannelId(20))));
    let store = restart(&store);
    server.reset().await;
    // Model operator credential repair by rebuilding the real HTTP client with a new token.
    let http = Arc::new(
        serenity::http::HttpBuilder::new("recovered-token")
            .application_id(42.into())
            .proxy(server.uri())
            .ratelimiter_disabled(true)
            .build(),
    );
    Mock::given(method("POST"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bot recovered-token",
        ))
        .respond_with(delivered())
        .mount(&server)
        .await;
    deliver_due(store.clone(), http.clone(), clock(1004))
        .await
        .unwrap();
    assert_that!(server.received_requests().await.unwrap(), is_empty());
    deliver_due(store.clone(), http, clock(1005)).await.unwrap();
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(0)
    );
}

#[googletest::test]
#[tokio::test]
async fn delivery_preserves_guild_revision_order_while_other_guilds_progress() {
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    queued(&store, 11, 21).await;
    store
        .execute_at(
            10.into(),
            "discord:resolve",
            admin(),
            &Command::Resolve {
                id: FIXTURE_MARKET.into(),
                outcome: OutcomeIndex(0),
            },
            2000,
        )
        .await
        .unwrap();
    let server = MockServer::start().await;
    Mock::given(path("/api/v10/channels/20/messages"))
        .respond_with(
            ResponseTemplate::new(500).set_body_json(json!({"code":0,"message":"temporary"})),
        )
        .mount(&server)
        .await;
    Mock::given(path("/api/v10/channels/21/messages"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(2000))
        .await
        .unwrap();
    assert_that!(
        store
            .announcement_status(11.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(0)
    );
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(2)
    );
    let failed: Vec<i64> = sqlx::query_scalar(
        "SELECT attempts FROM prediction_announcement_outbox WHERE guild_id='10' ORDER BY revision",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_that!(failed, eq(&[1, 0]));
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(2004))
        .await
        .unwrap();
    assert_that!(server.received_requests().await.unwrap(), is_empty());
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(2005))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_that!(requests.len(), eq(2));
    assert_that!(
        requests[0].body_json::<serde_json::Value>().unwrap()["content"]
            .as_str()
            .unwrap(),
        contains_substring("Market created")
    );
    assert_that!(
        requests[1].body_json::<serde_json::Value>().unwrap()["content"]
            .as_str()
            .unwrap(),
        contains_substring("Market resolved")
    );
}

#[googletest::test]
#[tokio::test]
async fn delivery_recovers_the_acknowledgement_gap_after_restart() {
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    sqlx::query("ALTER TABLE prediction_announcement_outbox ADD CONSTRAINT reject_delivery CHECK (state <> 'delivered')").execute(&owner).await.unwrap();
    assert_that!(
        deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(1000)).await,
        err(anything())
    );
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(1)
    );
    sqlx::query("ALTER TABLE prediction_announcement_outbox DROP CONSTRAINT reject_delivery")
        .execute(&owner)
        .await
        .unwrap();
    let restarted = restart(&store);
    deliver_due(
        restarted.clone(),
        Arc::new(discord_http(&server)),
        clock(1000),
    )
    .await
    .unwrap();
    assert_that!(server.received_requests().await.unwrap().len(), eq(2));
    assert_that!(
        restarted
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(0)
    );
}

#[googletest::test]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delivery_in_flight_configuration_changes_condition_completion() {
    use std::sync::atomic::{AtomicI64, Ordering};
    use support::ResponseBarrier;
    for (disable, success) in [(false, false), (false, true), (true, false), (true, true)] {
        let (_container, store, owner) = fixture().await;
        queued(&store, 10, 20).await;
        let server = MockServer::start().await;
        let response = if success {
            delivered()
        } else {
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50013,"message":"missing permission"}))
        };
        let barrier = ResponseBarrier::mount(&server, response).await;
        let now = Arc::new(AtomicI64::new(1000));
        let clock: prediction_bot::announcements::Clock = {
            let now = now.clone();
            Arc::new(move || now.load(Ordering::SeqCst))
        };
        let delivery = tokio::spawn(deliver_due(
            store.clone(),
            Arc::new(discord_http(&server)),
            clock.clone(),
        ));
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            barrier.arrived.notified(),
        )
        .await
        .expect("send did not reach endpoint");
        let change = if disable {
            ConfigurationChange::Disable
        } else {
            ConfigurationChange::Set {
                channel_id: ChannelId(21),
            }
        };
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            store.configure_announcements(10.into(), "discord:change", admin(), change),
        )
        .await
        .expect("HTTP must not hold the guild lock")
        .unwrap();
        barrier.release();
        delivery.await.unwrap().unwrap();
        let status = store.announcement_status(10.into(), admin()).await.unwrap();
        assert_that!(status.pause_reason, eq(&None));
        let row: (String, Option<String>) =
            sqlx::query_as("SELECT state,delivered_channel_id FROM prediction_announcement_outbox WHERE snapshot ? 'Created'")
                .fetch_one(&owner)
                .await
                .unwrap();
        if disable {
            assert_that!(row, eq(&("discarded".into(), None)));
        } else if success {
            assert_that!(row, eq(&("delivered".into(), Some("20".into()))));
        } else {
            assert_that!(row, eq(&("pending".into(), None)));
        }
        server.reset().await;
        Mock::given(path("/api/v10/channels/21/messages"))
            .respond_with(delivered())
            .mount(&server)
            .await;
        // Set resets the deadline using wall time; drive the worker from the saved deadline.
        let deadline: i64 =
            sqlx::query_scalar("SELECT MAX(next_attempt_at) FROM prediction_announcement_outbox")
                .fetch_one(&owner)
                .await
                .unwrap();
        now.store(deadline, Ordering::SeqCst);
        deliver_due(store.clone(), Arc::new(discord_http(&server)), clock)
            .await
            .unwrap();
        assert_that!(
            server.received_requests().await.unwrap().len(),
            eq(usize::from(!disable) + usize::from(!disable && !success))
        );
    }
}

async fn wait_for_guild_lock_waiters(owner: &sqlx::PgPool, expected: i64) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let waiting: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pg_locks WHERE locktype='advisory' AND objid=10 AND NOT granted").fetch_one(owner).await.unwrap();
            if waiting == expected { break; }
            tokio::task::yield_now().await;
        }
    }).await.expect("guild operations did not serialize on the transaction lock");
}

#[googletest::test]
#[tokio::test]
async fn enabled_announcements_enqueue_a_member_join_only_once() {
    let (_container, store, owner) = fixture().await;
    support::existing_destination(&store, 10, 20).await;
    store
        .execute_at(10.into(), "discord:join", member(), &Command::Join, 1000)
        .await
        .unwrap();
    assert_that!(
        store.view(10.into()).await.unwrap().state.accounts,
        contains((eq(&UserId(8)), anything()))
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM prediction_announcement_outbox")
        .fetch_one(&owner)
        .await
        .unwrap();
    assert_that!(count, eq(1));
    for key in ["discord:join", "discord:join-again"] {
        store
            .execute_at(10.into(), key, member(), &Command::Join, 1001)
            .await
            .unwrap();
    }
    let snapshots: Vec<serde_json::Value> =
        sqlx::query_scalar("SELECT snapshot FROM prediction_announcement_outbox")
            .fetch_all(&owner)
            .await
            .unwrap();
    assert_that!(
        snapshots,
        eq(&vec![json!({"MemberEnrolled": {
            "user_id": 8, "occurred_at": 1000
        }})])
    );
}

#[googletest::test]
#[tokio::test]
async fn delivery_waiting_for_one_guild_completion_does_not_block_another_guild() {
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    queued(&store, 11, 21).await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    let mut gate = owner.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(10)")
        .execute(&mut *gate)
        .await
        .unwrap();
    let delivery = tokio::spawn(deliver_due(
        store.clone(),
        Arc::new(discord_http(&server)),
        clock(1000),
    ));
    wait_for_guild_lock_waiters(&owner, 1).await;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while store
            .announcement_status(11.into(), admin())
            .await
            .unwrap()
            .pending
            != 0
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("another guild must complete while guild 10 waits");
    gate.commit().await.unwrap();
    delivery.await.unwrap().unwrap();
    assert_that!(server.received_requests().await.unwrap().len(), eq(2));
}

#[googletest::test]
#[tokio::test]
async fn delivery_permission_failure_pauses_until_configuration_changes() {
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50013,"message":"sentinel-provider-secret"})),
        )
        .mount(&server)
        .await;
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(1000))
        .await
        .unwrap();
    let status = store.announcement_status(10.into(), admin()).await.unwrap();
    assert_that!(status.pending, eq(1));
    let reason = status.pause_reason.unwrap();
    assert_that!(reason, contains_substring("permission"));
    assert_that!(reason, not(contains_substring("sentinel")));
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(2000))
        .await
        .unwrap();
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    store
        .configure_announcements(
            10.into(),
            "discord:reset",
            admin(),
            ConfigurationChange::Set {
                channel_id: ChannelId(21),
            },
        )
        .await
        .unwrap();
    server.reset().await;
    Mock::given(path("/api/v10/channels/21/messages"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    let deadline: i64 =
        sqlx::query_scalar("SELECT next_attempt_at FROM prediction_announcement_outbox")
            .fetch_one(&owner)
            .await
            .unwrap();
    deliver_due(
        store.clone(),
        Arc::new(discord_http(&server)),
        clock(deadline),
    )
    .await
    .unwrap();
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(0)
    );
    assert_that!(server.received_requests().await.unwrap().len(), eq(2));
}

#[googletest::test]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delivery_retry_deadline_uses_completion_time_and_saturates_attempts() {
    use std::sync::atomic::{AtomicI64, Ordering};
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    sqlx::query("UPDATE prediction_announcement_outbox SET attempts=$1")
        .bind(i64::MAX)
        .execute(&owner)
        .await
        .unwrap();
    let server = MockServer::start().await;
    let barrier = support::ResponseBarrier::mount(
        &server,
        ResponseTemplate::new(500).set_body_json(json!({"code":0,"message":"temporary"})),
    )
    .await;
    let now = Arc::new(AtomicI64::new(1000));
    let clock: prediction_bot::announcements::Clock = {
        let now = now.clone();
        Arc::new(move || now.load(Ordering::SeqCst))
    };
    let delivery = tokio::spawn(deliver_due(store, Arc::new(discord_http(&server)), clock));
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        barrier.arrived.notified(),
    )
    .await
    .unwrap();
    now.store(1007, Ordering::SeqCst);
    barrier.release();
    delivery.await.unwrap().unwrap();
    let retry: (i64, i64) =
        sqlx::query_as("SELECT attempts,next_attempt_at FROM prediction_announcement_outbox")
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_that!(retry, eq((i64::MAX, 1307)));
}

#[googletest::test]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_shutdown_acknowledges_in_flight_send_without_discovering_backlog() {
    use prediction_bot::announcements::start_announcement_worker;
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    store
        .execute_at(
            10.into(),
            "discord:later",
            admin(),
            &create(CONCURRENT_MARKET),
            1000,
        )
        .await
        .unwrap();
    let server = MockServer::start().await;
    let barrier = support::ResponseBarrier::mount(&server, delivered()).await;
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let worker = start_announcement_worker(
        store.clone(),
        Arc::new(discord_http(&server)),
        clock(1000),
        receiver,
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        barrier.arrived.notified(),
    )
    .await
    .unwrap();
    shutdown.send(true).unwrap();
    barrier.release();
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .expect("worker must stop despite due backlog")
        .unwrap();
    let states: Vec<String> =
        sqlx::query_scalar("SELECT state FROM prediction_announcement_outbox ORDER BY revision")
            .fetch_all(&owner)
            .await
            .unwrap();
    assert_that!(states, eq(&vec!["delivered", "pending"]));
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));

    server.reset().await;
    let barrier = support::ResponseBarrier::mount(&server, delivered()).await;
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let worker = start_announcement_worker(
        restart(&store),
        Arc::new(discord_http(&server)),
        clock(1000),
        receiver,
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        barrier.arrived.notified(),
    )
    .await
    .unwrap();
    shutdown.send(true).unwrap();
    barrier.release();
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap();
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(0)
    );
}

#[googletest::test]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_restart_retries_failed_send_from_its_persisted_deadline() {
    use prediction_bot::announcements::start_announcement_worker;
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    let server = MockServer::start().await;
    let barrier = support::ResponseBarrier::mount(
        &server,
        ResponseTemplate::new(503)
            .set_body_json(json!({"code":0,"message":"sentinel private provider body"})),
    )
    .await;
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let worker = start_announcement_worker(
        store.clone(),
        Arc::new(discord_http(&server)),
        clock(1000),
        receiver,
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        barrier.arrived.notified(),
    )
    .await
    .unwrap();
    shutdown.send(true).unwrap();
    barrier.release();
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap();
    let retry: (i64, i64, String) =
        sqlx::query_as("SELECT attempts,next_attempt_at,state FROM prediction_announcement_outbox")
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_that!(retry, eq(&(1, 1005, "pending".into())));
    server.reset().await;
    let barrier = support::ResponseBarrier::mount(&server, delivered()).await;
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let worker = start_announcement_worker(
        restart(&store),
        Arc::new(discord_http(&server)),
        clock(1005),
        receiver,
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        barrier.arrived.notified(),
    )
    .await
    .unwrap();
    drop(shutdown);
    barrier.release();
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap();
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(0)
    );
}

#[googletest::test]
#[tokio::test]
async fn worker_does_not_discover_when_shutdown_is_already_set_or_closed() {
    use prediction_bot::announcements::start_announcement_worker;
    let (_container, store, _owner) = fixture().await;
    queued(&store, 10, 20).await;
    let server = MockServer::start().await;
    for closed in [false, true] {
        let (shutdown, receiver) = tokio::sync::watch::channel(!closed);
        if closed {
            drop(shutdown);
        }
        let worker = start_announcement_worker(
            store.clone(),
            Arc::new(discord_http(&server)),
            clock(1000),
            receiver,
        );
        tokio::time::timeout(std::time::Duration::from_secs(2), worker)
            .await
            .unwrap()
            .unwrap();
    }
    assert_that!(server.received_requests().await.unwrap(), is_empty());
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(1)
    );
}

#[derive(Default)]
struct WorkerAudit {
    events: std::sync::Mutex<Vec<prediction_bot::audit::AuditEvent>>,
    changed: tokio::sync::Notify,
}
impl prediction_bot::audit::AuditListener for WorkerAudit {
    fn on_event(&self, event: &prediction_bot::audit::AuditEvent) {
        self.events.lock().unwrap().push(event.clone());
        self.changed.notify_one();
    }
}
impl WorkerAudit {
    async fn wait_for(&self, predicate: impl Fn(&prediction_bot::audit::AuditEvent) -> bool) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let changed = self.changed.notified();
                if self.events.lock().unwrap().iter().any(&predicate) {
                    return;
                }
                changed.await;
            }
        })
        .await
        .expect("expected worker audit event");
    }
}

#[googletest::test]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_reports_discovery_failure_then_recovers_on_the_next_poll() {
    use prediction_bot::{
        announcements::start_announcement_worker,
        audit::{AuditEvent, Failure, FailureCategory, Outcome, Stage},
        store::Store,
    };
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    let audit = Arc::new(WorkerAudit::default());
    let store = Arc::new(Store::new_with_audit(
        store.pool.clone(),
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86400,
        },
        audit.clone(),
    ));
    sqlx::query("REVOKE SELECT ON prediction_announcement_outbox FROM prediction_bot_runtime")
        .execute(&owner)
        .await
        .unwrap();
    let server = MockServer::start().await;
    let barrier = support::ResponseBarrier::mount(&server, delivered()).await;
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let worker = start_announcement_worker(
        store.clone(),
        Arc::new(discord_http(&server)),
        clock(1000),
        receiver,
    );
    audit.wait_for(|event| matches!(event, AuditEvent::AnnouncementWorkerFailed { stage: Stage::Discover, outcome: Outcome::Failed(Failure { category: FailureCategory::Database, sqlstate: Some(code), .. }) } if code == "42501")).await;
    sqlx::query("GRANT SELECT ON prediction_announcement_outbox TO prediction_bot_runtime")
        .execute(&owner)
        .await
        .unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        barrier.arrived.notified(),
    )
    .await
    .unwrap();
    shutdown.send(true).unwrap();
    barrier.release();
    tokio::time::timeout(std::time::Duration::from_secs(5), worker)
        .await
        .unwrap()
        .unwrap();
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(0)
    );
    assert_that!(
        audit.events.lock().unwrap().as_slice(),
        contains(matches_pattern!(AuditEvent::AnnouncementAttemptCompleted {
            guild: eq(&GuildId(10)),
            revision: eq(&EventRevision(4)),
            channel_id: eq(&ChannelId(20)),
            stage: eq(&Stage::Deliver),
            outcome: eq(&Outcome::Succeeded),
            ..
        }))
    );
}

#[googletest::test]
#[tokio::test]
async fn worker_reports_safe_attempt_failure_and_acknowledgement_persistence_failure() {
    use prediction_bot::{
        announcements::start_announcement_worker,
        audit::{AnnouncementDecision, AuditEvent, FailureCategory, Outcome, Stage},
        store::Store,
    };
    for rejected in [false, true] {
        let (_container, store, owner) = fixture().await;
        queued(&store, 10, 20).await;
        let audit = Arc::new(WorkerAudit::default());
        let store = Arc::new(Store::new_with_audit(
            store.pool.clone(),
            42.into(),
            Policy {
                amount: Points(100),
                interval: 86400,
            },
            audit.clone(),
        ));
        if !rejected {
            sqlx::query("ALTER TABLE prediction_announcement_outbox ADD CONSTRAINT reject_ack CHECK (state <> 'delivered')").execute(&owner).await.unwrap();
        }
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/20/messages"))
            .respond_with(if rejected {
                ResponseTemplate::new(403).set_body_json(
                    json!({"code":50013,"message":"sentinel private body and token"}),
                )
            } else {
                delivered()
            })
            .mount(&server)
            .await;
        let (shutdown, receiver) = tokio::sync::watch::channel(false);
        let worker = start_announcement_worker(
            store.clone(),
            Arc::new(discord_http(&server)),
            clock(1000),
            receiver,
        );
        audit
            .wait_for(|event| matches!(event, AuditEvent::AnnouncementAttemptCompleted { .. }))
            .await;
        shutdown.send(true).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), worker)
            .await
            .unwrap()
            .unwrap();
        {
            let events = audit.events.lock().unwrap();
            assert_that!(
                events.iter().any(|event| match event {
                    AuditEvent::AnnouncementAttemptCompleted {
                        guild: GuildId(10),
                        revision: EventRevision(4),
                        channel_id: ChannelId(20),
                        decision,
                        outcome: Outcome::Failed(failure),
                        stage,
                        ..
                    } => {
                        if rejected {
                            *decision == AnnouncementDecision::Pause
                                && *stage == Stage::Deliver
                                && failure.http_status == Some(403)
                                && failure.discord_code == Some(50013)
                        } else {
                            *decision == AnnouncementDecision::Delivered
                                && *stage == Stage::Commit
                                && failure.category == FailureCategory::Constraint
                                && failure.sqlstate.as_deref() == Some("23514")
                        }
                    }
                    _ => false,
                }),
                eq(true)
            );
            assert_that!(format!("{events:?}"), not(contains_substring("sentinel")));
        }
        assert_that!(
            store
                .announcement_status(10.into(), admin())
                .await
                .unwrap()
                .pending,
            eq(1)
        );
    }
}

#[googletest::test]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn application_startup_delivers_announcements_under_the_gateway_guard() {
    let (_container, store, _owner) = fixture().await;
    store
        .execute_at(10.into(), "discord:join", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:create",
            admin(),
            &create(FIXTURE_MARKET),
            1000,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:startup-bet-1",
            admin(),
            &Command::Bet {
                id: FIXTURE_MARKET.into(),
                outcome: OutcomeIndex(0),
                amount: Points(10),
            },
            1001,
        )
        .await
        .unwrap();
    // Queue only the movement announcement so startup coverage does not depend on polling delays.
    support::existing_destination(&store, 10, 20).await;
    store
        .execute_at(
            10.into(),
            "discord:startup-bet-2",
            admin(),
            &Command::Bet {
                id: FIXTURE_MARKET.into(),
                outcome: OutcomeIndex(1),
                amount: Points(10),
            },
            1001,
        )
        .await
        .unwrap();
    let server = MockServer::start().await;
    let mut user = serenity::all::CurrentUser::default();
    user.id = 42.into();
    Mock::given(method("GET"))
        .and(path("/api/v10/users/@me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(user))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/gateway/bot"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50001,"message":"private gateway failure"}))
                .set_delay(std::time::Duration::from_secs(2)),
        )
        .mount(&server)
        .await;
    let barrier = support::ResponseBarrier::mount(&server, delivered()).await;
    let running = tokio::spawn(prediction_bot::discord::run_with_http(
        store.clone(),
        discord_http(&server),
    ));
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        barrier.arrived.notified(),
    )
    .await
    .unwrap();
    assert_that!(
        store.gateway_guard().await,
        err(matches_pattern!(StoreError::Configuration(anything())))
    );
    barrier.release();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), running)
        .await
        .unwrap()
        .unwrap();
    assert_that!(
        result,
        err(matches_pattern!(
            prediction_bot::discord::DiscordError::Gateway
        ))
    );
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(0)
    );
    let requests = server.received_requests().await.unwrap();
    let movement = requests
        .iter()
        .filter_map(|request| request.body_json::<serde_json::Value>().ok())
        .find(|body| {
            body["content"]
                .as_str()
                .is_some_and(|text| text.contains("2 bets placed"))
        })
        .expect("startup must deliver the bet movement announcement");
    let text = movement["content"].as_str().unwrap();
    assert_that!(text, contains_substring("Yes — 50.0% implied chance 🔴 ⬇️"));
    assert_that!(text, contains_substring("No — 50.0% implied chance 🟢 ⬆️"));
    store.gateway_guard().await.unwrap().close().await.unwrap();
}

#[googletest::test]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn worker_shutdown_aborts_unacknowledged_send_after_the_grace_budget() {
    use prediction_bot::{
        announcements::start_announcement_worker,
        audit::{AuditEvent, Failure, FailureCategory, Stage},
        store::Store,
    };
    let (_container, store, _owner) = fixture().await;
    queued(&store, 10, 20).await;
    let audit = Arc::new(WorkerAudit::default());
    let store = Arc::new(Store::new_with_audit(
        store.pool.clone(),
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86400,
        },
        audit.clone(),
    ));
    let server = MockServer::start().await;
    let barrier = support::ResponseBarrier::mount(&server, delivered()).await;
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let worker = start_announcement_worker(
        store.clone(),
        Arc::new(discord_http(&server)),
        clock(1000),
        receiver,
    );
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        barrier.arrived.notified(),
    )
    .await
    .unwrap();
    shutdown.send(true).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(17), worker)
        .await
        .expect("worker must abort stalled request within grace")
        .unwrap();
    barrier.release();
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(1)
    );
    assert_that!(
        audit.events.lock().unwrap().as_slice(),
        contains(matches_pattern!(AuditEvent::Lifecycle {
            stage: eq(&Stage::AnnouncementWorkerShutdown),
            outcome: matches_pattern!(prediction_bot::audit::Outcome::Failed(matches_pattern!(
                Failure {
                    category: eq(&FailureCategory::Timeout),
                    ..
                }
            ))),
            ..
        }))
    );
}

#[googletest::test]
#[tokio::test]
async fn real_interaction_adapters_configure_and_enqueue_each_creation_once() {
    use prediction_bot::discord::handle_interaction;
    use serenity::all::Interaction;
    use support::{interaction_json, mount_replayed_destination_validation};

    let (_container, store, owner) = fixture().await;
    store
        .execute(10.into(), "discord:900", admin(), &Command::Join)
        .await
        .unwrap();
    let server = MockServer::start().await;
    let http = discord_http(&server);
    mount_replayed_destination_validation(&server).await;
    let callback_attempts = std::sync::atomic::AtomicUsize::new(0);
    Mock::given(method("POST"))
        .and(wiremock::matchers::path_regex(
            "^/api/v10/interactions/20[123]/test-interaction-token/callback$",
        ))
        .respond_with(move |_: &wiremock::Request| {
            if callback_attempts
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                .is_multiple_of(2)
            {
                ResponseTemplate::new(204)
            } else {
                // Discord redelivery can report that the original acknowledgement already exists.
                ResponseTemplate::new(400).set_body_json(
                    json!({"code": 40060, "message": "Interaction has already been acknowledged."}),
                )
            }
        })
        .expect(6)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(
            "/api/v10/webhooks/42/test-interaction-token/messages/@original",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serenity::all::Message::default()))
        .expect(6)
        .mount(&server)
        .await;

    let configure = Interaction::Command(serde_json::from_value(interaction_json(201, &json!({
        "id": "1", "name": "market", "type": 1,
        "options": [{"name": "announcements", "type": 2, "options": [{
            "name": "set", "type": 1, "options": [{"name": "channel", "type": 7, "value": "55"}]
        }]}]
    }))).unwrap());
    let slash = Interaction::Command(
        serde_json::from_value(interaction_json(
            202,
            &json!({
                "id": "1", "name": "market", "type": 1,
                "options": [{"name": "create", "type": 1, "options": [
                    {"name": "question", "type": 3, "value": "Slash-created market?"},
                    {"name": "options", "type": 3, "value": "Yes | No"},
                    {"name": "closes_at", "type": 3, "value": "2099-01-01T00:00:00Z"}
                ]}]
            }),
        ))
        .unwrap(),
    );
    let modal = Interaction::Modal(serde_json::from_value(interaction_json(203, &json!({
        "custom_id": "pm:10:7:new:yesno",
        "components": [
            {"type": 1, "components": [{"type": 4, "custom_id": "question", "style": 1, "label": "Question", "value": "Modal-created market?"}]},
            {"type": 1, "components": [{"type": 4, "custom_id": "closes_at", "style": 1, "label": "Closing time", "value": "2099-01-01T00:00:00Z"}]}
        ]
    }))).unwrap());

    for interaction in [configure, slash, modal] {
        for _ in 0..2 {
            handle_interaction(store.clone(), &http, 99.into(), interaction.clone()).await;
        }
    }

    let status = store.announcement_status(10.into(), admin()).await.unwrap();
    assert_that!(status.enabled, eq(true));
    assert_that!(status.channel_id, eq(Some(ChannelId(55))));
    assert_that!(
        status.version.0,
        eq(1),
        "replayed configuration must keep its original version"
    );
    assert_that!(status.pending, eq(3));
    let enqueued: Vec<(String, i64)> = sqlx::query_as(
        "SELECT snapshot->'Created'->>'question',count(*) FROM prediction_announcement_outbox WHERE guild_id='10' AND snapshot ? 'Created' GROUP BY 1 ORDER BY 1",
    ).fetch_all(&owner).await.unwrap();
    assert_that!(
        enqueued,
        eq(&vec![
            ("Modal-created market?".into(), 1),
            ("Slash-created market?".into(), 1)
        ])
    );

    let requests = server.received_requests().await.unwrap();
    let acknowledgements: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path().ends_with("/callback"))
        .collect();
    assert_that!(acknowledgements.len(), eq(6));
    for acknowledgement in acknowledgements {
        let body = acknowledgement.body_json::<serde_json::Value>().unwrap();
        assert_that!(body["type"], eq(5));
        assert_that!(
            body["data"]["flags"],
            eq(64),
            "all successful and replayed interactions stay private"
        );
    }
    let replies: Vec<serde_json::Value> = requests
        .iter()
        .filter(|request| request.method.as_str() == "PATCH")
        .map(|request| request.body_json().unwrap())
        .collect();
    assert_that!(replies.len(), eq(6));
    for pair in replies.chunks_exact(2) {
        assert_that!(
            pair[0]["content"],
            eq(&pair[1]["content"]),
            "redelivery must recover the same receipt"
        );
        for reply in pair {
            assert_that!(reply["allowed_mentions"]["parse"], eq(&json!([])));
        }
    }
    assert_that!(
        replies[0]["content"].as_str().unwrap(),
        contains_substring("Announcements enabled for <#55>")
    );
    for index in [2, 4] {
        assert_that!(
            replies[index]["content"].as_str().unwrap(),
            starts_with("Market created: ")
        );
    }
    assert_that!(
        requests
            .iter()
            .any(|request| request.method.as_str() == "POST"
                && request.url.path().ends_with("/messages")),
        eq(false)
    );
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/55/messages"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    deliver_due(store, Arc::new(http), clock(4_070_908_800))
        .await
        .unwrap();
    let messages: Vec<serde_json::Value> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == "/api/v10/channels/55/messages")
        .map(|request| request.body_json().unwrap())
        .collect();
    assert_that!(messages.len(), eq(3));
    assert_that!(
        messages[0]["content"],
        eq(
            "Prediction market announcements are enabled! New markets, bets, and results will appear here."
        )
    );
    assert_that!(messages[0]["allowed_mentions"]["parse"], eq(&json!([])));
}

#[googletest::test]
#[tokio::test]
async fn set_redelivery_recovers_original_receipt_without_destination_reads() {
    use prediction_bot::discord::handle_interaction;
    use serenity::all::Interaction;
    let (_container, store, _owner) = fixture().await;
    store
        .configure_announcements(
            10.into(),
            "discord:201",
            admin(),
            ConfigurationChange::Set {
                channel_id: ChannelId(55),
            },
        )
        .await
        .unwrap();
    store
        .configure_announcements(
            10.into(),
            "discord:202",
            admin(),
            ConfigurationChange::Set {
                channel_id: ChannelId(56),
            },
        )
        .await
        .unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v10/interactions/201/test-interaction-token/callback",
        ))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(json!({"code":40060,"message":"Already acknowledged"})),
        )
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(
            "/api/v10/webhooks/42/test-interaction-token/messages/@original",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serenity::all::Message::default()))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"code":10003,"message":"Unknown channel"})),
        )
        .expect(0)
        .mount(&server)
        .await;
    let mut input = support::interaction_json(
        201,
        &json!({
            "id":"1", "name":"market", "type":1,
            "options":[{"name":"announcements", "type":2, "options":[{
                "name":"set", "type":1, "options":[{"name":"channel", "type":7, "value":"55"}]
            }]}]
        }),
    );
    handle_interaction(
        store.clone(),
        &discord_http(&server),
        99.into(),
        Interaction::Command(serde_json::from_value(input.clone()).unwrap()),
    )
    .await;
    // A receipt must not be exposed to a different actor using the same key.
    input["member"]["user"]["id"] = json!("8");
    input["user"]["id"] = json!("8");
    handle_interaction(
        store.clone(),
        &discord_http(&server),
        99.into(),
        Interaction::Command(serde_json::from_value(input).unwrap()),
    )
    .await;
    let requests = server.received_requests().await.unwrap();
    let replies: Vec<serde_json::Value> = requests
        .iter()
        .filter(|request| request.method.as_str() == "PATCH")
        .map(|request| request.body_json().unwrap())
        .collect();
    assert_that!(
        replies[0]["content"].as_str().unwrap(),
        starts_with("Announcements enabled for <#55>.")
    );
    assert_that!(
        replies[1]["content"].as_str().unwrap(),
        not(contains_substring("Announcements enabled"))
    );
    for reply in replies {
        assert_that!(reply["allowed_mentions"]["parse"], eq(&json!([])));
    }
    let status = store.announcement_status(10.into(), admin()).await.unwrap();
    assert_that!(status.channel_id, eq(Some(ChannelId(56))));
    assert_that!(status.version.0, eq(2));
}

#[googletest::test]
#[tokio::test]
async fn bet_delivery_preserves_event_percentages_through_later_bets_and_retries() {
    let (_container, store, _owner) = fixture().await;
    store
        .execute_at(10.into(), "discord:join", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:create",
            admin(),
            &create(FIXTURE_MARKET),
            1000,
        )
        .await
        .unwrap();
    support::existing_destination(&store, 10, 20).await;
    let bet = Command::Bet {
        id: FIXTURE_MARKET.into(),
        outcome: OutcomeIndex(0),
        amount: Points(13),
    };
    for (key, time) in [("discord:bet-1", 1001), ("discord:bet-1", 1002)] {
        store
            .execute_at(10.into(), key, admin(), &bet, time)
            .await
            .unwrap();
    }
    store
        .execute_at(
            10.into(),
            "discord:bet-2",
            admin(),
            &Command::Bet {
                id: FIXTURE_MARKET.into(),
                outcome: OutcomeIndex(1),
                amount: Points(39),
            },
            1001,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:bet-3",
            admin(),
            &Command::Bet {
                id: FIXTURE_MARKET.into(),
                outcome: OutcomeIndex(0),
                amount: Points(26),
            },
            1002,
        )
        .await
        .unwrap();
    let rejected = Command::Bet {
        id: FIXTURE_MARKET.into(),
        outcome: OutcomeIndex(0),
        amount: Points(1000),
    };
    assert_that!(
        store
            .execute_at(10.into(), "discord:rejected", admin(), &rejected, 1002)
            .await,
        err(anything())
    );
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/20/messages"))
        .and(wiremock::matchers::body_string_contains("1 bet placed"))
        .respond_with(delivered())
        .with_priority(1)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/20/messages"))
        .respond_with(ResponseTemplate::new(500))
        .with_priority(2)
        .expect(1)
        .mount(&server)
        .await;
    deliver_due(
        restart(&store),
        Arc::new(discord_http(&server)),
        clock(3000),
    )
    .await
    .unwrap();
    let failed_requests = server.received_requests().await.unwrap();
    let failed_body: serde_json::Value = failed_requests[0].body_json().unwrap();
    assert_that!(
        failed_body["content"].as_str().unwrap(),
        contains_substring("Yes — 100.0% implied chance")
    );
    assert_that!(failed_requests.len(), eq(2));
    let first = failed_body["content"].as_str().unwrap();
    assert_that!(first, not(contains_substring("⬆️")));
    assert_that!(first, not(contains_substring("⬇️")));
    let failed_movement: serde_json::Value = failed_requests[1].body_json().unwrap();
    assert_that!(
        failed_movement["content"].as_str().unwrap(),
        contains_substring("Yes — 25.0% implied chance 🔴 ⬇️")
    );
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/20/messages"))
        .respond_with(delivered())
        .expect(2)
        .mount(&server)
        .await;
    deliver_due(
        restart(&store),
        Arc::new(discord_http(&server)),
        clock(3010),
    )
    .await
    .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_that!(requests.len(), eq(2));
    let retried: serde_json::Value = requests[0].body_json().unwrap();
    assert_that!(retried, eq(&failed_movement));
    for (request, (expected_count, yes, no)) in requests.iter().zip([
        (
            "2 bets placed",
            "25.0% implied chance 🔴 ⬇️",
            "75.0% implied chance 🟢 ⬆️",
        ),
        (
            "3 bets placed",
            "50.0% implied chance 🟢 ⬆️",
            "50.0% implied chance 🔴 ⬇️",
        ),
    ]) {
        let body: serde_json::Value = request.body_json().unwrap();
        let content = body["content"].as_str().unwrap();
        assert_that!(content, contains_substring("Another bet"));
        assert_that!(content, contains_substring("Will it rain?"));
        assert_that!(content, contains_substring(FIXTURE_MARKET));
        assert_that!(content, contains_substring(expected_count));
        assert_that!(content, contains_substring(format!("Yes — {yes}")));
        assert_that!(content, contains_substring(format!("No — {no}")));
        for private in ["<@7>", "13", "Bettor", "Stake"] {
            assert_that!(content, not(contains_substring(private)));
        }
        assert_that!(body["allowed_mentions"]["parse"], eq(&json!([])));
    }
}

#[googletest::test]
#[tokio::test]
async fn interaction_join_and_bet_deliver_once_after_redelivery() {
    use prediction_bot::discord::handle_interaction;
    use serenity::all::Interaction;
    use support::interaction_json;

    let (_container, store, _owner) = fixture().await;
    support::existing_destination(&store, 10, 20).await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(wiremock::matchers::path_regex("/callback$"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serenity::all::Message::default()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/20/messages"))
        .respond_with(delivered())
        .expect(3)
        .mount(&server)
        .await;
    let http = Arc::new(discord_http(&server));
    let join = Interaction::Command(
        serde_json::from_value(interaction_json(
            301,
            &json!({
                "id": "1", "name": "market", "type": 1,
                "options": [{"name": "join", "type": 1, "options": []}]
            }),
        ))
        .unwrap(),
    );
    for _ in 0..2 {
        handle_interaction(store.clone(), &http, 99.into(), join.clone()).await;
    }
    let market = Command::Create {
        id: FIXTURE_MARKET.into(),
        question: "Will it rain?".into(),
        options: vec!["Yes".into(), "No".into()],
        closes_at: 4_070_908_800,
    };
    store
        .execute(10.into(), "discord:create", admin(), &market)
        .await
        .unwrap();
    let bet = Interaction::Command(
        serde_json::from_value(interaction_json(
            302,
            &json!({
                "id": "1", "name": "market", "type": 1,
                "options": [{"name": "bet", "type": 1, "options": [
                    {"name": "id", "type": 3, "value": FIXTURE_MARKET},
                    {"name": "outcome", "type": 4, "value": 1},
                    {"name": "amount", "type": 4, "value": 10}
                ]}]
            }),
        ))
        .unwrap(),
    );
    for _ in 0..2 {
        handle_interaction(store.clone(), &http, 99.into(), bet.clone()).await;
    }
    deliver_due(store, http, clock(4_070_908_800))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let messages: Vec<serde_json::Value> = requests
        .iter()
        .filter(|request| request.url.path() == "/api/v10/channels/20/messages")
        .map(|request| request.body_json().unwrap())
        .collect();
    assert_that!(messages.len(), eq(3));
    assert_that!(
        messages[0]["content"].as_str().unwrap(),
        contains_substring("<@7> joined this server’s prediction market!")
    );
    assert_that!(
        messages[2]["content"].as_str().unwrap(),
        contains_substring("1 bet placed")
    );
    assert_that!(
        messages[2]["content"].as_str().unwrap(),
        contains_substring("Yes — 100.0% implied chance")
    );
    for message in messages {
        assert_that!(message["allowed_mentions"]["parse"], eq(&json!([])));
    }
}

#[googletest::test]
#[tokio::test]
async fn welcome_is_delivered_for_each_activation_but_not_unchanged_or_replayed_commands() {
    let (_container, store, _owner) = fixture().await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    let http = Arc::new(discord_http(&server));
    let steps = [
        ("discord:welcome-1", Some(20), 1),
        ("discord:welcome-1", Some(20), 1),
        ("discord:welcome-2", Some(20), 1),
        ("discord:welcome-3", Some(30), 2),
        ("discord:welcome-4", None, 2),
        ("discord:welcome-5", Some(30), 3),
    ];
    for (key, channel, expected_messages) in steps {
        let change = channel.map_or(ConfigurationChange::Disable, |channel| {
            ConfigurationChange::Set {
                channel_id: ChannelId(channel),
            }
        });
        store
            .configure_announcements(10.into(), key, admin(), change)
            .await
            .unwrap();
        deliver_due(store.clone(), http.clone(), clock(4_070_908_800))
            .await
            .unwrap();
        assert_that!(
            server.received_requests().await.unwrap().len(),
            eq(expected_messages),
            "command: {key}"
        );
    }
    let requests = server.received_requests().await.unwrap();
    for (request, channel) in requests.iter().zip([20, 30, 30]) {
        assert_that!(
            request.url.path(),
            eq(format!("/api/v10/channels/{channel}/messages"))
        );
        let payload: serde_json::Value = request.body_json().unwrap();
        assert_that!(
            payload["content"],
            eq(
                "Prediction market announcements are enabled! New markets, bets, and results will appear here."
            )
        );
        assert_that!(payload["allowed_mentions"]["parse"], eq(&json!([])));
    }
    let replayed = restart(&store);
    assert_that!(
        replayed.view(10.into()).await.unwrap().state.policy,
        eq(None)
    );
    replayed
        .execute_at(
            10.into(),
            "discord:welcome-join",
            admin(),
            &Command::Join,
            1000,
        )
        .await
        .unwrap();
    replayed
        .execute_at(
            10.into(),
            "discord:welcome-create",
            admin(),
            &create(FIXTURE_MARKET),
            1000,
        )
        .await
        .unwrap();
    assert_that!(
        replayed.view(10.into()).await.unwrap().state.accounts[&admin().user_id].balance,
        eq(Points(100))
    );
    let before = replayed.view(10.into()).await.unwrap();
    replayed
        .configure_announcements(
            10.into(),
            "discord:welcome-existing-economy",
            admin(),
            ConfigurationChange::Set {
                channel_id: ChannelId(40),
            },
        )
        .await
        .unwrap();
    assert_that!(
        restart(&replayed).view(10.into()).await.unwrap().state,
        eq(&before.state)
    );
}

#[googletest::test]
#[tokio::test]
async fn welcome_outbox_failure_rolls_back_configuration_and_can_be_retried() {
    let (_container, store, owner) = fixture().await;
    sqlx::query("ALTER TABLE prediction_announcement_outbox ADD CONSTRAINT reject_welcome CHECK (guild_id <> '10')").execute(&owner).await.unwrap();
    let change = ConfigurationChange::Set {
        channel_id: ChannelId(20),
    };
    assert_that!(
        store
            .configure_announcements(10.into(), "discord:welcome", admin(), change)
            .await,
        err(anything())
    );
    let status = store.announcement_status(10.into(), admin()).await.unwrap();
    assert_that!(status.enabled, eq(false));
    assert_that!(status.pending, eq(0));
    assert_that!(
        store.view(10.into()).await.unwrap().revision,
        eq(EventRevision(0))
    );
    sqlx::query("ALTER TABLE prediction_announcement_outbox DROP CONSTRAINT reject_welcome")
        .execute(&owner)
        .await
        .unwrap();
    store
        .configure_announcements(10.into(), "discord:welcome", admin(), change)
        .await
        .unwrap();
    let status = store.announcement_status(10.into(), admin()).await.unwrap();
    assert_that!(status.enabled, eq(true));
    assert_that!(status.pending, eq(1));
}

#[googletest::test]
#[tokio::test]
async fn concurrent_activation_queues_one_welcome() {
    let (_container, store, owner) = fixture().await;
    let change = ConfigurationChange::Set {
        channel_id: ChannelId(20),
    };
    let mut lock = owner.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(10)")
        .execute(&mut *lock)
        .await
        .unwrap();
    let first_store = store.clone();
    let first = tokio::spawn(async move {
        first_store
            .configure_announcements(10.into(), "discord:first", admin(), change)
            .await
    });
    let second_store = store.clone();
    let second = tokio::spawn(async move {
        second_store
            .configure_announcements(10.into(), "discord:second", admin(), change)
            .await
    });
    wait_for_guild_lock_waiters(&owner, 2).await;
    lock.commit().await.unwrap();
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/20/messages"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    deliver_due(store, Arc::new(discord_http(&server)), clock(4_070_908_800))
        .await
        .unwrap();
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
}

#[googletest::test]
#[tokio::test]
async fn welcome_retries_after_restart_and_resuming_same_paused_channel_adds_no_welcome() {
    let (_container, store, _owner) = fixture().await;
    let change = ConfigurationChange::Set {
        channel_id: ChannelId(20),
    };
    store
        .configure_announcements(10.into(), "discord:welcome", admin(), change)
        .await
        .unwrap();
    let server = MockServer::start().await;
    let http = Arc::new(discord_http(&server));
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(500).set_body_json(json!({"code":0,"message":"temporary"})),
        )
        .mount(&server)
        .await;
    let now = 4_070_908_800;
    deliver_due(store.clone(), http.clone(), clock(now))
        .await
        .unwrap();
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    let store = restart(&store);
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50013,"message":"missing permission"})),
        )
        .mount(&server)
        .await;
    deliver_due(store.clone(), http.clone(), clock(now + 4))
        .await
        .unwrap();
    assert_that!(server.received_requests().await.unwrap(), is_empty());
    deliver_due(store.clone(), http.clone(), clock(now + 5))
        .await
        .unwrap();
    assert_that!(server.received_requests().await.unwrap().len(), eq(1));
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pause_reason
            .is_some(),
        eq(true)
    );
    store
        .configure_announcements(10.into(), "discord:resume", admin(), change)
        .await
        .unwrap();
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(1)
    );
    server.reset().await;
    Mock::given(method("POST"))
        .and(path("/api/v10/channels/20/messages"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    deliver_due(store.clone(), http, clock(now + 5))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_that!(requests.len(), eq(1));
    let payload: serde_json::Value = requests[0].body_json().unwrap();
    assert_that!(
        payload["content"],
        eq(
            "Prediction market announcements are enabled! New markets, bets, and results will appear here."
        )
    );
    assert_that!(
        store
            .announcement_status(10.into(), admin())
            .await
            .unwrap()
            .pending,
        eq(0)
    );
}

#[googletest::test]
#[tokio::test]
async fn changing_destination_coalesces_pending_welcomes_without_discarding_market_activity() {
    for retrying in [false, true] {
        let (_container, store, _owner) = fixture().await;
        store
            .execute_at(10.into(), "discord:join", admin(), &Command::Join, 1000)
            .await
            .unwrap();
        store
            .configure_announcements(
                10.into(),
                "discord:enable",
                admin(),
                ConfigurationChange::Set {
                    channel_id: ChannelId(20),
                },
            )
            .await
            .unwrap();
        store
            .execute_at(
                10.into(),
                "discord:create",
                admin(),
                &create(FIXTURE_MARKET),
                1000,
            )
            .await
            .unwrap();
        // A second guild's pending welcome must remain deliverable.
        store
            .configure_announcements(
                11.into(),
                "discord:other",
                admin(),
                ConfigurationChange::Set {
                    channel_id: ChannelId(90),
                },
            )
            .await
            .unwrap();
        let server = MockServer::start().await;
        let http = Arc::new(discord_http(&server));
        let now = 4_070_908_800;
        if retrying {
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(500)
                        .set_body_json(json!({"code":0,"message":"temporary"})),
                )
                .mount(&server)
                .await;
            deliver_due(store.clone(), http.clone(), clock(now))
                .await
                .unwrap();
            assert_that!(server.received_requests().await.unwrap().len(), eq(2));
            server.reset().await;
        }
        for (key, channel) in [("discord:move-1", 30), ("discord:move-2", 40)] {
            store
                .configure_announcements(
                    10.into(),
                    key,
                    admin(),
                    ConfigurationChange::Set {
                        channel_id: ChannelId(channel),
                    },
                )
                .await
                .unwrap();
        }
        Mock::given(method("POST"))
            .respond_with(delivered())
            .mount(&server)
            .await;
        deliver_due(restart(&store), http, clock(now + 5))
            .await
            .unwrap();
        let requests = server.received_requests().await.unwrap();
        let messages: Vec<serde_json::Value> = requests
            .iter()
            .filter(|request| request.url.path() == "/api/v10/channels/40/messages")
            .map(|request| request.body_json().unwrap())
            .collect();
        assert_that!(messages.len(), eq(2), "retrying: {retrying}");
        assert_that!(
            messages[0]["content"].as_str().unwrap(),
            contains_substring("Market created")
        );
        assert_that!(
            messages[1]["content"],
            eq(
                "Prediction market announcements are enabled! New markets, bets, and results will appear here."
            )
        );
        assert_that!(requests.len(), eq(3));
        assert_that!(
            requests
                .iter()
                .filter(|request| request.url.path() == "/api/v10/channels/90/messages")
                .count(),
            eq(1)
        );
        assert_that!(
            store
                .announcement_status(10.into(), admin())
                .await
                .unwrap()
                .pending,
            eq(0)
        );
    }
}

#[googletest::test]
#[tokio::test]
async fn bet_widget_confirms_once_per_submission_through_discord() {
    const WIDGET_MARKET: &str = "aBcDeF00-0000-4000-8000-00000000000A";
    use prediction_bot::discord::handle_interaction;
    use serenity::all::Interaction;
    use support::interaction_json;

    let (_container, store, _owner) = fixture().await;
    store
        .execute_at(10.into(), "discord:801", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:802",
            admin(),
            &Command::Create {
                id: WIDGET_MARKET.into(),
                question: "Will it rain?".into(),
                options: vec!["Yes".into(), "No".into()],
                closes_at: 4_000_000_000,
            },
            1000,
        )
        .await
        .unwrap();
    let server = MockServer::start().await;
    let http = discord_http(&server);
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(support::delivered())
        .mount(&server)
        .await;
    let slash = Interaction::Command(
        serde_json::from_value(interaction_json(
            803,
            &json!({
                "id": "1", "name": "market", "type": 1,
                "options": [{"name": "bet", "type": 1, "options": []}]
            }),
        ))
        .unwrap(),
    );
    handle_interaction(store.clone(), &http, 99.into(), slash).await;
    let picker = last_widget_response(&server).await;
    assert_that!(
        picker["components"][0]["components"][0]["options"][0]["value"],
        eq(WIDGET_MARKET)
    );
    let selection = |id, custom_id: &str, values: Vec<&str>| {
        Interaction::Component(serde_json::from_value(interaction_json(id, &json!({
            "custom_id": custom_id, "component_type": if values.is_empty() {2} else {3}, "values": values,
        }))).unwrap())
    };
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(
            804,
            picker["components"][0]["components"][0]["custom_id"]
                .as_str()
                .unwrap(),
            vec![WIDGET_MARKET],
        ),
    )
    .await;
    let outcomes = last_widget_response(&server).await;
    assert_that!(outcomes["embeds"][0]["title"], eq("Will it rain?"));
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(
            805,
            outcomes["components"][0]["components"][0]["custom_id"]
                .as_str()
                .unwrap(),
            vec!["1"],
        ),
    )
    .await;
    let requests = server.received_requests().await.unwrap();
    let modal: serde_json::Value = requests
        .iter()
        .rev()
        .find(|r| r.method.as_str() == "POST")
        .unwrap()
        .body_json()
        .unwrap();
    assert_that!(modal["type"], eq(9));
    let modal_id = modal["data"]["custom_id"].as_str().unwrap();
    let submission = |id| {
        Interaction::Modal(serde_json::from_value(interaction_json(id, &json!({
        "custom_id": modal_id, "components": [{"type": 1, "components": [{"type": 4, "custom_id": "amount", "value": "10"}]}]
    }))).unwrap())
    };
    handle_interaction(store.clone(), &http, 99.into(), submission(806)).await;
    let confirmation = last_widget_response(&server).await;
    assert_that!(
        confirmation["content"].as_str().unwrap(),
        contains_substring("10")
    );
    assert_that!(
        confirmation["content"].as_str().unwrap(),
        contains_substring("No")
    );
    assert_that!(
        store.view(10.into()).await.unwrap().state.accounts[&admin().user_id].balance,
        eq(Points(100))
    );
    let confirm_id = confirmation["components"][0]["components"][0]["custom_id"]
        .as_str()
        .unwrap();
    let first = handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(807, confirm_id, vec![]),
    );
    let second = handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(808, confirm_id, vec![]),
    );
    tokio::join!(first, second);
    // Redelivery and a fresh click must both recover the same receipt.
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(807, confirm_id, vec![]),
    )
    .await;
    let view = store.view(10.into()).await.unwrap();
    assert_that!(
        view.state.accounts[&admin().user_id].balance,
        eq(Points(90))
    );
    assert_that!(view.state.markets[WIDGET_MARKET].bets.len(), eq(1));
    assert_that!(
        view.state.markets[WIDGET_MARKET].bets[0].outcome,
        eq(OutcomeIndex(1))
    );
    // A new stake submission is a separate intended bet.
    handle_interaction(store.clone(), &http, 99.into(), submission(809)).await;
    let another = last_widget_response(&server).await;
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(
            810,
            another["components"][0]["components"][0]["custom_id"]
                .as_str()
                .unwrap(),
            vec![],
        ),
    )
    .await;
    assert_that!(
        store.view(10.into()).await.unwrap().state.accounts[&admin().user_id].balance,
        eq(Points(80))
    );
    // A market cancelled after preview cannot accept the pending bet.
    handle_interaction(store.clone(), &http, 99.into(), submission(811)).await;
    let stale = last_widget_response(&server).await;
    handle_interaction(store.clone(), &http, 99.into(), submission(815)).await;
    let depleted = last_widget_response(&server).await;
    store
        .execute(
            10.into(),
            "discord:814",
            admin(),
            &Command::Bet {
                id: WIDGET_MARKET.into(),
                outcome: OutcomeIndex(0),
                amount: Points(75),
            },
        )
        .await
        .unwrap();
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(
            816,
            depleted["components"][0]["components"][0]["custom_id"]
                .as_str()
                .unwrap(),
            vec![],
        ),
    )
    .await;
    let view = store.view(10.into()).await.unwrap();
    assert_that!(view.state.accounts[&admin().user_id].balance, eq(Points(5)));
    assert_that!(view.state.markets[WIDGET_MARKET].bets.len(), eq(3));
    store
        .execute(
            10.into(),
            "discord:812",
            admin(),
            &Command::Cancel {
                id: WIDGET_MARKET.into(),
            },
        )
        .await
        .unwrap();
    let before = store.view(10.into()).await.unwrap();
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(
            813,
            stale["components"][0]["components"][0]["custom_id"]
                .as_str()
                .unwrap(),
            vec![],
        ),
    )
    .await;
    let after = store.view(10.into()).await.unwrap();
    assert_that!(after.state.accounts, eq(&before.state.accounts));
    assert_that!(after.state.markets[WIDGET_MARKET].bets.len(), eq(3));
    // Even after cancellation, a successful submission recovers its receipt.
    handle_interaction(
        store.clone(),
        &http,
        99.into(),
        selection(817, confirm_id, vec![]),
    )
    .await;
    assert_that!(
        store.view(10.into()).await.unwrap().state.accounts,
        eq(&after.state.accounts)
    );
    assert_that!(
        last_widget_response(&server).await["content"]
            .as_str()
            .unwrap(),
        contains_substring("90")
    );
    let requests = server.received_requests().await.unwrap();
    let callback = |id: u64| -> serde_json::Value {
        requests
            .iter()
            .find(|r| {
                r.url.path()
                    == format!("/api/v10/interactions/{id}/test-interaction-token/callback")
            })
            .unwrap()
            .body_json()
            .unwrap()
    };
    for id in [803, 806, 809, 811, 815] {
        assert_that!(callback(id)["type"], eq(5));
        assert_that!(callback(id)["data"]["flags"], eq(64));
    }
    for id in [804, 807, 808, 810, 813, 816, 817] {
        assert_that!(callback(id)["type"], eq(6));
    }
    for response in requests.iter().filter(|r| r.method.as_str() == "PATCH") {
        assert_that!(
            response.body_json::<serde_json::Value>().unwrap()["allowed_mentions"]["parse"],
            eq(&json!([]))
        );
    }
}

#[googletest::test]
#[tokio::test]
async fn legacy_betting_interactions_cannot_stake_points() {
    use prediction_bot::discord::handle_interaction;
    use serenity::all::Interaction;
    use support::interaction_json;

    let (_container, store, _owner) = fixture().await;
    store
        .execute_at(10.into(), "discord:820", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(
            10.into(),
            "discord:821",
            admin(),
            &Command::Create {
                id: FIXTURE_MARKET.into(),
                question: "Will it rain?".into(),
                options: vec!["Yes".into(), "No".into()],
                closes_at: 4_000_000_000,
            },
            1000,
        )
        .await
        .unwrap();
    let server = MockServer::start().await;
    let http = discord_http(&server);
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(support::delivered())
        .mount(&server)
        .await;
    let interactions = [
        Interaction::Component(
            serde_json::from_value(interaction_json(
                822,
                &json!({
                    "custom_id": format!("pm:10:7:bet:{FIXTURE_MARKET}"),
                    "component_type": 3, "values": ["1"]
                }),
            ))
            .unwrap(),
        ),
        Interaction::Modal(
            serde_json::from_value(interaction_json(
                823,
                &json!({
                    "custom_id": format!("pm:10:7:stake:{FIXTURE_MARKET}:1"),
                    "components": [{"type": 1, "components": [{
                        "type": 4, "custom_id": "amount", "value": "25"
                    }]}]
                }),
            ))
            .unwrap(),
        ),
    ];
    for interaction in interactions {
        handle_interaction(store.clone(), &http, 99.into(), interaction).await;
        let requests = server.received_requests().await.unwrap();
        let body: serde_json::Value = requests.last().unwrap().body_json().unwrap();
        let response = body.get("data").unwrap_or(&body);
        assert_that!(
            response["content"].as_str().unwrap(),
            contains_substring("/market bet")
        );
        let view = store.view(10.into()).await.unwrap();
        assert_that!(
            view.state.accounts[&admin().user_id].balance,
            eq(Points(100))
        );
        assert_that!(view.state.markets[FIXTURE_MARKET].bets, is_empty());
    }
}
