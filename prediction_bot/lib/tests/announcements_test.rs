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

    let created_market = create(CONCURRENT_MARKET);
    let (disabled, created) = tokio::join!(
        store.configure_announcements(10, "discord:402", admin(), ConfigurationChange::Disable),
        store.execute_at(10, "discord:403", admin(), &created_market, 1000),
    );
    disabled.unwrap();
    created.unwrap();

    let states: Vec<String> = sqlx::query_scalar(
        "SELECT state FROM prediction_announcement_outbox WHERE guild_id='10' ORDER BY revision",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    assert!(states.is_empty() || states == ["discarded"]);
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
