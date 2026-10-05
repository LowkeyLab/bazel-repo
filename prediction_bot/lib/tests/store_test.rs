use googletest::{
    assert_that,
    matchers::{
        anything, contains_substring, elements_are, eq, err, gt, is_empty, matches_pattern, ne,
        none, ok, some,
    },
};
use prediction_bot::store::{Store, StoreError, migrate};
use prediction_bot::types::{ChannelId, GuildId, OutcomeIndex, Points, UserId};
use prediction_bot::{
    announcements::ConfigurationChange,
    audit::{AuditEvent, AuditListener, Outcome, Rejection, SharedAudit, Stage},
    domain::{Actor, Command, Policy},
};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use std::sync::{Arc, Mutex};
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, runners::AsyncRunner},
};

const RUNTIME_PASSWORD: &str = "test-runtime-'password\\with-special-characters";

async fn fixture() -> (ContainerAsync<Postgres>, Arc<Store>) {
    fixture_with_audit(prediction_bot::audit::logging_listener()).await
}

async fn fixture_with_audit(audit: SharedAudit) -> (ContainerAsync<Postgres>, Arc<Store>) {
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
        Arc::new(Store::new_with_audit(
            pool,
            42.into(),
            Policy {
                amount: Points(100),
                interval: 86_400,
            },
            audit,
        )),
    )
}

#[derive(Default)]
struct Recorder(Mutex<Vec<AuditEvent>>);

impl AuditListener for Recorder {
    fn on_event(&self, event: &AuditEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

fn recording_fixture() -> (SharedAudit, Arc<Recorder>) {
    let recorder = Arc::new(Recorder::default());
    (recorder.clone(), recorder)
}
fn player(id: u64) -> Actor {
    Actor {
        user_id: id.into(),
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

#[googletest::test]
#[tokio::test]
async fn enrollment_events_are_individually_revisioned_and_replay_after_restart() {
    let (_container, store) = fixture().await;
    let response = store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    assert_that!(
        store
            .execute_at(1.into(), "discord:1", player(7), &Command::Join, 2000)
            .await
            .unwrap(),
        eq(&response)
    );
    let before = store.view(1.into()).await.unwrap();
    assert_that!(before.state.accounts[&UserId(7)].balance.0, eq(100));
    assert_that!(before.revision.0, eq(3)); // initialize, enroll, grant: three rows, not one batch revision
    let revisions: Vec<i64> = sqlx::query_scalar(
        "SELECT revision FROM prediction_events WHERE guild_id='1' ORDER BY revision",
    )
    .fetch_all(&store.pool)
    .await
    .unwrap();
    assert_that!(revisions, eq(&vec![1, 2, 3]));
    let fresh = Store::new(
        store.pool.clone(),
        42.into(),
        Policy {
            amount: Points(999),
            interval: 1,
        },
    );
    assert_that!(fresh.view(1.into()).await.unwrap().state, eq(&before.state));
    assert_that!(
        fresh.view(2.into()).await.unwrap().state.accounts,
        is_empty()
    );
    store
        .execute_at(1.into(), "discord:2", player(7), &Command::Join, 2000)
        .await
        .unwrap();
    assert_that!(store.view(1.into()).await.unwrap().revision.0, eq(3)); // accepted no-op consumes no event revision
}

#[googletest::test]
#[tokio::test]
async fn concurrent_bets_cannot_overspend_and_duplicate_delivery_cannot_double_charge() {
    let (_container, store) = fixture().await;
    let market = uuid::Uuid::new_v4().to_string();
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(1.into(), "discord:2", player(7), &create(&market), 1000)
        .await
        .unwrap();
    let bet = Command::Bet {
        id: market.clone().into(),
        outcome: OutcomeIndex(0),
        amount: Points(80),
    };
    let (a, b) = tokio::join!(
        store.execute_at(1.into(), "discord:3", player(7), &bet, 1100),
        store.execute_at(1.into(), "discord:4", player(7), &bet, 1100)
    );
    assert_that!(a.is_ok(), ne(b.is_ok()));
    let (key, original) = if let Ok(receipt) = a {
        ("discord:3", receipt)
    } else {
        ("discord:4", b.unwrap())
    };
    assert_that!(original, contains_substring("Yes"));
    assert_that!(original, contains_substring("80 points"));
    assert_that!(original, contains_substring("20 points"));
    // A redelivery after the market closes must replay the original receipt.
    let repeated = store
        .execute_at(1.into(), key, player(7), &bet, 2100)
        .await
        .unwrap();
    assert_that!(repeated, eq(&original));
    let view = store.view(1.into()).await.unwrap();
    assert_that!(view.state.accounts[&UserId(7)].balance.0, eq(20));
    assert_that!(view.state.markets[market.as_str()].bets.len(), eq(1));
    assert_that!(view.revision.0, eq(5));
    assert_that!(
        store
            .execute_at(2.into(), "discord:5", player(7), &bet, 1200)
            .await,
        err(anything())
    );
}

#[googletest::test]
#[tokio::test]
async fn premature_grant_can_retry_when_due_and_only_credit_once() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    let command = Command::Grant { user_id: UserId(7) };
    let key = "grant:7:87400";

    // Discovery saw the deadline, but the clock moved back before execution.
    store
        .execute_at(1.into(), key, player(0), &command, 87_399)
        .await
        .unwrap();
    let early = store.view(1.into()).await.unwrap();
    assert_that!(early.state.accounts[&UserId(7)].balance.0, eq(100));
    assert_that!(early.state.accounts[&UserId(7)].next_grant, eq(87_400));

    store
        .execute_at(1.into(), key, player(0), &command, 87_400)
        .await
        .unwrap();
    let due = store.view(1.into()).await.unwrap();
    assert_that!(due.state.accounts[&UserId(7)].balance.0, eq(200));
    assert_that!(due.state.accounts[&UserId(7)].next_grant, eq(173_800));

    // Redelivery at the next deadline must still replay the successful grant.
    store
        .execute_at(1.into(), key, player(0), &command, 173_800)
        .await
        .unwrap();
    let duplicate = store.view(1.into()).await.unwrap();
    assert_that!(duplicate.state.accounts[&UserId(7)].balance.0, eq(200));
    assert_that!(duplicate.state.accounts[&UserId(7)].next_grant, eq(173_800));
}

#[googletest::test]
#[tokio::test]
async fn workers_grant_each_interval_once_and_settlement_credits_once() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    let system = Actor {
        user_id: UserId(0),
        moderator: false,
        bot: false,
    };
    let cmd = Command::Grant { user_id: UserId(7) };
    let (a, b) = tokio::join!(
        store.execute_at(1.into(), "grant:7:87400", system, &cmd, 173_800),
        store.execute_at(1.into(), "grant:7:87400", system, &cmd, 173_800)
    );
    a.unwrap();
    b.unwrap();
    assert_that!(
        store.view(1.into()).await.unwrap().state.accounts[&UserId(7)]
            .balance
            .0,
        eq(300)
    );
    let market = uuid::Uuid::new_v4().to_string();
    store
        .execute_at(1.into(), "discord:2", player(7), &create(&market), 1000)
        .await
        .unwrap();
    store
        .execute_at(
            1.into(),
            "discord:3",
            player(7),
            &Command::Bet {
                id: market.clone().into(),
                outcome: OutcomeIndex(0),
                amount: Points(30),
            },
            1100,
        )
        .await
        .unwrap();
    let moderator = Actor {
        user_id: UserId(9),
        moderator: true,
        bot: false,
    };
    let settle = Command::Resolve {
        id: market.into(),
        outcome: OutcomeIndex(0),
    };
    let (a, b) = tokio::join!(
        store.execute_at(1.into(), "discord:4", moderator, &settle, 2000),
        store.execute_at(1.into(), "discord:5", moderator, &settle, 2000)
    );
    assert_that!(a.is_ok(), ne(b.is_ok()));
    assert_that!(
        store.view(1.into()).await.unwrap().state.accounts[&UserId(7)]
            .balance
            .0,
        eq(300)
    );
}

#[googletest::test]
#[tokio::test]
async fn failed_append_rolls_back_all_events_and_does_not_consume_revisions() {
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    sqlx::query("ALTER TABLE prediction_commands ADD CONSTRAINT reject_test_command CHECK (command_key <> 'discord:99')")
        .execute(&store.pool).await.unwrap();
    assert_that!(
        store
            .execute_at(1.into(), "discord:99", player(7), &Command::Join, 1000)
            .await,
        err(anything())
    );
    assert_that!(store.view(1.into()).await.unwrap().revision.0, eq(0));
    assert_that!(
        store.view(1.into()).await.unwrap().state.accounts,
        is_empty()
    );
    assert_that!(
        recorder.0.lock().unwrap().as_slice(),
        elements_are![matches_pattern!(AuditEvent::CommandCompleted {
            key: some(eq("discord:99")),
            outcome: matches_pattern!(Outcome::Failed(anything())),
            stage: eq(&Stage::Append),
            ..
        })]
    );
    store
        .execute_at(
            1.into(),
            "discord:accepted",
            player(7),
            &Command::Join,
            1000,
        )
        .await
        .unwrap();
    assert_that!(store.view(1.into()).await.unwrap().revision.0, eq(3));
}

#[googletest::test]
#[tokio::test]
async fn legacy_command_keys_are_accepted_with_omitted_audit_correlation() {
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;

    store
        .execute_at(1.into(), "discord:legacy", player(7), &Command::Join, 1000)
        .await
        .unwrap();

    assert_that!(
        recorder.0.lock().unwrap().as_slice(),
        elements_are![matches_pattern!(AuditEvent::CommandCompleted {
            key: none(),
            outcome: eq(&Outcome::Succeeded),
            stage: eq(&Stage::Commit),
            ..
        })]
    );
}

#[googletest::test]
#[tokio::test]
async fn rejected_bet_emits_one_expected_outcome_without_changing_balance() {
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    let market = uuid::Uuid::new_v4().to_string();
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(1.into(), "discord:2", player(7), &create(&market), 1000)
        .await
        .unwrap();

    let error = store
        .execute_at(
            1.into(),
            "discord:99",
            player(7),
            &Command::Bet {
                id: market.into(),
                outcome: OutcomeIndex(0),
                amount: Points(101),
            },
            1100,
        )
        .await
        .unwrap_err();

    assert_that!(error, matches_pattern!(StoreError::Domain(anything())));
    assert_that!(
        store.view(1.into()).await.unwrap().state.accounts[&UserId(7)]
            .balance
            .0,
        eq(100)
    );
    let events = recorder.0.lock().unwrap();
    let commands: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            AuditEvent::CommandCompleted { key, outcome, .. }
                if key.as_deref() == Some("discord:99") =>
            {
                Some(outcome)
            }
            _ => None,
        })
        .collect();
    assert_that!(
        commands,
        eq(&vec![&Outcome::Rejected(Rejection::InsufficientPoints)])
    );
}

#[googletest::test]
#[tokio::test]
async fn replay_failure_emits_an_operational_command_outcome() {
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    sqlx::query("UPDATE prediction_events SET event=jsonb_set(event, '{specversion}', '\"9.0\"') WHERE guild_id='1' AND revision=2")
        .execute(&store.pool)
        .await
        .unwrap();

    assert_that!(
        store
            .execute_at(1.into(), "discord:99", player(8), &Command::Join, 1000)
            .await,
        err(anything())
    );
    assert_that!(
        recorder.0.lock().unwrap().last(),
        some(matches_pattern!(AuditEvent::CommandCompleted {
            key: some(eq("discord:99")),
            outcome: matches_pattern!(Outcome::Failed(anything())),
            stage: eq(&Stage::Replay),
            ..
        }))
    );
}

#[googletest::test]
#[tokio::test]
async fn grant_due_continues_after_a_receipt_failure_and_reports_the_schedule_key() {
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    for user in [7, 8] {
        store
            .execute_at(
                1.into(),
                &format!("discord:{user}"),
                player(user),
                &Command::Join,
                -86_400,
            )
            .await
            .unwrap();
    }
    sqlx::query("ALTER TABLE prediction_commands ADD CONSTRAINT reject_first_grant CHECK (command_key <> 'grant:7:0')")
        .execute(&store.pool)
        .await
        .unwrap();

    store.grant_due().await.unwrap();

    let view = store.view(1.into()).await.unwrap();
    assert_that!(view.state.accounts[&UserId(7)].balance.0, eq(100));
    assert_that!(view.state.accounts[&UserId(8)].balance.0, gt(100));
    assert_that!(
        recorder.0.lock().unwrap().iter().find(|event| matches!(
            event,
            AuditEvent::CommandCompleted { key: Some(key), .. } if key == "grant:7:0"
        )),
        some(matches_pattern!(AuditEvent::CommandCompleted {
            outcome: matches_pattern!(Outcome::Failed(anything())),
            stage: eq(&Stage::Append),
            ..
        }))
    );
}

#[googletest::test]
#[tokio::test]
async fn grant_due_reports_corrupt_guild_reconstruction_and_continues() {
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    store
        .execute_at(1.into(), "discord:7", player(7), &Command::Join, -86_400)
        .await
        .unwrap();
    store
        .execute_at(2.into(), "discord:8", player(8), &Command::Join, -86_400)
        .await
        .unwrap();
    sqlx::query("UPDATE prediction_events SET event=jsonb_set(event, '{specversion}', '\"9.0\"') WHERE guild_id='1' AND revision=2")
        .execute(&store.pool)
        .await
        .unwrap();

    store.grant_due().await.unwrap();

    assert_that!(
        store.view(2.into()).await.unwrap().state.accounts[&UserId(8)]
            .balance
            .0,
        gt(100)
    );
    assert_that!(
        recorder.0.lock().unwrap().iter().find(|event| matches!(
            event,
            AuditEvent::GrantFailed {
                guild: Some(GuildId(1)),
                ..
            }
        )),
        some(matches_pattern!(AuditEvent::GrantFailed {
            outcome: matches_pattern!(Outcome::Failed(anything())),
            stage: eq(&Stage::Reconstruct),
            ..
        }))
    );
}

#[googletest::test]
#[tokio::test]
async fn grant_due_returns_discovery_errors_for_the_worker_to_report() {
    let (_container, store) = fixture().await;
    store.pool.close().await;

    assert_that!(
        store.grant_due().await,
        err(matches_pattern!(StoreError::Database(matches_pattern!(
            sqlx::Error::PoolClosed
        ))))
    );
}

#[googletest::test]
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
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86_400,
        },
    );
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .configure_announcements(
            1.into(),
            "discord:2",
            Actor {
                user_id: UserId(9),
                moderator: true,
                bot: false,
            },
            ConfigurationChange::Set {
                channel_id: ChannelId(20),
            },
        )
        .await
        .unwrap();
    let status = store
        .announcement_status(
            1.into(),
            Actor {
                user_id: UserId(9),
                moderator: true,
                bot: false,
            },
        )
        .await
        .unwrap();
    assert_that!(status.enabled, eq(true));
    assert_that!(status.channel_id, eq(Some(ChannelId(20))));
    sqlx::query("INSERT INTO prediction_announcement_outbox(guild_id,revision,snapshot_version,snapshot,next_attempt_at) VALUES ('1',1,1,'{}'::jsonb,0)")
        .execute(&runtime)
        .await
        .unwrap();
    sqlx::query("UPDATE prediction_announcement_outbox SET attempts=1,next_attempt_at=5 WHERE guild_id='1' AND revision=1")
        .execute(&runtime)
        .await
        .unwrap();
    let attempts: i64 = sqlx::query_scalar(
        "SELECT attempts FROM prediction_announcement_outbox WHERE guild_id='1' AND revision=1",
    )
    .fetch_one(&runtime)
    .await
    .unwrap();
    assert_that!(attempts, eq(1));
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
        assert_that!(
            err.as_database_error().unwrap().code().as_deref(),
            eq(Some("42501"))
        );
    }
    assert_that!(
        owner.view(1.into()).await.unwrap().state.accounts[&UserId(7)]
            .balance
            .0,
        eq(100)
    );
}

#[googletest::test]
#[tokio::test]
async fn startup_rejects_a_missing_announcements_migration_record() {
    let (container, owner) = fixture().await;
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version=3")
        .execute(&owner.pool)
        .await
        .unwrap();
    let runtime_url = format!(
        "postgres://prediction_bot_app:test-runtime-%27password%5Cwith-special-characters@{}:{}/postgres",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5432).await.unwrap()
    );

    let error = Store::connect(
        &runtime_url,
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86_400,
        },
    )
    .await
    .err()
    .unwrap();

    assert_that!(
        error,
        matches_pattern!(StoreError::Configuration(anything()))
    );
}

#[googletest::test]
#[tokio::test]
async fn corrupt_or_unsupported_history_is_not_served_as_a_valid_projection() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    sqlx::query("UPDATE prediction_events SET event=jsonb_set(event, '{specversion}', '\"9.0\"') WHERE revision=2").execute(&store.pool).await.unwrap();
    assert_that!(store.view(1.into()).await, err(anything()));
}

#[googletest::test]
#[tokio::test]
async fn replay_requires_the_initial_grant_to_belong_to_its_enrollment() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    sqlx::query("UPDATE prediction_events SET event=jsonb_set(event, '{data,reason}', '\"periodic\"') WHERE revision=3").execute(&store.pool).await.unwrap();
    assert_that!(store.view(1.into()).await, err(anything()));
}

#[googletest::test]
#[tokio::test]
async fn replay_rejects_receipts_that_disagree_with_their_events() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
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
        assert_that!(
            store.view(1.into()).await,
            err(anything()),
            "accepted corrupt receipt: {corrupt}"
        );
        sqlx::query(restore).execute(&store.pool).await.unwrap();
    }
}

#[googletest::test]
#[tokio::test]
async fn gateway_lock_is_exclusive_and_released_when_connection_closes() {
    let (_container, store) = fixture().await;
    let guard = store.gateway_guard().await.unwrap();
    assert_that!(store.gateway_guard().await, err(anything()));
    guard.close().await.unwrap();
    assert_that!(store.gateway_guard().await, ok(anything()));
}

#[googletest::test]
#[tokio::test]
async fn bet_racing_resolution_cannot_leave_points_in_a_terminal_pool() {
    let (_container, store) = fixture().await;
    let market = uuid::Uuid::new_v4().to_string();
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(1.into(), "discord:2", player(7), &create(&market), 1000)
        .await
        .unwrap();
    let moderator = Actor {
        user_id: UserId(9),
        moderator: true,
        bot: false,
    };
    let bet = Command::Bet {
        id: market.clone().into(),
        outcome: OutcomeIndex(0),
        amount: Points(25),
    };
    let resolve = Command::Resolve {
        id: market.clone().into(),
        outcome: OutcomeIndex(0),
    };
    let (bet_result, resolve_result) = tokio::join!(
        store.execute_at(1.into(), "discord:3", player(7), &bet, 1999),
        store.execute_at(1.into(), "discord:4", moderator, &resolve, 2000)
    );
    resolve_result.unwrap();
    let view = store.view(1.into()).await.unwrap();
    assert_that!(view.state.accounts[&UserId(7)].balance.0, eq(100));
    assert_that!(
        view.state.markets[market.as_str()].bets.len(),
        eq(usize::from(bet_result.is_ok()))
    );
    assert_that!(
        view.state.markets[market.as_str()].status,
        matches_pattern!(prediction_bot::domain::Status::Resolved { .. })
    );
    assert_that!(
        store
            .execute_at(1.into(), "discord:5", player(7), &bet, 2001)
            .await,
        err(anything())
    );
}

#[googletest::test]
#[tokio::test]
async fn repeated_migrations_preserve_events_and_login_credentials() {
    let (container, store) = fixture().await;
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    let before = store.view(1.into()).await.unwrap();
    migrate(&store.pool, "different-password").await.unwrap();
    let versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
            .fetch_all(&store.pool)
            .await
            .unwrap();
    assert_that!(versions, eq(&vec![1, 2, 3]));
    let options = PgConnectOptions::new()
        .host(&container.get_host().await.unwrap().to_string())
        .port(container.get_host_port_ipv4(5432).await.unwrap())
        .database("postgres")
        .username("prediction_bot_app")
        .password(RUNTIME_PASSWORD);
    let runtime = PgPoolOptions::new().connect_with(options).await.unwrap();
    let restarted = Store::new(
        runtime,
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86_400,
        },
    );
    assert_that!(
        restarted.view(1.into()).await.unwrap().state,
        eq(&before.state)
    );
    assert_that!(
        restarted.view(1.into()).await.unwrap().revision.0,
        eq(before.revision.0)
    );
}

#[derive(Default)]
struct TestTransport {
    reject_acknowledgement: bool,
    edits: Mutex<Vec<serenity::builder::EditInteractionResponse>>,
}

#[serenity::async_trait]
impl prediction_bot::discord::transport::InteractionTransport for TestTransport {
    async fn acknowledge(&self) -> serenity::Result<()> {
        if self.reject_acknowledgement {
            Err(std::io::Error::other("secret transport URL").into())
        } else {
            Ok(())
        }
    }
    async fn edit(
        &self,
        response: serenity::builder::EditInteractionResponse,
    ) -> serenity::Result<()> {
        self.edits.lock().unwrap().push(response);
        Err(std::io::Error::other("secret interaction token").into())
    }
    async fn respond(
        &self,
        _: serenity::builder::CreateInteractionResponse,
    ) -> serenity::Result<()> {
        unreachable!("writes must edit their deferred response")
    }
}

#[googletest::test]
#[tokio::test]
async fn creator_resolution_through_discord_is_persisted_and_replayed_once() {
    let (_container, store) = fixture().await;
    let market = uuid::Uuid::new_v4().to_string();
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(1.into(), "discord:2", player(7), &create(&market), 1000)
        .await
        .unwrap();
    store
        .execute_at(
            1.into(),
            "discord:3",
            player(7),
            &Command::Bet {
                id: market.clone().into(),
                outcome: OutcomeIndex(0),
                amount: Points(25),
            },
            1500,
        )
        .await
        .unwrap();
    let transport = TestTransport::default();
    let resolve = Command::Resolve {
        id: market.clone().into(),
        outcome: OutcomeIndex(0),
    };
    for _ in 0..2 {
        prediction_bot::discord::execute_interaction(
            &transport,
            &store,
            1.into(),
            player(7),
            &resolve,
            4,
        )
        .await;
    }
    let restarted = Store::new(
        store.pool.clone(),
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86_400,
        },
    );
    let view = restarted.view(1.into()).await.unwrap();
    assert_that!(
        view.state.markets[market.as_str()].status,
        eq(&prediction_bot::domain::Status::Resolved {
            outcome: OutcomeIndex(0),
            refunded: false,
        })
    );
    assert_that!(view.state.accounts[&UserId(7)].balance, eq(Points(100)));
    // Three enrollment events, creation, bet, and exactly one settlement.
    assert_that!(view.revision.0, eq(6));
    let edits = transport.edits.lock().unwrap();
    assert_that!(edits.len(), eq(2));
    for edit in edits.iter() {
        assert_that!(
            serde_json::to_value(edit).unwrap()["content"],
            eq("Market resolved.")
        );
    }
}

#[googletest::test]
#[tokio::test]
async fn committed_bet_and_failed_delivery_have_matching_audit_correlation() {
    use prediction_bot::audit::{Failure, FailureCategory};
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    store
        .execute_at(1.into(), "discord:101", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    let market = uuid::Uuid::new_v4().to_string();
    let mut command = create(&market);
    if let Command::Create { closes_at, .. } = &mut command {
        *closes_at = i64::MAX;
    }
    store
        .execute_at(1.into(), "discord:102", player(7), &command, 1000)
        .await
        .unwrap();
    recorder.0.lock().unwrap().clear();
    let transport = TestTransport::default();
    let bet = Command::Bet {
        id: market.clone().into(),
        outcome: OutcomeIndex(0),
        amount: Points(80),
    };
    for _ in 0..2 {
        prediction_bot::discord::execute_interaction(
            &transport,
            &store,
            1.into(),
            player(7),
            &bet,
            123,
        )
        .await;
    }
    let view = store.view(1.into()).await.unwrap();
    assert_that!(view.state.accounts[&UserId(7)].balance.0, eq(20));
    assert_that!(view.state.markets[market.as_str()].bets.len(), eq(1));
    let receipt: String = sqlx::query_scalar(
        "SELECT response FROM prediction_commands WHERE guild_id='1' AND command_key='discord:123'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    let edits = transport.edits.lock().unwrap();
    assert_that!(edits.len(), eq(2));
    for edit in edits.iter() {
        assert_that!(serde_json::to_value(edit).unwrap()["content"], eq(&receipt));
    }
    let events = recorder.0.lock().unwrap();
    assert_that!(events.iter().filter(|event| matches!(event, AuditEvent::CommandCompleted { key: Some(key), outcome: Outcome::Succeeded, .. } if key == "discord:123")).count(), eq(2));
    assert_that!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                AuditEvent::InteractionCompleted {
                    guild: Some(GuildId(1)),
                    interaction_id: 123,
                    stage: Stage::Deliver,
                    outcome: Outcome::Failed(Failure {
                        category: FailureCategory::Transport,
                        ..
                    })
                }
            ))
            .count(),
        eq(2)
    );
}

#[googletest::test]
#[tokio::test]
async fn failed_acknowledgement_prevents_mutation_and_reports_safe_failure() {
    use prediction_bot::audit::{Failure, FailureCategory};
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    let transport = TestTransport {
        reject_acknowledgement: true,
        ..Default::default()
    };
    prediction_bot::discord::execute_interaction(
        &transport,
        &store,
        1.into(),
        player(7),
        &Command::Join,
        124,
    )
    .await;
    assert_that!(
        store.view(1.into()).await.unwrap().state.accounts,
        is_empty()
    );
    assert_that!(transport.edits.lock().unwrap().as_slice(), is_empty());
    assert_that!(
        *recorder.0.lock().unwrap(),
        eq(&vec![AuditEvent::InteractionCompleted {
            guild: Some(GuildId(1)),
            interaction_id: 124,
            stage: Stage::Acknowledge,
            outcome: Outcome::Failed(Failure {
                category: FailureCategory::Transport,
                sqlstate: None,
                http_status: None,
                discord_code: None
            }),
        }])
    );
}

#[googletest::test]
#[tokio::test]
async fn recovered_receipt_skips_verification_and_rejects_other_actors() {
    let (_container, store) = fixture().await;
    let original = store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    let recovered = store
        .execute_with_verification_at(
            1.into(),
            "discord:1",
            player(7),
            &Command::Join,
            2000,
            async { panic!("committed receipts must not depend on external verification") },
        )
        .await
        .unwrap();
    assert_that!(recovered, eq(&original));
    assert_that!(
        store
            .execute_with_verification_at(
                1.into(),
                "discord:1",
                player(8),
                &Command::Join,
                2000,
                async { panic!("actor mismatch must be rejected before verification") }
            )
            .await,
        err(matches_pattern!(StoreError::History(eq(
            &"command actor mismatch"
        ))))
    );
    assert_that!(store.view(1.into()).await.unwrap().revision.0, eq(3));
}

#[googletest::test]
#[tokio::test]
async fn receipt_committed_during_verification_wins_even_when_verification_fails() {
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    let response = store
        .execute_with_verification_at(
            1.into(),
            "discord:1",
            player(7),
            &Command::Join,
            2000,
            Box::pin(async {
                // Awaiting the competing command makes the interleaving deterministic and
                // proves verification holds neither the guild lock nor a transaction.
                store
                    .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
                    .await
                    .unwrap();
                Err(StoreError::Configuration("verification unavailable"))
            }),
        )
        .await
        .unwrap();
    assert_that!(response, eq("Enrolled with 100 points."));
    assert_that!(store.view(1.into()).await.unwrap().revision.0, eq(3));
    let accepted_at: i64 = sqlx::query_scalar("SELECT accepted_at FROM prediction_commands WHERE guild_id='1' AND command_key='discord:1'").fetch_one(&store.pool).await.unwrap();
    assert_that!(accepted_at, eq(1000));
    assert_that!(
        recorder.0.lock().unwrap().as_slice(),
        elements_are![
            matches_pattern!(AuditEvent::CommandCompleted {
                outcome: eq(&Outcome::Succeeded),
                stage: eq(&Stage::Commit),
                ..
            }),
            matches_pattern!(AuditEvent::CommandCompleted {
                outcome: eq(&Outcome::Succeeded),
                stage: eq(&Stage::Refresh),
                ..
            }),
        ]
    );
}

#[googletest::test]
#[tokio::test]
async fn command_uses_state_committed_during_verification_instead_of_cached_state() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    let market = uuid::Uuid::new_v4().to_string();
    store
        .execute_at(1.into(), "discord:2", player(7), &create(&market), 1000)
        .await
        .unwrap();
    let before = store.view(1.into()).await.unwrap();
    let bet = Command::Bet {
        id: market.clone().into(),
        outcome: OutcomeIndex(0),
        amount: Points(80),
    };
    let result = store
        .execute_with_verification_at(
            1.into(),
            "discord:3",
            player(7),
            &bet,
            1100,
            Box::pin(async {
                store
                    .execute_at(
                        1.into(),
                        "discord:4",
                        Actor {
                            moderator: true,
                            ..player(8)
                        },
                        &Command::Cancel {
                            id: market.clone().into(),
                        },
                        1100,
                    )
                    .await
                    .unwrap();
                Ok(())
            }),
        )
        .await;
    assert_that!(
        result,
        err(matches_pattern!(StoreError::Domain(anything())))
    );
    let after = store.view(1.into()).await.unwrap();
    assert_that!(after.revision.0, eq(before.revision.0 + 1));
    assert_that!(after.state.accounts[&UserId(7)].balance.0, eq(100));
    assert_that!(after.state.markets[market.as_str()].bets, is_empty());
    let receipt_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM prediction_commands WHERE command_key='discord:3'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_that!(receipt_count, eq(0));
}

#[googletest::test]
#[tokio::test]
async fn verification_failure_is_retryable_and_cannot_recover_another_guilds_receipt() {
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    recorder.0.lock().unwrap().clear();
    assert_that!(
        store
            .execute_with_verification_at(
                2.into(),
                "discord:1",
                player(7),
                &Command::Join,
                1000,
                async { Err(StoreError::Configuration("verification unavailable")) }
            )
            .await,
        err(matches_pattern!(StoreError::Configuration(anything())))
    );
    assert_that!(store.view(2.into()).await.unwrap().revision.0, eq(0));
    let receipt_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM prediction_commands WHERE guild_id='2'")
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_that!(receipt_count, eq(0));
    assert_that!(
        store
            .execute_with_verification_at(
                2.into(),
                "discord:1",
                player(7),
                &Command::Join,
                2000,
                async { Ok(()) }
            )
            .await
            .unwrap(),
        eq("Enrolled with 100 points.")
    );
    assert_that!(store.view(2.into()).await.unwrap().revision.0, eq(3));
    assert_that!(
        recorder.0.lock().unwrap().as_slice(),
        elements_are![
            matches_pattern!(AuditEvent::CommandCompleted {
                guild: eq(&GuildId(2)),
                outcome: matches_pattern!(Outcome::Failed(anything())),
                ..
            }),
            matches_pattern!(AuditEvent::CommandCompleted {
                guild: eq(&GuildId(2)),
                outcome: eq(&Outcome::Succeeded),
                ..
            }),
        ]
    );
}

#[googletest::test]
#[tokio::test]
async fn receipt_recovery_preserves_success_after_moderator_permission_loss_and_terminality() {
    let (_container, store) = fixture().await;
    store
        .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
        .await
        .unwrap();
    let market = uuid::Uuid::new_v4().to_string();
    store
        .execute_at(1.into(), "discord:2", player(7), &create(&market), 1000)
        .await
        .unwrap();
    let cancel = Command::Cancel { id: market.into() };
    let original = store
        .execute_at(
            1.into(),
            "discord:3",
            Actor {
                moderator: true,
                ..player(8)
            },
            &cancel,
            1100,
        )
        .await
        .unwrap();
    let revision = store.view(1.into()).await.unwrap().revision;
    assert_that!(
        store
            .execute_with_verification_at(1.into(), "discord:3", player(8), &cancel, 2100, async {
                panic!("historical response recovery must not need current permissions")
            })
            .await
            .unwrap(),
        eq(&original)
    );
    assert_that!(
        store
            .execute_at(1.into(), "discord:4", player(8), &cancel, 2100)
            .await,
        err(anything())
    );
    assert_that!(store.view(1.into()).await.unwrap().revision, eq(revision));
}

#[googletest::test]
#[tokio::test]
async fn event_append_and_deferred_commit_failures_leave_no_partial_events_or_receipt() {
    for fail_at_commit in [false, true] {
        let (audit, recorder) = recording_fixture();
        let (_container, store) = fixture_with_audit(audit).await;
        if fail_at_commit {
            sqlx::raw_sql("CREATE FUNCTION reject_test_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected commit failure'; END $$; CREATE CONSTRAINT TRIGGER reject_test_commit AFTER INSERT ON prediction_commands DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION reject_test_commit();").execute(&store.pool).await.unwrap();
        } else {
            sqlx::query("ALTER TABLE prediction_events ADD CONSTRAINT reject_third_event CHECK (revision <> 3)").execute(&store.pool).await.unwrap();
        }
        assert_that!(
            store
                .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
                .await,
            err(anything())
        );
        assert_that!(store.view(1.into()).await.unwrap().revision.0, eq(0));
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM prediction_commands")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_that!(receipts, eq(0));
        assert_that!(
            recorder.0.lock().unwrap().as_slice(),
            elements_are![matches_pattern!(AuditEvent::CommandCompleted {
                outcome: matches_pattern!(Outcome::Failed(anything())),
                stage: eq(&if fail_at_commit {
                    Stage::Commit
                } else {
                    Stage::Append
                }),
                ..
            })]
        );
        if fail_at_commit {
            sqlx::query("DROP TRIGGER reject_test_commit ON prediction_commands")
                .execute(&store.pool)
                .await
                .unwrap();
        } else {
            sqlx::query("ALTER TABLE prediction_events DROP CONSTRAINT reject_third_event")
                .execute(&store.pool)
                .await
                .unwrap();
        }
        store
            .execute_at(1.into(), "discord:1", player(7), &Command::Join, 1000)
            .await
            .unwrap();
        assert_that!(store.view(1.into()).await.unwrap().revision.0, eq(3));
    }
}

#[googletest::test]
#[tokio::test]
async fn resolver_addition_recovers_before_verification_and_replays_after_restart() {
    use prediction_bot::domain::MembershipEvidence;
    let (_container, store) = fixture().await;
    let guild = GuildId(1);
    let id = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";
    store
        .execute_at(guild, "discord:join1", player(1), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(guild, "discord:join2", player(2), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(guild, "discord:create", player(1), &create(id), 1000)
        .await
        .unwrap();
    let command = Command::AddResolver {
        id: id.into(),
        user_id: UserId(2),
    };
    let before = store.view(guild).await.unwrap().revision;
    let result = store
        .execute_with_membership_at(guild, "discord:add", player(1), &command, 2100, async {
            MembershipEvidence::Present {
                user_id: UserId(2),
                bot: false,
            }
        })
        .await
        .unwrap();
    let revision = store.view(guild).await.unwrap().revision;
    assert_that!(revision, eq(before.next().unwrap()));
    let recovered = store
        .execute_with_membership_at(guild, "discord:add", player(1), &command, 2200, async {
            panic!("recovery must not verify membership")
        })
        .await
        .unwrap();
    assert_that!(recovered, eq(&result));
    let duplicate = store
        .execute_with_membership_at(
            guild,
            "discord:duplicate",
            player(1),
            &command,
            2200,
            async {
                MembershipEvidence::Present {
                    user_id: UserId(2),
                    bot: false,
                }
            },
        )
        .await
        .unwrap();
    assert_that!(duplicate, contains_substring("already"));
    assert_that!(store.view(guild).await.unwrap().revision, eq(revision));
    let restarted = Store::new(
        store.pool.clone(),
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86400,
        },
    );
    let replayed = restarted.view(guild).await.unwrap();
    assert_that!(
        replayed.state.markets[id].resolvers.contains(&UserId(2)),
        eq(true)
    );
}

struct ControlledMembership(Mutex<prediction_bot::domain::MembershipEvidence>);
#[serenity::async_trait]
impl prediction_bot::discord::resolvers::MembershipVerifier for ControlledMembership {
    async fn verify(
        &self,
        guild: GuildId,
        user: UserId,
    ) -> prediction_bot::domain::MembershipEvidence {
        assert_that!(guild, eq(GuildId(1)));
        assert_that!(user, eq(UserId(2)));
        *self.0.lock().unwrap()
    }
}

#[googletest::test]
#[tokio::test]
async fn confirmed_resolver_addition_distinguishes_rejection_failure_and_lost_response() {
    use prediction_bot::{
        audit::{CommandKind, FailureCategory},
        discord::resolvers::execute_interaction,
        domain::MembershipEvidence,
    };
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    let guild = GuildId(1);
    let id = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";
    store
        .execute_at(guild, "discord:1", player(1), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(guild, "discord:2", player(2), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(guild, "discord:3", player(1), &create(id), 1000)
        .await
        .unwrap();
    let command = Command::AddResolver {
        id: id.into(),
        user_id: UserId(2),
    };
    store
        .configure_announcements(
            guild,
            "discord:configure",
            Actor {
                moderator: true,
                ..player(1)
            },
            ConfigurationChange::Set {
                channel_id: ChannelId(99),
            },
        )
        .await
        .unwrap();
    let jobs_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM prediction_announcement_outbox")
            .fetch_one(&store.pool)
            .await
            .unwrap();
    let revision = store.view(guild).await.unwrap().revision;
    let transport = TestTransport::default();
    let verifier = ControlledMembership(Mutex::new(MembershipEvidence::Unavailable));
    recorder.0.lock().unwrap().clear();
    for (evidence, outcome, stage, text) in [
        (
            MembershipEvidence::Unavailable,
            Outcome::Failed(prediction_bot::audit::Failure {
                category: FailureCategory::Discord,
                sqlstate: None,
                http_status: None,
                discord_code: None,
            }),
            Stage::Validate,
            "try again",
        ),
        (
            MembershipEvidence::Absent { user_id: UserId(2) },
            Outcome::Rejected(Rejection::NotMember),
            Stage::Decide,
            "no longer a server member",
        ),
        (
            MembershipEvidence::Present {
                user_id: UserId(2),
                bot: true,
            },
            Outcome::Rejected(Rejection::NotHuman),
            Stage::Decide,
            "human",
        ),
    ] {
        *verifier.0.lock().unwrap() = evidence;
        execute_interaction(&transport, &store, &verifier, guild, player(1), &command, 4).await;
        assert_that!(store.view(guild).await.unwrap().revision, eq(revision));
        let receipt_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM prediction_commands WHERE command_key = 'discord:4'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_that!(receipt_count, eq(0));
        let response =
            serde_json::to_value(transport.edits.lock().unwrap().last().unwrap()).unwrap();
        assert_that!(
            response["content"].as_str().unwrap(),
            contains_substring(text)
        );
        let events = recorder.0.lock().unwrap();
        let completed: Vec<_> = events
            .iter()
            .filter(|event| matches!(event, AuditEvent::CommandCompleted { .. }))
            .collect();
        assert_that!(
            *completed.last().unwrap(),
            matches_pattern!(AuditEvent::CommandCompleted {
                guild: eq(&guild),
                key: some(eq("discord:4")),
                command: eq(&CommandKind::AddResolver),
                outcome: eq(&outcome),
                stage: eq(&stage),
                ..
            })
        );
    }
    *verifier.0.lock().unwrap() = MembershipEvidence::Present {
        user_id: UserId(2),
        bot: false,
    };
    execute_interaction(&transport, &store, &verifier, guild, player(1), &command, 4).await;
    let original = serde_json::to_value(transport.edits.lock().unwrap().last().unwrap()).unwrap();
    assert_that!(
        original["content"].as_str().unwrap(),
        contains_substring("Added")
    );
    assert_that!(
        store.view(guild).await.unwrap().state.markets[id]
            .resolvers
            .contains(&UserId(2)),
        eq(true)
    );
    *verifier.0.lock().unwrap() = MembershipEvidence::Unavailable;
    execute_interaction(&transport, &store, &verifier, guild, player(1), &command, 4).await;
    assert_that!(
        serde_json::to_value(transport.edits.lock().unwrap().last().unwrap()).unwrap(),
        eq(&original)
    );
    assert_that!(
        store.view(guild).await.unwrap().revision,
        eq(revision.next().unwrap())
    );
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM prediction_announcement_outbox")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_that!(jobs, eq(jobs_before));
    let events = recorder.0.lock().unwrap();
    assert_that!(
        events
            .iter()
            .filter(|e| matches!(
                e,
                AuditEvent::CommandCompleted {
                    outcome: Outcome::Succeeded,
                    ..
                }
            ))
            .count(),
        eq(2)
    );
    assert_that!(
        events.iter().any(|e| matches!(
            e,
            AuditEvent::InteractionCompleted {
                stage: Stage::Deliver,
                outcome: Outcome::Failed(_),
                ..
            }
        )),
        eq(true)
    );
}

#[googletest::test]
#[tokio::test]
async fn resolver_concurrency_rollback_isolation_and_terminal_receipt_recovery() {
    use prediction_bot::domain::MembershipEvidence;
    let (_container, store) = fixture().await;
    let guild = GuildId(1);
    let id = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";
    for user in 1..=3 {
        store
            .execute_at(
                guild,
                &format!("discord:join{user}"),
                player(user),
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
    }
    store
        .execute_at(guild, "discord:create", player(1), &create(id), 1000)
        .await
        .unwrap();
    let add = Command::AddResolver {
        id: id.into(),
        user_id: UserId(2),
    };
    let add3 = Command::AddResolver {
        id: id.into(),
        user_id: UserId(3),
    };
    let present = || async {
        MembershipEvidence::Present {
            user_id: UserId(2),
            bot: false,
        }
    };
    let manager = Actor {
        moderator: true,
        ..player(3)
    };
    let before = store.view(guild).await.unwrap().revision;
    sqlx::query("ALTER TABLE prediction_commands ADD CONSTRAINT reject_resolver CHECK (command_key <> 'discord:rollback')").execute(&store.pool).await.unwrap();
    assert_that!(
        store
            .execute_with_membership_at(guild, "discord:rollback", manager, &add, 1001, present())
            .await,
        err(anything())
    );
    assert_that!(store.view(guild).await.unwrap().revision, eq(before));
    assert_that!(
        store.view(guild).await.unwrap().state.markets[id].resolvers,
        is_empty()
    );
    sqlx::query("ALTER TABLE prediction_commands DROP CONSTRAINT reject_resolver")
        .execute(&store.pool)
        .await
        .unwrap();
    let (first, duplicate, distinct) = tokio::join!(
        store.execute_with_membership_at(guild, "discord:add", manager, &add, 1001, present()),
        store.execute_with_membership_at(
            guild,
            "discord:duplicate",
            manager,
            &add,
            1001,
            present()
        ),
        store.execute_with_membership_at(guild, "discord:distinct", manager, &add3, 1001, async {
            MembershipEvidence::Present {
                user_id: UserId(3),
                bot: false,
            }
        }),
    );
    let original = first.unwrap();
    duplicate.unwrap();
    distinct.unwrap();
    let current = store.view(guild).await.unwrap();
    assert_that!(current.revision.0, eq(before.0 + 2));
    assert_that!(
        current.state.markets[id]
            .resolvers
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        elements_are![eq(&UserId(2)), eq(&UserId(3))]
    );
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM prediction_commands WHERE command_key IN ('discord:add','discord:duplicate','discord:distinct')").fetch_one(&store.pool).await.unwrap();
    assert_that!(receipts, eq(3));
    assert_that!(
        store
            .execute_with_membership_at(guild, "discord:add", player(1), &add, 1002, async {
                panic!("cross actor receipt may not verify")
            })
            .await,
        err(anything())
    );
    assert_that!(
        store
            .execute_with_membership_at(GuildId(2), "discord:add", manager, &add, 1002, present())
            .await,
        err(anything())
    );
    assert_that!(
        store
            .execute_with_membership_at(guild, "discord:fresh", player(3), &add, 1002, present())
            .await,
        err(anything())
    );
    assert_that!(
        store
            .execute_with_membership_at(guild, "discord:absent", manager, &add, 1002, async {
                MembershipEvidence::Absent { user_id: UserId(2) }
            })
            .await,
        err(anything())
    );
    store
        .execute_at(
            guild,
            "discord:cancel",
            manager,
            &Command::Cancel { id: id.into() },
            1003,
        )
        .await
        .unwrap();
    let final_revision = store.view(guild).await.unwrap().revision;
    assert_that!(
        store
            .execute_with_membership_at(guild, "discord:terminal", manager, &add, 1004, present())
            .await,
        err(anything())
    );
    let recovered = store
        .execute_with_membership_at(guild, "discord:add", player(3), &add, 1004, async {
            panic!("committed response precedes permission, terminality, and membership")
        })
        .await
        .unwrap();
    assert_that!(recovered, eq(&original));
    assert_that!(
        store.view(guild).await.unwrap().revision,
        eq(final_revision)
    );
}

#[googletest::test]
#[tokio::test]
async fn resolver_cloud_event_contract_rejects_corrupt_metadata_and_payloads() {
    use prediction_bot::domain::MembershipEvidence;
    let (_container, store) = fixture().await;
    let guild = GuildId(1);
    let id = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";
    for user in [1, 2] {
        store
            .execute_at(
                guild,
                &format!("discord:join{user}"),
                player(user),
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
    }
    store
        .execute_at(guild, "discord:create", player(1), &create(id), 1000)
        .await
        .unwrap();
    store
        .execute_with_membership_at(
            guild,
            "discord:add",
            player(1),
            &Command::AddResolver {
                id: id.into(),
                user_id: UserId(2),
            },
            1001,
            async {
                MembershipEvidence::Present {
                    user_id: UserId(2),
                    bot: false,
                }
            },
        )
        .await
        .unwrap();
    let original: serde_json::Value =
        sqlx::query_scalar("SELECT event FROM prediction_events WHERE command_key='discord:add'")
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_that!(
        original["type"].as_str().unwrap(),
        eq("io.lowkeylab.predictionbot.market.resolver.added.v1")
    );
    assert_that!(
        original["dataschema"].as_str().unwrap(),
        eq("urn:lowkeylab:prediction-bot:schema:market-resolver-added:v1")
    );
    assert_that!(
        original["subject"].as_str().unwrap(),
        eq("markets/78e82954-4c67-4e0d-8c80-8ab95a527ae5")
    );
    assert_that!(
        original["data"],
        eq(
            &serde_json::json!({"kind":"market_resolver_added", "id":id, "user_id":"2", "added_by":"1", "added_at":1001})
        )
    );
    for (pointer, bad) in [
        (
            "/type",
            serde_json::json!("io.lowkeylab.predictionbot.market.resolver.added.v2"),
        ),
        ("/dataschema", serde_json::json!("urn:unsupported")),
        ("/subject", serde_json::json!("markets/another")),
        ("/time", serde_json::json!("1970-01-01T00:16:42Z")),
        ("/data/added_by", serde_json::json!("3")),
        ("/data/added_at", serde_json::json!(1002)),
        ("/data/user_id", serde_json::json!(2)),
        ("/data/user_id", serde_json::json!("0")),
        ("/data/user_id", serde_json::json!("02")),
        ("/data/id", serde_json::json!("missing")),
    ] {
        let mut corrupt = original.clone();
        *corrupt.pointer_mut(pointer).unwrap() = bad;
        sqlx::query("UPDATE prediction_events SET event=$1 WHERE command_key='discord:add'")
            .bind(corrupt)
            .execute(&store.pool)
            .await
            .unwrap();
        assert_that!(
            store.view(guild).await,
            err(anything()),
            "corrupt {pointer}"
        );
        sqlx::query("UPDATE prediction_events SET event=$1 WHERE command_key='discord:add'")
            .bind(&original)
            .execute(&store.pool)
            .await
            .unwrap();
        assert_that!(
            store.view(guild).await.unwrap().state.markets[id]
                .resolvers
                .contains(&UserId(2)),
            eq(true)
        );
    }
}

#[path = "support/resolver_add.rs"]
mod guided_resolver_add;

#[googletest::test]
#[tokio::test]
async fn removal_recovers_after_terminal_and_permission_loss_without_verification() {
    use prediction_bot::{
        audit::CommandKind, discord::resolvers::execute_interaction, domain::MembershipEvidence,
    };
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    let guild = GuildId(1);
    let id = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";
    for user in [1, 2] {
        store
            .execute_at(
                guild,
                &format!("discord:join{user}"),
                player(user),
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
    }
    store
        .execute_at(guild, "discord:create", player(1), &create(id), 1000)
        .await
        .unwrap();
    let add = Command::AddResolver {
        id: id.into(),
        user_id: UserId(2),
    };
    let added = store
        .execute_with_membership_at(guild, "discord:add", player(1), &add, 1001, async {
            MembershipEvidence::Present {
                user_id: UserId(2),
                bot: false,
            }
        })
        .await
        .unwrap();
    let command = Command::RemoveResolver {
        id: id.into(),
        user_id: UserId(2),
    };
    let manager = Actor {
        moderator: true,
        ..player(3)
    };
    store
        .configure_announcements(
            guild,
            "discord:configure",
            manager,
            ConfigurationChange::Set {
                channel_id: ChannelId(99),
            },
        )
        .await
        .unwrap();
    let jobs_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM prediction_announcement_outbox")
            .fetch_one(&store.pool)
            .await
            .unwrap();
    let revision = store.view(guild).await.unwrap().revision;
    let transport = TestTransport::default();
    let verifier = NoMembershipLookup;
    recorder.0.lock().unwrap().clear();
    execute_interaction(
        &transport,
        &store,
        &verifier,
        guild,
        player(2),
        &command,
        40,
    )
    .await;
    let rejected: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM prediction_commands WHERE command_key='discord:40'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_that!(rejected, eq(0));
    assert_that!(store.view(guild).await.unwrap().revision, eq(revision));
    execute_interaction(&transport, &store, &verifier, guild, manager, &command, 40).await;
    let original = serde_json::to_value(transport.edits.lock().unwrap().last().unwrap()).unwrap();
    assert_that!(
        original["content"].as_str().unwrap(),
        contains_substring("Removed <@2>")
    );
    assert_that!(
        original["content"].as_str().unwrap(),
        contains_substring("moderator remains unchanged")
    );
    assert_that!(
        store.view(guild).await.unwrap().state.markets[id].resolvers,
        is_empty()
    );
    assert_that!(
        store.view(guild).await.unwrap().revision,
        eq(revision.next().unwrap())
    );
    execute_interaction(&transport, &store, &verifier, guild, manager, &command, 41).await;
    assert_that!(
        serde_json::to_value(transport.edits.lock().unwrap().last().unwrap()).unwrap()["content"]
            .as_str()
            .unwrap(),
        contains_substring("not an additional resolver")
    );
    assert_that!(
        store.view(guild).await.unwrap().revision,
        eq(revision.next().unwrap())
    );
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM prediction_commands WHERE command_key IN ('discord:40','discord:41')",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_that!(receipts, eq(2));
    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM prediction_announcement_outbox")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_that!(jobs, eq(jobs_before));
    assert_that!(
        store
            .execute_with_membership_at(guild, "discord:add", player(1), &add, 3000, async {
                panic!("recovered add must not verify")
            })
            .await
            .unwrap(),
        eq(&added)
    );
    assert_that!(
        store.view(guild).await.unwrap().state.markets[id].resolvers,
        is_empty()
    );
    store
        .execute_at(
            guild,
            "discord:cancel",
            manager,
            &Command::Cancel { id: id.into() },
            3001,
        )
        .await
        .unwrap();
    let final_view = store.view(guild).await.unwrap();
    let restarted = Store::new(
        store.pool.clone(),
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86_400,
        },
    );
    assert_that!(
        restarted.view(guild).await.unwrap().state,
        eq(&final_view.state)
    );
    assert_that!(
        restarted
            .execute_at(guild, "discord:40", player(3), &command, 3002)
            .await
            .unwrap(),
        eq(original["content"].as_str().unwrap())
    );
    execute_interaction(
        &transport,
        &store,
        &verifier,
        guild,
        player(3),
        &command,
        40,
    )
    .await;
    assert_that!(
        serde_json::to_value(transport.edits.lock().unwrap().last().unwrap()).unwrap(),
        eq(&original)
    );
    assert_that!(
        store
            .execute_at(guild, "discord:fresh", player(3), &command, 3002)
            .await,
        err(anything())
    );
    assert_that!(
        store
            .execute_at(guild, "discord:terminal", manager, &command, 3002)
            .await,
        err(anything())
    );
    assert_that!(
        store
            .execute_at(guild, "discord:40", player(1), &command, 3002)
            .await,
        err(anything())
    );
    assert_that!(
        store
            .execute_at(GuildId(2), "discord:40", manager, &command, 3002)
            .await,
        err(anything())
    );
    assert_that!(
        store.view(guild).await.unwrap().revision,
        eq(final_view.revision)
    );
    let events = recorder.0.lock().unwrap();
    for (key, outcome, stage) in [
        (
            "discord:40",
            Outcome::Rejected(Rejection::PermissionDenied),
            Stage::Decide,
        ),
        ("discord:41", Outcome::Succeeded, Stage::Commit),
        ("discord:40", Outcome::Succeeded, Stage::Refresh),
    ] {
        assert_that!(events.iter().any(|event| matches!(event, AuditEvent::CommandCompleted {guild:g,key:Some(k),command:CommandKind::RemoveResolver,outcome:o,stage:s,..} if *g == guild && k == key && *o == outcome && *s == stage)), eq(true));
    }
    assert_that!(
        events
            .iter()
            .filter(|event| matches!(
                event,
                AuditEvent::CommandCompleted {
                    command: CommandKind::RemoveResolver,
                    outcome: Outcome::Succeeded,
                    ..
                }
            ))
            .count(),
        eq(3)
    );
    assert_that!(
        events.iter().any(|event| matches!(
            event,
            AuditEvent::InteractionCompleted {
                stage: Stage::Deliver,
                outcome: Outcome::Failed(_),
                ..
            }
        )),
        eq(true)
    );
}

struct NoMembershipLookup;
#[serenity::async_trait]
impl prediction_bot::discord::resolvers::MembershipVerifier for NoMembershipLookup {
    async fn verify(&self, _: GuildId, _: UserId) -> prediction_bot::domain::MembershipEvidence {
        panic!("Removal never requires target membership, even during an outage")
    }
}

#[googletest::test]
#[tokio::test]
async fn removal_cloud_event_contract_rejects_corrupt_metadata_and_payloads() {
    use prediction_bot::domain::MembershipEvidence;
    let (_container, store) = fixture().await;
    let guild = GuildId(1);
    let id = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";
    for user in [1, 2] {
        store
            .execute_at(
                guild,
                &format!("discord:join{user}"),
                player(user),
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
    }
    store
        .execute_at(guild, "discord:create", player(1), &create(id), 1000)
        .await
        .unwrap();
    store
        .execute_with_membership_at(
            guild,
            "discord:add",
            player(1),
            &Command::AddResolver {
                id: id.into(),
                user_id: UserId(2),
            },
            1001,
            async {
                MembershipEvidence::Present {
                    user_id: UserId(2),
                    bot: false,
                }
            },
        )
        .await
        .unwrap();
    store
        .execute_at(
            guild,
            "discord:remove",
            player(1),
            &Command::RemoveResolver {
                id: id.into(),
                user_id: UserId(2),
            },
            1002,
        )
        .await
        .unwrap();
    let original: serde_json::Value = sqlx::query_scalar(
        "SELECT event FROM prediction_events WHERE command_key='discord:remove'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_that!(
        original["type"].as_str().unwrap(),
        eq("io.lowkeylab.predictionbot.market.resolver.removed.v1")
    );
    assert_that!(
        original["dataschema"].as_str().unwrap(),
        eq("urn:lowkeylab:prediction-bot:schema:market-resolver-removed:v1")
    );
    assert_that!(
        original["subject"].as_str().unwrap(),
        eq("markets/78e82954-4c67-4e0d-8c80-8ab95a527ae5")
    );
    assert_that!(
        original["data"],
        eq(
            &serde_json::json!({"kind":"market_resolver_removed", "id":id, "user_id":"2", "removed_by":"1", "removed_at":1002})
        )
    );
    for (pointer, bad) in [
        (
            "/type",
            serde_json::json!("io.lowkeylab.predictionbot.market.resolver.removed.v2"),
        ),
        ("/dataschema", serde_json::json!("urn:unsupported")),
        ("/subject", serde_json::json!("markets/another")),
        ("/time", serde_json::json!("1970-01-01T00:16:43Z")),
        ("/data/removed_by", serde_json::json!("3")),
        ("/data/removed_at", serde_json::json!(1003)),
        ("/data/user_id", serde_json::json!(2)),
        ("/data/user_id", serde_json::json!("0")),
        ("/data/user_id", serde_json::json!("02")),
        ("/data/id", serde_json::json!("missing")),
    ] {
        let mut corrupt = original.clone();
        *corrupt.pointer_mut(pointer).unwrap() = bad;
        sqlx::query("UPDATE prediction_events SET event=$1 WHERE command_key='discord:remove'")
            .bind(corrupt)
            .execute(&store.pool)
            .await
            .unwrap();
        assert_that!(
            store.view(guild).await,
            err(anything()),
            "corrupt {pointer}"
        );
        sqlx::query("UPDATE prediction_events SET event=$1 WHERE command_key='discord:remove'")
            .bind(&original)
            .execute(&store.pool)
            .await
            .unwrap();
        assert_that!(
            store.view(guild).await.unwrap().state.markets[id]
                .resolvers
                .contains(&UserId(2)),
            eq(false)
        );
    }
}

// Wait for PostgreSQL itself to report an advisory-lock waiter. No elapsed-time ordering.
async fn await_removal_lock_waiters(store: &Store, count: i64) {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND NOT granted",
            )
            .fetch_one(&store.pool)
            .await
            .unwrap();
            if waiting >= count {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[googletest::test]
#[tokio::test]
async fn removal_duplicate_and_add_order_follow_postgres_lock_commit_order() {
    use prediction_bot::domain::MembershipEvidence;
    let (_container, store) = fixture().await;
    let guild = GuildId(1);
    let id = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";
    for user in [1, 2] {
        store
            .execute_at(
                guild,
                &format!("discord:join{user}"),
                player(user),
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
    }
    store
        .execute_at(guild, "discord:create", player(1), &create(id), 1000)
        .await
        .unwrap();
    let add = Command::AddResolver {
        id: id.into(),
        user_id: UserId(2),
    };
    let remove = Command::RemoveResolver {
        id: id.into(),
        user_id: UserId(2),
    };
    store
        .execute_with_membership_at(guild, "discord:add", player(1), &add, 1001, async {
            MembershipEvidence::Present {
                user_id: UserId(2),
                bot: false,
            }
        })
        .await
        .unwrap();
    let revision = store.view(guild).await.unwrap().revision;
    sqlx::query("ALTER TABLE prediction_commands ADD CONSTRAINT removal_rollback CHECK (command_key <> 'discord:rollback')").execute(&store.pool).await.unwrap();
    assert_that!(
        store
            .execute_at(guild, "discord:rollback", player(1), &remove, 1002)
            .await,
        err(anything())
    );
    assert_that!(store.view(guild).await.unwrap().revision, eq(revision));
    assert_that!(
        store.view(guild).await.unwrap().state.markets[id]
            .resolvers
            .contains(&UserId(2)),
        eq(true)
    );
    sqlx::query("ALTER TABLE prediction_commands DROP CONSTRAINT removal_rollback")
        .execute(&store.pool)
        .await
        .unwrap();
    let mut lock = store.pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(1::bigint)")
        .execute(&mut *lock)
        .await
        .unwrap();
    let first_store = store.clone();
    let first_command = remove.clone();
    let first = tokio::spawn(async move {
        first_store
            .execute_at(guild, "discord:first", player(1), &first_command, 1002)
            .await
    });
    await_removal_lock_waiters(&store, 1).await;
    let second_store = store.clone();
    let second_command = remove.clone();
    let second = tokio::spawn(async move {
        second_store
            .execute_at(guild, "discord:second", player(1), &second_command, 1002)
            .await
    });
    await_removal_lock_waiters(&store, 2).await;
    lock.commit().await.unwrap();
    assert_that!(first.await.unwrap().unwrap(), contains_substring("Removed"));
    assert_that!(
        second.await.unwrap().unwrap(),
        contains_substring("not an additional resolver")
    );
    assert_that!(
        store.view(guild).await.unwrap().revision,
        eq(revision.next().unwrap())
    );
    let effects:i64=sqlx::query_scalar("SELECT count(*) FROM prediction_events WHERE event->>'type'='io.lowkeylab.predictionbot.market.resolver.removed.v1'").fetch_one(&store.pool).await.unwrap();
    assert_that!(effects, eq(1));
    let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM prediction_commands WHERE command_key IN ('discord:first','discord:second')").fetch_one(&store.pool).await.unwrap();
    assert_that!(receipts, eq(2));
    for add_first in [true, false] {
        let mut lock = store.pool.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(1::bigint)")
            .execute(&mut *lock)
            .await
            .unwrap();
        let commands = if add_first {
            [add.clone(), remove.clone()]
        } else {
            [remove.clone(), add.clone()]
        };
        let mut tasks = vec![];
        for (index, command) in commands.into_iter().enumerate() {
            let task_store = store.clone();
            tasks.push(tokio::spawn(async move {
                task_store
                    .execute_with_membership_at(
                        guild,
                        &format!("discord:order-{add_first}-{index}"),
                        player(1),
                        &command,
                        1003,
                        async {
                            MembershipEvidence::Present {
                                user_id: UserId(2),
                                bot: false,
                            }
                        },
                    )
                    .await
            }));
            await_removal_lock_waiters(&store, i64::try_from(index + 1).unwrap()).await;
        }
        lock.commit().await.unwrap();
        for task in tasks {
            task.await.unwrap().unwrap();
        }
        assert_that!(
            store.view(guild).await.unwrap().state.markets[id]
                .resolvers
                .contains(&UserId(2)),
            eq(!add_first)
        );
    }
}
