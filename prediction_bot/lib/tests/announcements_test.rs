use prediction_bot::{announcements::ConfigurationChange, domain::Actor, store::StoreError};

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
