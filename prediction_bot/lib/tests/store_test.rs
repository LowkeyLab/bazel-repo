use prediction_bot::domain::{Actor, Command, Policy};
use prediction_bot::store::{Store, migrate};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use std::sync::Arc;
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, runners::AsyncRunner},
};

const RUNTIME_PASSWORD: &str = "test-runtime-'password\\with-special-characters";

async fn fixture() -> (ContainerAsync<Postgres>, Arc<Store>) {
    let container = test_images::postgres().await.start().await.unwrap();
    let url = format!(
        "postgres://postgres:postgres@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    );
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .unwrap();
    migrate(&pool, RUNTIME_PASSWORD).await.unwrap();
    (
        container,
        Arc::new(Store::new(
            pool,
            42,
            Policy {
                amount: 100,
                interval: 86_400,
            },
        )),
    )
}
fn player(id: u64) -> Actor {
    Actor {
        user_id: id,
        moderator: false,
        bot: false,
    }
}
fn create(id: &str) -> Command {
    Command::Create {
        id: id.into(),
        question: "Will it rain?".into(),
        options: vec!["Yes".into(), "No".into()],
        closes_at: 2000,
    }
}

#[tokio::test]
async fn enrollment_events_are_individually_revisioned_and_replay_after_restart() {
    let (_container, store) = fixture().await;
    let response = store
        .execute_at(1, "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    assert_eq!(
        store
            .execute_at(1, "discord:1", player(7), &Command::Join, 2000)
            .await
            .unwrap(),
        response
    );
    let before = store.view(1).await.unwrap();
    assert_eq!(before.state.accounts[&7].balance, 100);
    assert_eq!(before.revision, 3); // initialize, enroll, grant: three rows, not one batch revision
    let revisions: Vec<i64> = sqlx::query_scalar(
        "SELECT revision FROM prediction_events WHERE guild_id='1' ORDER BY revision",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    assert_eq!(revisions, vec![1, 2, 3]);
    let fresh = Store::new(
        store.pool.clone(),
        42,
        Policy {
            amount: 999,
            interval: 1,
        },
    );
    assert_eq!(fresh.view(1).await.unwrap().state, before.state);
    assert!(fresh.view(2).await.unwrap().state.accounts.is_empty());
    store
        .execute_at(1, "discord:2", player(7), &Command::Join, 2000)
        .await
        .unwrap();
    assert_eq!(store.view(1).await.unwrap().revision, 3); // accepted no-op consumes no event revision
}

#[tokio::test]
async fn concurrent_bets_cannot_overspend_and_duplicate_delivery_cannot_double_charge() {
    let (_container, store) = fixture().await;
    let market = uuid::Uuid::new_v4().to_string();
    store
        .execute_at(1, "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(1, "discord:2", player(7), &create(&market), 1000)
        .await
        .unwrap();
    let bet = Command::Bet {
        id: market.clone(),
        outcome: 0,
        amount: 80,
    };
    let (a, b) = tokio::join!(
        store.execute_at(1, "discord:3", player(7), &bet, 1100),
        store.execute_at(1, "discord:4", player(7), &bet, 1100)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let (key, original) = if let Ok(receipt) = a {
        ("discord:3", receipt)
    } else {
        ("discord:4", b.unwrap())
    };
    assert!(original.contains("Yes"));
    assert!(original.contains("80 points"));
    assert!(original.contains("20 points"));
    // A redelivery after the market closes must replay the original receipt.
    let repeated = store
        .execute_at(1, key, player(7), &bet, 2100)
        .await
        .unwrap();
    assert_eq!(repeated, original);
    let view = store.view(1).await.unwrap();
    assert_eq!(view.state.accounts[&7].balance, 20);
    assert_eq!(view.state.markets[&market].bets.len(), 1);
    assert_eq!(view.revision, 5);
    assert!(
        store
            .execute_at(2, "discord:5", player(7), &bet, 1200)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn premature_grant_can_retry_when_due_and_only_credit_once() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1, "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    let command = Command::Grant { user_id: 7 };
    let key = "grant:7:87400";

    // Discovery saw the deadline, but the clock moved back before execution.
    store
        .execute_at(1, key, player(0), &command, 87_399)
        .await
        .unwrap();
    let early = store.view(1).await.unwrap();
    assert_eq!(early.state.accounts[&7].balance, 100);
    assert_eq!(early.state.accounts[&7].next_grant, 87_400);

    store
        .execute_at(1, key, player(0), &command, 87_400)
        .await
        .unwrap();
    let due = store.view(1).await.unwrap();
    assert_eq!(due.state.accounts[&7].balance, 200);
    assert_eq!(due.state.accounts[&7].next_grant, 173_800);

    // Redelivery at the next deadline must still replay the successful grant.
    store
        .execute_at(1, key, player(0), &command, 173_800)
        .await
        .unwrap();
    let duplicate = store.view(1).await.unwrap();
    assert_eq!(duplicate.state.accounts[&7].balance, 200);
    assert_eq!(duplicate.state.accounts[&7].next_grant, 173_800);
}

#[tokio::test]
async fn workers_grant_each_interval_once_and_settlement_credits_once() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1, "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    let system = Actor {
        user_id: 0,
        moderator: false,
        bot: false,
    };
    let cmd = Command::Grant { user_id: 7 };
    let (a, b) = tokio::join!(
        store.execute_at(1, "grant:7:87400", system, &cmd, 173_800),
        store.execute_at(1, "grant:7:87400", system, &cmd, 173_800)
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(store.view(1).await.unwrap().state.accounts[&7].balance, 300);
    let market = uuid::Uuid::new_v4().to_string();
    store
        .execute_at(1, "discord:2", player(7), &create(&market), 1000)
        .await
        .unwrap();
    store
        .execute_at(
            1,
            "discord:3",
            player(7),
            &Command::Bet {
                id: market.clone(),
                outcome: 0,
                amount: 30,
            },
            1100,
        )
        .await
        .unwrap();
    let moderator = Actor {
        user_id: 9,
        moderator: true,
        bot: false,
    };
    let settle = Command::Resolve {
        id: market,
        outcome: 0,
    };
    let (a, b) = tokio::join!(
        store.execute_at(1, "discord:4", moderator, &settle, 2000),
        store.execute_at(1, "discord:5", moderator, &settle, 2000)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert_eq!(store.view(1).await.unwrap().state.accounts[&7].balance, 300);
}

#[tokio::test]
async fn failed_append_rolls_back_all_events_and_does_not_consume_revisions() {
    let (_container, store) = fixture().await;
    sqlx::query("ALTER TABLE prediction_commands ADD CONSTRAINT reject_test_command CHECK (command_key <> 'discord:reject')")
        .execute(&store.pool).await.unwrap();
    assert!(
        store
            .execute_at(1, "discord:reject", player(7), &Command::Join, 1000)
            .await
            .is_err()
    );
    assert_eq!(store.view(1).await.unwrap().revision, 0);
    assert!(store.view(1).await.unwrap().state.accounts.is_empty());
    store
        .execute_at(1, "discord:accepted", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    assert_eq!(store.view(1).await.unwrap().revision, 3);
}

#[tokio::test]
async fn runtime_role_can_append_but_cannot_change_history() {
    let (container, owner) = fixture().await;
    let options = PgConnectOptions::new()
        .host(&container.get_host().await.unwrap().to_string())
        .port(container.get_host_port_ipv4(5432).await.unwrap())
        .database("postgres")
        .username("prediction_bot_app")
        .password(RUNTIME_PASSWORD);
    let runtime = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .unwrap();
    let store = Store::new(
        runtime.clone(),
        42,
        Policy {
            amount: 100,
            interval: 86_400,
        },
    );
    store
        .execute_at(1, "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    for query in [
        "UPDATE prediction_events SET accepted_at=0",
        "DELETE FROM prediction_events",
        "TRUNCATE prediction_events",
        "UPDATE prediction_commands SET response='changed'",
        "DELETE FROM prediction_commands",
        "TRUNCATE prediction_commands CASCADE",
        "DELETE FROM _sqlx_migrations",
        "CREATE ROLE unauthorized_role",
    ] {
        let err = sqlx::query(query).execute(&runtime).await.unwrap_err();
        assert_eq!(
            err.as_database_error().unwrap().code().as_deref(),
            Some("42501")
        );
    }
    assert_eq!(owner.view(1).await.unwrap().state.accounts[&7].balance, 100);
}

#[tokio::test]
async fn corrupt_or_unsupported_history_is_not_served_as_a_valid_projection() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1, "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    sqlx::query("UPDATE prediction_events SET event=jsonb_set(event, '{specversion}', '\"9.0\"') WHERE revision=2").execute(&store.pool).await.unwrap();
    assert!(store.view(1).await.is_err());
}

#[tokio::test]
async fn replay_requires_the_initial_grant_to_belong_to_its_enrollment() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1, "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    sqlx::query("UPDATE prediction_events SET event=jsonb_set(event, '{data,reason}', '\"periodic\"') WHERE revision=3").execute(&store.pool).await.unwrap();
    assert!(store.view(1).await.is_err());
}

#[tokio::test]
async fn replay_rejects_receipts_that_disagree_with_their_events() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1, "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    for (corrupt, restore) in [
        (
            "UPDATE prediction_commands SET accepted_at=999",
            "UPDATE prediction_commands SET accepted_at=1000",
        ),
        (
            "UPDATE prediction_commands SET last_revision=2",
            "UPDATE prediction_commands SET last_revision=3",
        ),
        (
            "UPDATE prediction_commands SET actor_id='8'",
            "UPDATE prediction_commands SET actor_id='7'",
        ),
    ] {
        sqlx::query(corrupt).execute(&store.pool).await.unwrap();
        assert!(
            store.view(1).await.is_err(),
            "accepted corrupt receipt: {corrupt}"
        );
        sqlx::query(restore).execute(&store.pool).await.unwrap();
    }
}

#[tokio::test]
async fn gateway_lock_is_exclusive_and_released_when_connection_closes() {
    let (_container, store) = fixture().await;
    let guard = store.gateway_guard().await.unwrap();
    assert!(store.gateway_guard().await.is_err());
    guard.close().await.unwrap();
    assert!(store.gateway_guard().await.is_ok());
}

#[tokio::test]
async fn bet_racing_resolution_cannot_leave_points_in_a_terminal_pool() {
    let (_container, store) = fixture().await;
    let market = uuid::Uuid::new_v4().to_string();
    store
        .execute_at(1, "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(1, "discord:2", player(7), &create(&market), 1000)
        .await
        .unwrap();
    let moderator = Actor {
        user_id: 9,
        moderator: true,
        bot: false,
    };
    let bet = Command::Bet {
        id: market.clone(),
        outcome: 0,
        amount: 25,
    };
    let resolve = Command::Resolve {
        id: market.clone(),
        outcome: 0,
    };
    let (bet_result, resolve_result) = tokio::join!(
        store.execute_at(1, "discord:3", player(7), &bet, 1999),
        store.execute_at(1, "discord:4", moderator, &resolve, 2000)
    );
    resolve_result.unwrap();
    let view = store.view(1).await.unwrap();
    assert_eq!(view.state.accounts[&7].balance, 100);
    assert_eq!(
        view.state.markets[&market].bets.len(),
        usize::from(bet_result.is_ok())
    );
    assert!(matches!(
        view.state.markets[&market].status,
        prediction_bot::domain::Status::Resolved { .. }
    ));
    assert!(
        store
            .execute_at(1, "discord:5", player(7), &bet, 2001)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn repeated_migrations_preserve_events_and_login_credentials() {
    let (container, store) = fixture().await;
    store
        .execute_at(1, "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    let before = store.view(1).await.unwrap();
    migrate(&store.pool, "different-password").await.unwrap();
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
            .fetch_all(&store.pool)
            .await
            .unwrap();
    assert_eq!(versions, vec![1, 2]);
    let options = PgConnectOptions::new()
        .host(&container.get_host().await.unwrap().to_string())
        .port(container.get_host_port_ipv4(5432).await.unwrap())
        .database("postgres")
        .username("prediction_bot_app")
        .password(RUNTIME_PASSWORD);
    let runtime = PgPoolOptions::new().connect_with(options).await.unwrap();
    let restarted = Store::new(
        runtime,
        42,
        Policy {
            amount: 100,
            interval: 86_400,
        },
    );
    assert_eq!(restarted.view(1).await.unwrap().state, before.state);
    assert_eq!(restarted.view(1).await.unwrap().revision, before.revision);
}
