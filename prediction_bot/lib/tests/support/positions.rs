use std::sync::Arc;

use googletest::{
    assert_that,
    matchers::{contains_substring, eq},
};
use prediction_bot::{
    discord::handle_interaction,
    domain::{Command, Policy},
    store::Store,
    types::{OutcomeIndex, Points, UserId},
};
use serde_json::{Value, json};
use serenity::all::Interaction;

use super::{create, fixture, listing_http, listing_interaction, listing_response, player};

const MARKET: &str = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";
fn slash(id: u64, market: &str) -> Interaction {
    Interaction::Command(serde_json::from_value(listing_interaction(id, &json!({"id":"42","name":"market","type":1,"options":[{"name":"positions","type":1,"options":[{"name":"id","type":3,"value":market}]}]}))).unwrap())
}
async fn seed(store: &Store, market: &str, bets: &[(u64, usize, i64)]) {
    for user in [1, 2, 3, 4, 99] {
        store
            .execute_at(
                1.into(),
                &format!("discord:join:{user}"),
                player(user),
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
    }
    store
        .execute_at(
            1.into(),
            &format!("discord:create:{market}"),
            player(1),
            &create(market),
            1000,
        )
        .await
        .unwrap();
    for (i, (user, outcome, amount)) in bets.iter().enumerate() {
        store
            .execute_at(
                1.into(),
                &format!("discord:bet:{market}:{i}"),
                player(*user),
                &Command::Bet {
                    id: market.into(),
                    outcome: OutcomeIndex(*outcome),
                    amount: Points(*amount),
                },
                1500,
            )
            .await
            .unwrap();
    }
    store
        .execute_at(
            1.into(),
            &format!("discord:resolve:{market}"),
            player(1),
            &Command::Resolve {
                id: market.into(),
                outcome: OutcomeIndex(0),
            },
            2000,
        )
        .await
        .unwrap();
}
fn fresh(store: &Store) -> Arc<Store> {
    Arc::new(Store::new(
        store.pool.clone(),
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86400,
        },
    ))
}
async fn persisted(store: &Store) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY guild_id, revision) FROM prediction_events e),'receipts',(SELECT jsonb_agg(to_jsonb(c) ORDER BY guild_id, command_key) FROM prediction_commands c),'announcements',(SELECT jsonb_agg(to_jsonb(a) ORDER BY guild_id, revision) FROM prediction_announcement_outbox a))").fetch_one(&store.pool).await.unwrap()
}
#[googletest::test]
#[tokio::test]
async fn positions_show_historical_recorded_payouts_after_restart_without_writes() {
    let (_container, store) = fixture().await;
    seed(&store, MARKET, &[(2, 0, 1), (3, 0, 2), (4, 1, 2)]).await;
    sqlx::query("UPDATE prediction_events SET event=jsonb_set(event,'{data,payouts}',$1) WHERE command_key=$2").bind(json!([{"user_id":"2","amount":3},{"user_id":"3","amount":2}])).bind(format!("discord:resolve:{MARKET}")).execute(&store.pool).await.unwrap();
    let before = persisted(&store).await;
    let (server, http) = listing_http().await;
    for id in 700..702 {
        let reader = fresh(&store);
        handle_interaction(reader.clone(), &http, UserId(99), slash(id, MARKET)).await;
        let panel = listing_response(&server).await;
        let text = panel["embeds"][0]["description"].as_str().unwrap_or("");
        assert_that!(
            text,
            contains_substring("<@2>\n1. Yes: 1 points\nTotal stake: 1 · Payout: 3 · Net: +2")
        );
        assert_that!(
            text,
            contains_substring("<@3>\n1. Yes: 2 points\nTotal stake: 2 · Payout: 2 · Net: +0")
        );
        assert_that!(
            text,
            contains_substring("<@4>\n2. No: 2 points\nTotal stake: 2 · Payout: 0 · Net: -2")
        );
        assert_that!(text.contains("<@99>"), eq(false));
        assert_that!(panel["allowed_mentions"]["parse"], eq(&json!([])));
    }
    assert_that!(persisted(&store).await, eq(&before));
    assert_that!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.method == "GET"),
        eq(false)
    );
}

#[googletest::test]
#[tokio::test]
async fn positions_group_repeated_and_mixed_bets_and_identify_refunds_and_empty_settlement() {
    let (_container, store) = fixture().await;
    seed(
        &store,
        MARKET,
        &[(2, 0, 10), (2, 0, 5), (2, 1, 5), (3, 0, 10), (4, 1, 20)],
    )
    .await;
    // Pool50: recorded winners receive30 and20. Mixed bettor's all-outcome total20 yields+10.
    let before = store.view(1.into()).await.unwrap();
    let persisted_before = persisted(&store).await;
    let (server, http) = listing_http().await;
    handle_interaction(store.clone(), &http, UserId(99), slash(720, MARKET)).await;
    let result = listing_response(&server).await;
    let description = result["embeds"][0]["description"].as_str().unwrap();
    assert_that!(
        result["embeds"][0]["title"].as_str(),
        eq(Some("Will it rain?"))
    );
    assert_that!(
        description,
        contains_substring(format!("ID: {MARKET}\nWinning outcome: 1. Yes"))
    );
    assert_that!(
        description,
        contains_substring(
            "<@2>\n1. Yes: 15 points\n2. No: 5 points\nTotal stake: 20 · Payout: 30 · Net: +10"
        )
    );
    assert_that!(
        description,
        contains_substring("<@3>\n1. Yes: 10 points\nTotal stake: 10 · Payout: 20 · Net: +10")
    );
    assert_that!(
        description,
        contains_substring("<@4>\n2. No: 20 points\nTotal stake: 20 · Payout: 0 · Net: -20")
    );
    assert_that!(
        description.find("<@2>").unwrap() < description.find("<@3>").unwrap(),
        eq(true)
    );
    assert_that!(description.matches("<@2>").count(), eq(1));
    assert_that!(persisted(&store).await, eq(&persisted_before));
    let after = store.view(1.into()).await.unwrap();
    assert_that!(after.state, eq(&before.state));
    assert_that!(after.revision, eq(before.revision));
    let refund = "00000000-0000-4000-8000-000000000002";
    seed(&store, refund, &[(2, 1, 7), (2, 1, 5), (3, 1, 5)]).await;
    handle_interaction(store.clone(), &http, UserId(99), slash(721, refund)).await;
    let result = listing_response(&server).await;
    let description = result["embeds"][0]["description"].as_str().unwrap();
    assert_that!(
        description,
        contains_substring("Refunded: no bets backed the winning outcome; stakes were returned.")
    );
    assert_that!(
        description,
        contains_substring("<@2>\n2. No: 12 points\nTotal stake: 12 · Payout: 12 · Net: +0")
    );
    assert_that!(
        description,
        contains_substring("<@3>\n2. No: 5 points\nTotal stake: 5 · Payout: 5 · Net: +0")
    );
    let empty = "00000000-0000-4000-8000-000000000003";
    seed(&store, empty, &[]).await;
    let before = persisted(&store).await;
    handle_interaction(store.clone(), &http, UserId(99), slash(722, empty)).await;
    let result = listing_response(&server).await;
    assert_that!(
        result["embeds"][0]["description"].as_str().unwrap(),
        contains_substring("No bets were placed in this market.")
    );
    assert_that!(persisted(&store).await, eq(&before));
    for request in server.received_requests().await.unwrap() {
        if request.method == "POST" {
            let body: Value = request.body_json().unwrap();
            assert_that!(body["type"].as_u64(), eq(Some(5)));
            assert_that!(body["data"]["flags"].as_u64(), eq(Some(64)));
        }
    }
}

fn scoped(value: &mut Value, guild: u64, user: u64) {
    value["guild_id"] = json!(guild.to_string());
    value["user"]["id"] = json!(user.to_string());
    value["member"]["user"]["id"] = json!(user.to_string());
}
fn component(id: u64, control: &str, kind: u8, guild: u64, user: u64) -> Interaction {
    let mut value = listing_interaction(
        id,
        &json!({"custom_id":control,"component_type":kind,"values":[]}),
    );
    scoped(&mut value, guild, user);
    Interaction::Component(serde_json::from_value(value).unwrap())
}
fn button(panel: &Value, label: &str) -> Option<String> {
    panel["components"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row["components"].as_array().unwrap())
        .find(|item| item["label"] == label)
        .map(|item| item["custom_id"].as_str().unwrap().to_owned())
}
#[googletest::test]
#[tokio::test]
async fn positions_all_bettors_remain_reachable_with_ten_long_unicode_outcomes_and_maximum_scope() {
    let (_container, store) = fixture().await;
    let guild = prediction_bot::types::GuildId(u64::MAX);
    let reader = u64::MAX;
    let market = "ABCDEF12-ABCD-4ABC-8ABC-ABCDEF123456";
    for user in [1, reader].into_iter().chain(2..38) {
        store
            .execute_at(
                guild,
                &format!("discord:join:{user}"),
                player(user),
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
    }
    store
        .configure_announcements(
            guild,
            "discord:announcements",
            prediction_bot::domain::Actor {
                moderator: true,
                ..player(1)
            },
            prediction_bot::announcements::ConfigurationChange::Set {
                channel_id: 20.into(),
            },
        )
        .await
        .unwrap();
    let options: Vec<_> = (0..10).map(|n| format!("{n}{}", "😀".repeat(79))).collect();
    store
        .execute_at(
            guild,
            "discord:create",
            player(1),
            &Command::Create {
                id: market.into(),
                question: "😀".repeat(200),
                options,
                closes_at: 2000,
            },
            1000,
        )
        .await
        .unwrap();
    for user in 2..38 {
        for outcome in 0..10 {
            store
                .execute_at(
                    guild,
                    &format!("discord:bet:{user}:{outcome}"),
                    player(user),
                    &Command::Bet {
                        id: market.into(),
                        outcome: OutcomeIndex(outcome),
                        amount: Points(1),
                    },
                    1500,
                )
                .await
                .unwrap();
        }
    }
    store
        .execute_at(
            guild,
            "discord:resolve",
            player(1),
            &Command::Resolve {
                id: market.into(),
                outcome: OutcomeIndex(0),
            },
            2000,
        )
        .await
        .unwrap();
    let before = store.view(guild).await.unwrap();
    let saved = persisted(&store).await;
    let (server, http) = listing_http().await;
    let mut value = listing_interaction(
        740,
        &json!({"id":"42","name":"market","type":1,"options":[{"name":"positions","type":1,"options":[{"name":"id","type":3,"value":market}]}]}),
    );
    scoped(&mut value, guild.0, reader);
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        Interaction::Command(serde_json::from_value(value).unwrap()),
    )
    .await;
    let mut panel = listing_response(&server).await;
    let mut bettors = vec![];
    let mut pages = 0;
    loop {
        pages += 1;
        let description = panel["embeds"][0]["description"].as_str().unwrap();
        assert_that!(description.encode_utf16().count() <= 4096, eq(true));
        assert_that!(
            panel["embeds"][0]["title"]
                .as_str()
                .unwrap()
                .encode_utf16()
                .count()
                <= 256,
            eq(true)
        );
        assert_that!(
            panel["content"].as_str().unwrap().encode_utf16().count() <= 2000,
            eq(true)
        );
        assert_that!(description, contains_substring(market));
        for block in description
            .split("\n\n")
            .filter(|block| block.starts_with("<@"))
        {
            let user = block
                .lines()
                .next()
                .unwrap()
                .trim_start_matches("<@")
                .trim_end_matches('>')
                .parse::<u64>()
                .unwrap();
            bettors.push(user);
            for outcome in 1..=10 {
                assert_that!(block, contains_substring(format!("\n{outcome}. ")));
            }
            assert_that!(
                block,
                contains_substring("Total stake: 10 · Payout: 10 · Net: +0")
            );
        }
        for row in panel["components"].as_array().unwrap() {
            for control in row["components"].as_array().unwrap() {
                assert_that!(
                    control["custom_id"].as_str().unwrap().len() <= 100,
                    eq(true)
                );
            }
        }
        let Some(next) = button(&panel, "Next") else {
            break;
        };
        handle_interaction(
            store.clone(),
            &http,
            UserId(99),
            component(740 + pages, &next, 2, guild.0, reader),
        )
        .await;
        panel = listing_response(&server).await;
    }
    assert_that!(pages > 1, eq(true));
    assert_that!(bettors, eq(&(2..38).collect::<Vec<_>>()));
    let last = panel.clone();
    let previous = button(&panel, "Previous").unwrap();
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(800, &previous, 2, guild.0, reader),
    )
    .await;
    panel = listing_response(&server).await;
    let next = button(&panel, "Next").unwrap();
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(801, &next, 2, guild.0, reader),
    )
    .await;
    assert_that!(listing_response(&server).await, eq(&last));
    let boundary = format!("pm:{guild}:{reader}:ps:{market}:4294967295");
    assert_that!(boundary.len() <= 100, eq(true));
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(802, &boundary, 2, guild.0, reader),
    )
    .await;
    assert_that!(listing_response(&server).await, eq(&last));
    assert_that!(persisted(&store).await, eq(&saved));
    let after = store.view(guild).await.unwrap();
    assert_that!(after.state, eq(&before.state));
    assert_that!(after.revision, eq(before.revision));
    for request in server.received_requests().await.unwrap() {
        if request.method == "POST" {
            let body: Value = request.body_json().unwrap();
            let t = body["type"].as_u64().unwrap();
            assert_that!(t == 5 || t == 6, eq(true));
            if t == 5 {
                assert_that!(body["data"]["flags"].as_u64(), eq(Some(64)));
            }
        }
        if request.method == "PATCH" {
            let body: Value = request.body_json().unwrap();
            assert_that!(body["allowed_mentions"]["parse"], eq(&json!([])));
        }
    }
}

fn slash_for(
    id: u64,
    guild: Option<u64>,
    user: u64,
    options: Value,
    permissions: &str,
) -> Interaction {
    let mut value = listing_interaction(
        id,
        &json!({"id":"42","name":"market","type":1,"options":[{"name":"positions","type":1,"options":options}]}),
    );
    scoped(&mut value, guild.unwrap_or(1), user);
    if guild.is_none() {
        value.as_object_mut().unwrap().remove("guild_id");
    }
    value["member"]["permissions"] = json!(permissions);
    Interaction::Command(serde_json::from_value(value).unwrap())
}
fn id_option(market: &str) -> Value {
    json!([{"name":"id","type":3,"value":market}])
}
async fn denied(server: &wiremock::MockServer, text: &str) {
    let panel = listing_response(server).await;
    assert_that!(panel["content"].as_str().unwrap(), contains_substring(text));
    assert_that!(
        panel["embeds"].as_array().is_none_or(Vec::is_empty),
        eq(true)
    );
    assert_that!(
        panel["components"].as_array().is_none_or(Vec::is_empty),
        eq(true)
    );
    assert_that!(panel["allowed_mentions"]["parse"], eq(&json!([])));
}
#[googletest::test]
#[tokio::test]
async fn positions_require_guild_enrollment_and_revalidate_scope_status_and_control_payloads() {
    let (_container, store) = fixture().await;
    seed(&store, MARKET, &[(2, 0, 5), (3, 1, 5)]).await;
    store
        .execute_at(
            2.into(),
            "discord:other-join",
            player(100),
            &Command::Join,
            1000,
        )
        .await
        .unwrap();
    let open = "00000000-0000-4000-8000-000000000004";
    let cancelled = "00000000-0000-4000-8000-000000000005";
    for market in [open, cancelled] {
        store
            .execute_at(
                1.into(),
                &format!("discord:create:{market}"),
                player(1),
                &create(market),
                1000,
            )
            .await
            .unwrap();
    }
    store
        .execute_at(
            1.into(),
            "discord:cancel",
            prediction_bot::domain::Actor {
                moderator: true,
                ..player(1)
            },
            &Command::Cancel {
                id: cancelled.into(),
            },
            1500,
        )
        .await
        .unwrap();
    let saved = persisted(&store).await;
    let before = store.view(1.into()).await.unwrap();
    let (server, http) = listing_http().await;
    for (index,(guild,user,options,permissions,text)) in [
  (Some(1),100,id_option(MARKET),"0","You are not enrolled. Use /market join first."),
  (Some(1),101,id_option(MARKET),"8","You are not enrolled. Use /market join first."),
  (Some(1),101,json!([]),"8","You are not enrolled. Use /market join first."),
  (Some(2),100,id_option(MARKET),"0","does not exist in this server"),
  (Some(1),99,id_option("missing"),"0","does not exist in this server"),
  (Some(1),99,id_option(open),"0","only for resolved markets"),
  (Some(1),99,id_option(cancelled),"0","only for resolved markets"),
  (None,99,id_option(MARKET),"0","only in a server"),
  (Some(1),99,json!([{"name":"id","type":4,"value":7}]),"0","text value"),
  (Some(1),99,id_option(" "),"0","valid market ID"),
  (Some(1),99,json!([{"name":"id","type":3,"value":MARKET},{"name":"extra","type":3,"value":"ignored"}]),"0","Invalid command options"),
 ].into_iter().enumerate(){handle_interaction(store.clone(),&http,UserId(99),slash_for(830+index as u64,guild,user,options,permissions)).await;denied(&server,text).await;}
    let valid = format!("pm:1:99:ps:{MARKET}:0");
    for (index, (control, guild, user, kind, text)) in [
        (valid.clone(), 1, 100, 2, "another member or server"),
        (valid.clone(), 2, 99, 2, "another member or server"),
        (
            format!("pm:1:100:ps:{MARKET}:0"),
            1,
            100,
            2,
            "You are not enrolled. Use /market join first.",
        ),
        (
            format!("pm:2:100:ps:{MARKET}:0"),
            2,
            100,
            2,
            "does not exist in this server",
        ),
        (
            format!("pm:1:99:ps:{open}:0"),
            1,
            99,
            2,
            "only for resolved markets",
        ),
        (
            format!("pm:1:99:ps:{cancelled}:0"),
            1,
            99,
            2,
            "only for resolved markets",
        ),
        (
            format!("pm:1:99:ps:missing:0"),
            1,
            99,
            2,
            "does not exist in this server",
        ),
        (
            format!("pm:1:99:ps:{MARKET}:4294967296"),
            1,
            99,
            2,
            "positions control is invalid",
        ),
        (
            format!("pm:1:99:ps:{MARKET}:-1"),
            1,
            99,
            2,
            "positions control is invalid",
        ),
        (
            format!("pm:1:99:ps:{MARKET}:0:extra"),
            1,
            99,
            2,
            "positions control is invalid",
        ),
        (valid, 1, 99, 3, "positions control is invalid"),
    ]
    .into_iter()
    .enumerate()
    {
        handle_interaction(
            store.clone(),
            &http,
            UserId(99),
            component(860 + index as u64, &control, kind, guild, user),
        )
        .await;
        denied(&server, text).await;
    }
    assert_that!(persisted(&store).await, eq(&saved));
    let after = store.view(1.into()).await.unwrap();
    assert_that!(after.state, eq(&before.state));
    assert_that!(after.revision, eq(before.revision));
    assert_that!(after.state.accounts.contains_key(&UserId(100)), eq(false));
    assert_that!(after.state.accounts.contains_key(&UserId(101)), eq(false));
    assert_that!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.method == "GET"),
        eq(false)
    );
}

#[googletest::test]
#[tokio::test]
async fn positions_query_and_delivery_failures_are_private_structured_and_read_only() {
    use super::{fixture_with_audit, recording_fixture};
    use prediction_bot::audit::{AuditEvent, FailureCategory, Outcome, QueryKind, Stage};
    use wiremock::{Mock, ResponseTemplate, matchers::method};
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    seed(&store, MARKET, &[(2, 0, 5), (3, 1, 5)]).await;
    let saved = persisted(&store).await;
    let (server, http) = listing_http().await;
    Mock::given(method("PATCH"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50013,"message":"private provider diagnostic"})),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    recorder.0.lock().unwrap().clear();
    handle_interaction(store.clone(), &http, UserId(99), slash(900, MARKET)).await;
    {
        let events = recorder.0.lock().unwrap();
        assert_that!(
            events.iter().any(|e| matches!(
                e,
                AuditEvent::QueryCompleted {
                    interaction_id: 900,
                    query: QueryKind::Positions,
                    outcome: Outcome::Succeeded,
                    ..
                }
            )),
            eq(true)
        );
        assert_that!(events.iter().any(|e|matches!(e,AuditEvent::InteractionCompleted{interaction_id:900,stage:Stage::Deliver,outcome:Outcome::Failed(failure),..} if failure.category==FailureCategory::Discord && failure.http_status==Some(403) && failure.discord_code==Some(50013))),eq(true));
    }
    assert_that!(persisted(&store).await, eq(&saved));
    let witness_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with((*store.pool.connect_options()).clone())
        .await
        .unwrap();
    let witness = Store::new(
        witness_pool,
        42.into(),
        Policy {
            amount: Points(100),
            interval: 86400,
        },
    );
    store.pool.close().await;
    let (server, http) = listing_http().await;
    recorder.0.lock().unwrap().clear();
    handle_interaction(store.clone(), &http, UserId(99), slash(901, MARKET)).await;
    denied(&server, "temporarily unavailable").await;
    {
        let events = recorder.0.lock().unwrap();
        assert_that!(
            events.iter().any(|e| matches!(
                e,
                AuditEvent::QueryCompleted {
                    interaction_id: 901,
                    query: QueryKind::Positions,
                    outcome: Outcome::Failed(_),
                    ..
                }
            )),
            eq(true)
        );
    }
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(902, &format!("pm:1:99:ps:{MARKET}:0"), 2, 1, 99),
    )
    .await;
    denied(&server, "temporarily unavailable").await;
    assert_that!(persisted(&witness).await, eq(&saved));
    let (server, http) = listing_http().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50013,"message":"acknowledgement failure"})),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    recorder.0.lock().unwrap().clear();
    handle_interaction(store.clone(), &http, UserId(99), slash(903, MARKET)).await;
    assert_that!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|r| r.method == "PATCH"),
        eq(false)
    );
    assert_that!(
        recorder.0.lock().unwrap().iter().any(|e| matches!(
            e,
            AuditEvent::QueryCompleted {
                interaction_id: 903,
                ..
            }
        )),
        eq(false)
    );
    assert_that!(persisted(&witness).await, eq(&saved));
}
