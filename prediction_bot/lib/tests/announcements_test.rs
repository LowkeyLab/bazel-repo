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
        user_id: 8,
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

fn create(id: &str) -> Command {
    Command::Create {
        id: id.into(),
        question: "Will it rain?".into(),
        options: vec!["Yes".into(), "No".into()],
        closes_at: 2000,
    }
}

#[tokio::test]
async fn market_events_enqueue_durable_snapshots_once_at_their_original_revisions() {
    let (_container, store, owner) = fixture().await;
    store
        .execute_at(10, "discord:100", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .configure_announcements(
            10,
            "discord:101",
            admin(),
            ConfigurationChange::Set { channel_id: 20 },
        )
        .await
        .unwrap();

    let resolved = create(RESOLVED_MARKET);
    let cancelled = create(CANCELLED_MARKET);
    store
        .execute_at(10, "discord:102", admin(), &resolved, 1000)
        .await
        .unwrap();
    store
        .execute_at(10, "discord:103", admin(), &cancelled, 1000)
        .await
        .unwrap();
    store
        .execute_at(
            10,
            "discord:104",
            admin(),
            &Command::Resolve {
                id: RESOLVED_MARKET.into(),
                outcome: 0,
            },
            2000,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10,
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
        .execute_at(10, "discord:102", admin(), &resolved, 2000)
        .await
        .unwrap();

    let snapshots: Vec<(i64, serde_json::Value, i64)> = sqlx::query_as(
        "SELECT revision, snapshot, next_attempt_at FROM prediction_announcement_outbox WHERE guild_id='10' ORDER BY revision",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_eq!(
        snapshots,
        vec![
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
                    "occurred_at": 2000,
                }}),
                2000,
            ),
            (
                7,
                json!({"Cancelled": {
                    "id": CANCELLED_MARKET,
                    "question": "Will it rain?",
                    "occurred_at": 2000,
                }}),
                2000,
            ),
        ],
    );
}

#[tokio::test]
async fn an_outbox_failure_rolls_back_the_market_and_receipt() {
    let (_container, store, owner) = fixture().await;
    store
        .execute_at(10, "discord:200", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .configure_announcements(
            10,
            "discord:201",
            admin(),
            ConfigurationChange::Set { channel_id: 20 },
        )
        .await
        .unwrap();
    sqlx::query("ALTER TABLE prediction_announcement_outbox ADD CONSTRAINT reject_fixture_guild CHECK (guild_id <> '10')")
        .execute(&owner)
        .await
        .unwrap();

    let result = store
        .execute_at(10, "discord:202", admin(), &create(FIXTURE_MARKET), 1000)
        .await;

    assert!(result.is_err());
    assert!(
        !store
            .view(10)
            .await
            .unwrap()
            .state
            .markets
            .contains_key(FIXTURE_MARKET)
    );
    let receipts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prediction_commands WHERE guild_id='10' AND command_key='discord:202'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(receipts, 0);
}

#[tokio::test]
async fn a_receipt_failure_rolls_back_the_enqueued_snapshot() {
    let (_container, store, owner) = fixture().await;
    store
        .execute_at(10, "discord:200", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .configure_announcements(
            10,
            "discord:201",
            admin(),
            ConfigurationChange::Set { channel_id: 20 },
        )
        .await
        .unwrap();
    sqlx::query("ALTER TABLE prediction_commands ADD CONSTRAINT reject_fixture_receipt CHECK (command_key <> 'discord:202')")
        .execute(&owner)
        .await
        .unwrap();

    let result = store
        .execute_at(10, "discord:202", admin(), &create(FIXTURE_MARKET), 1000)
        .await;

    assert!(result.is_err());
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prediction_announcement_outbox WHERE guild_id='10'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(pending, 0);
}

#[tokio::test]
async fn paused_enabled_settings_enqueue_without_backfilling_disabled_or_historical_events() {
    let (_container, store, owner) = fixture().await;
    store
        .execute_at(10, "discord:300", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(10, "discord:301", admin(), &create(HISTORICAL_MARKET), 1000)
        .await
        .unwrap();
    store
        .configure_announcements(
            10,
            "discord:302",
            admin(),
            ConfigurationChange::Set { channel_id: 20 },
        )
        .await
        .unwrap();
    let historical: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prediction_announcement_outbox WHERE guild_id='10'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(historical, 0);

    sqlx::query("UPDATE prediction_announcement_settings SET pause_reason='provider delay' WHERE guild_id='10'")
        .execute(&owner)
        .await
        .unwrap();
    store
        .execute_at(10, "discord:303", admin(), &create(PAUSED_MARKET), 1000)
        .await
        .unwrap();
    store
        .execute_at(
            10,
            "discord:304",
            admin(),
            &Command::Bet {
                id: PAUSED_MARKET.into(),
                outcome: 0,
                amount: 10,
            },
            1000,
        )
        .await
        .unwrap();
    store
        .execute_at(
            10,
            "grant:7:87400",
            Actor {
                user_id: 0,
                moderator: false,
                bot: false,
            },
            &Command::Grant { user_id: 7 },
            87_400,
        )
        .await
        .unwrap();
    let restarted = prediction_bot::store::Store::new(
        store.pool.clone(),
        42,
        Policy {
            amount: 100,
            interval: 86_400,
        },
    );
    restarted.view(10).await.unwrap();
    let pending_after_restart: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prediction_announcement_outbox WHERE guild_id='10' AND state='pending'",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(pending_after_restart, 1);

    store
        .configure_announcements(10, "discord:305", admin(), ConfigurationChange::Disable)
        .await
        .unwrap();
    store
        .execute_at(10, "discord:306", admin(), &create(DISABLED_MARKET), 1000)
        .await
        .unwrap();
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT revision, state FROM prediction_announcement_outbox WHERE guild_id='10' ORDER BY revision",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_eq!(rows, vec![(5, "discarded".into())]);
}

#[tokio::test]
async fn concurrent_disable_and_market_creation_leave_no_pending_announcement() {
    let (_container, store, owner) = fixture().await;
    store
        .execute_at(10, "discord:400", admin(), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .configure_announcements(
            10,
            "discord:401",
            admin(),
            ConfigurationChange::Set { channel_id: 20 },
        )
        .await
        .unwrap();

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
                .configure_announcements(10, "discord:402", admin(), ConfigurationChange::Disable)
                .await
        })
    };
    wait_for_guild_lock_waiters(&owner, 1).await;
    let creating = {
        let store = store.clone();
        tokio::spawn(async move {
            store
                .execute_at(10, "discord:403", admin(), &create(CONCURRENT_MARKET), 1000)
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
    assert!(states.is_empty());
}

#[tokio::test]
async fn old_configuration_receipt_cannot_restore_a_disabled_channel() {
    let (_container, store, _owner) = fixture().await;
    let change = ConfigurationChange::Set { channel_id: 20 };
    let receipt = store
        .configure_announcements(10, "discord:101", admin(), change)
        .await
        .unwrap();
    store
        .configure_announcements(10, "discord:102", admin(), ConfigurationChange::Disable)
        .await
        .unwrap();
    assert_eq!(
        store
            .configure_announcements(10, "discord:101", admin(), change)
            .await
            .unwrap(),
        receipt
    );
    let status = store.announcement_status(10, admin()).await.unwrap();
    assert!(!status.enabled);
    assert_eq!(status.pending, 0);
}

#[tokio::test]
async fn announcement_status_defaults_to_disabled_for_an_unconfigured_guild() {
    let (_container, store, _owner) = fixture().await;

    let status = store.announcement_status(10, admin()).await.unwrap();

    assert_eq!(status.channel_id, None);
    assert!(!status.enabled);
    assert_eq!(status.version, 0);
    assert_eq!(status.pause_reason, None);
    assert_eq!(status.pending, 0);
}

#[tokio::test]
async fn announcement_configuration_requires_a_human_administrator() {
    let (_container, store, _owner) = fixture().await;
    let bot_administrator = Actor {
        user_id: 9,
        moderator: true,
        bot: true,
    };

    for actor in [member(), bot_administrator] {
        assert!(matches!(
            store
                .configure_announcements(
                    10,
                    "discord:101",
                    actor,
                    ConfigurationChange::Set { channel_id: 20 },
                )
                .await,
            Err(StoreError::Configuration(_))
        ));
        assert!(matches!(
            store.announcement_status(10, actor).await,
            Err(StoreError::Configuration(_))
        ));
    }
    assert!(
        !store
            .announcement_status(10, admin())
            .await
            .unwrap()
            .enabled
    );
}

#[tokio::test]
async fn repeated_configuration_commands_replay_their_receipts_once() {
    let (_container, store, _owner) = fixture().await;
    let enabled = store
        .configure_announcements(
            10,
            "discord:101",
            admin(),
            ConfigurationChange::Set { channel_id: 20 },
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .configure_announcements(
                10,
                "discord:101",
                admin(),
                ConfigurationChange::Set { channel_id: 20 },
            )
            .await
            .unwrap(),
        enabled
    );
    assert_eq!(
        store
            .announcement_status(10, admin())
            .await
            .unwrap()
            .version,
        1
    );

    let disabled = store
        .configure_announcements(10, "discord:102", admin(), ConfigurationChange::Disable)
        .await
        .unwrap();
    assert_eq!(
        store
            .configure_announcements(10, "discord:102", admin(), ConfigurationChange::Disable,)
            .await
            .unwrap(),
        disabled
    );
    let status = store.announcement_status(10, admin()).await.unwrap();
    assert!(!status.enabled);
    assert_eq!(status.version, 2);
}

use prediction_bot::announcements::deliver_due;
use std::sync::Arc;
use support::{clock, delivered, discord_http, queued, restart};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[tokio::test]
async fn delivery_posts_saved_content_without_mentions_and_records_the_message() {
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
    assert_eq!(requests.len(), 1);
    let payload: serde_json::Value = requests[0].body_json().unwrap();
    assert!(
        payload["content"]
            .as_str()
            .unwrap()
            .contains("Market created")
    );
    assert!(
        payload["content"]
            .as_str()
            .unwrap()
            .contains("Will it rain?")
    );
    assert_eq!(payload["allowed_mentions"]["parse"], json!([]));
    assert_eq!(payload["allowed_mentions"]["replied_user"], false);
    assert_eq!(
        store
            .announcement_status(10, admin())
            .await
            .unwrap()
            .pending,
        0
    );
    let receipt: (String, String, String) = sqlx::query_as("SELECT state,delivered_channel_id,delivered_message_id FROM prediction_announcement_outbox").fetch_one(&owner).await.unwrap();
    assert_eq!(receipt, ("delivered".into(), "20".into(), "99".into()));
}

#[tokio::test]
async fn delivery_retries_after_restart_at_the_persisted_deadline() {
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(500).set_body_json(json!({"code":0,"message":"temporary"})),
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
    assert_eq!(retry, (1, 1005));
    let store = restart(&store);
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(1004))
        .await
        .unwrap();
    assert!(server.received_requests().await.unwrap().is_empty());
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(1005))
        .await
        .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert_eq!(
        store
            .announcement_status(10, admin())
            .await
            .unwrap()
            .pending,
        0
    );
}

#[tokio::test]
async fn delivery_preserves_guild_revision_order_while_other_guilds_progress() {
    let (_container, store, owner) = fixture().await;
    queued(&store, 10, 20).await;
    queued(&store, 11, 21).await;
    store
        .execute_at(
            10,
            "discord:resolve",
            admin(),
            &Command::Resolve {
                id: FIXTURE_MARKET.into(),
                outcome: 0,
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
    assert_eq!(
        store
            .announcement_status(11, admin())
            .await
            .unwrap()
            .pending,
        0
    );
    assert_eq!(
        store
            .announcement_status(10, admin())
            .await
            .unwrap()
            .pending,
        2
    );
    let failed: Vec<i64> = sqlx::query_scalar(
        "SELECT attempts FROM prediction_announcement_outbox WHERE guild_id='10' ORDER BY revision",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_eq!(failed, [1, 0]);
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(delivered())
        .mount(&server)
        .await;
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(2004))
        .await
        .unwrap();
    assert!(server.received_requests().await.unwrap().is_empty());
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(2005))
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].body_json::<serde_json::Value>().unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("Market created")
    );
    assert!(
        requests[1].body_json::<serde_json::Value>().unwrap()["content"]
            .as_str()
            .unwrap()
            .contains("Market resolved")
    );
}

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
    assert!(
        deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(1000))
            .await
            .is_err()
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert_eq!(
        store
            .announcement_status(10, admin())
            .await
            .unwrap()
            .pending,
        1
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
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    assert_eq!(
        restarted
            .announcement_status(10, admin())
            .await
            .unwrap()
            .pending,
        0
    );
}

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
            ConfigurationChange::Set { channel_id: 21 }
        };
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            store.configure_announcements(10, "discord:change", admin(), change),
        )
        .await
        .expect("HTTP must not hold the guild lock")
        .unwrap();
        barrier.release();
        delivery.await.unwrap().unwrap();
        let status = store.announcement_status(10, admin()).await.unwrap();
        assert_eq!(status.pause_reason, None);
        let row: (String, Option<String>) =
            sqlx::query_as("SELECT state,delivered_channel_id FROM prediction_announcement_outbox")
                .fetch_one(&owner)
                .await
                .unwrap();
        if disable {
            assert_eq!(row, ("discarded".into(), None));
        } else if success {
            assert_eq!(row, ("delivered".into(), Some("20".into())));
        } else {
            assert_eq!(row, ("pending".into(), None));
        }
        server.reset().await;
        Mock::given(path("/api/v10/channels/21/messages"))
            .respond_with(delivered())
            .mount(&server)
            .await;
        // Set resets the deadline using wall time; drive the worker from the saved deadline.
        let deadline: i64 =
            sqlx::query_scalar("SELECT next_attempt_at FROM prediction_announcement_outbox")
                .fetch_one(&owner)
                .await
                .unwrap();
        now.store(deadline, Ordering::SeqCst);
        deliver_due(store.clone(), Arc::new(discord_http(&server)), clock)
            .await
            .unwrap();
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            usize::from(!disable && !success)
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

#[tokio::test]
async fn enabled_announcements_do_not_enqueue_member_joins() {
    let (_container, store, owner) = fixture().await;
    store
        .configure_announcements(
            10,
            "discord:enable",
            admin(),
            ConfigurationChange::Set { channel_id: 20 },
        )
        .await
        .unwrap();
    store
        .execute_at(10, "discord:join", member(), &Command::Join, 1000)
        .await
        .unwrap();
    assert!(
        store
            .view(10)
            .await
            .unwrap()
            .state
            .accounts
            .contains_key(&8)
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM prediction_announcement_outbox")
        .fetch_one(&owner)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

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
            .announcement_status(11, admin())
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
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

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
    let status = store.announcement_status(10, admin()).await.unwrap();
    assert_eq!(status.pending, 1);
    let reason = status.pause_reason.unwrap();
    assert!(reason.contains("permission"));
    assert!(!reason.contains("sentinel"));
    deliver_due(store.clone(), Arc::new(discord_http(&server)), clock(2000))
        .await
        .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    store
        .configure_announcements(
            10,
            "discord:reset",
            admin(),
            ConfigurationChange::Set { channel_id: 21 },
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
    assert_eq!(
        store
            .announcement_status(10, admin())
            .await
            .unwrap()
            .pending,
        0
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

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
    assert_eq!(retry, (i64::MAX, 1307));
}
