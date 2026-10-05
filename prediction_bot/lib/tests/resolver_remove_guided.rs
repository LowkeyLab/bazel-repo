//! Acceptance through the real gateway dispatcher, `PostgreSQL` and outgoing Discord HTTP.
use super::{create, fixture, fixture_with_audit, player, recording_fixture};
use googletest::{
    assert_that,
    matchers::{contains_substring, eq, is_empty},
};
use prediction_bot::audit::{AuditEvent, Outcome, QueryKind};
use prediction_bot::{
    discord::handle_interaction,
    domain::{Actor, Command, MembershipEvidence},
    store::Store,
    types::{GuildId, UserId},
};
use serde_json::{Value, json};
use serenity::{all::Interaction, http::Http};
use std::sync::Arc;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

const MARKET: &str = "ABCDEF12-3456-4789-ABCD-EF1234567890";

async fn seed(store: &Store) {
    for user in [7, 8] {
        store
            .execute_at(
                GuildId(1),
                &format!("discord:join{user}"),
                player(user),
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
    }
    store
        .execute_at(
            GuildId(1),
            "discord:create",
            player(7),
            &create(MARKET),
            1000,
        )
        .await
        .unwrap();
    store
        .execute_with_membership_at(
            GuildId(1),
            "discord:add",
            player(7),
            &Command::AddResolver {
                id: MARKET.into(),
                user_id: UserId(8),
            },
            1000,
            async {
                MembershipEvidence::Present {
                    user_id: UserId(8),
                    bot: false,
                }
            },
        )
        .await
        .unwrap();
}

async fn discord() -> (MockServer, Http) {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let message = serenity::all::Message::default();
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(message))
        .mount(&server)
        .await;
    let http = serenity::http::HttpBuilder::new("test-token")
        .application_id(42.into())
        .proxy(server.uri())
        .ratelimiter_disabled(true)
        .build();
    (server, http)
}

fn payload(id: u64, data: &Value, actor: Actor, guild: GuildId) -> Value {
    let user = json!({"id":actor.user_id.to_string(),"username":"manager","discriminator":"0","avatar":null,"bot":actor.bot});
    json!({"id":id.to_string(),"application_id":"42","guild_id":guild.to_string(),"channel_id":"20","token":"test-token","version":1,"locale":"en-US","entitlements":[],"attachment_size_limit":1000,"data":data,"member":{"permissions":if actor.moderator {"32"} else {"0"},"roles":[],"deaf":false,"mute":false,"flags":0,"joined_at":null,"premium_since":null,"user":user},"user":user,"message":serenity::all::Message::default()})
}

fn slash(id: u64, options: &[Value], actor: Actor) -> Interaction {
    Interaction::Command(serde_json::from_value(payload(id, &json!({"id":"1","name":"market","type":1,"options":[{"name":"resolver","type":2,"options":[{"name":"remove","type":1,"options":options}]}]}), actor, GuildId(1))).unwrap())
}

fn component(
    id: u64,
    custom_id: &str,
    values: &[&str],
    actor: Actor,
    guild: GuildId,
) -> Interaction {
    Interaction::Component(serde_json::from_value(payload(id, &json!({"custom_id":custom_id,"component_type":if values.is_empty() {2} else {3},"values":values}), actor, guild)).unwrap())
}

async fn send(
    store: &Arc<Store>,
    server: &MockServer,
    http: &Http,
    interaction: Interaction,
) -> Value {
    Box::pin(handle_interaction(
        Arc::clone(store),
        http,
        UserId(99),
        interaction,
    ))
    .await;
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

fn control(panel: &Value, label: &str) -> String {
    panel["components"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row["components"].as_array().unwrap())
        .find(|item| item["label"] == label || item["placeholder"] == label)
        .unwrap()["custom_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[googletest::test]
#[tokio::test]
async fn removal_gateway_navigates_without_mutation_then_rechecks_permission_and_recovers_receipt()
{
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    seed(&store).await;
    sqlx::query("INSERT INTO prediction_announcement_settings(guild_id,channel_id,enabled,configuration_version) VALUES ('1','20',TRUE,1)").execute(&store.pool).await.unwrap();
    let (server, http) = discord().await;
    let manager = Actor {
        moderator: true,
        ..player(9)
    };
    let before = store.view(GuildId(1)).await.unwrap().revision;
    let markets = send(&store, &server, &http, slash(501, &[], manager)).await;
    let assignments = send(
        &store,
        &server,
        &http,
        component(
            502,
            &control(&markets, "Choose a market"),
            &[MARKET],
            manager,
            GuildId(1),
        ),
    )
    .await;
    assert_that!(
        assignments["content"].as_str().unwrap(),
        contains_substring("Departed members remain removable")
    );
    let confirmation = send(
        &store,
        &server,
        &http,
        component(
            503,
            &control(&assignments, "Choose an existing assignment"),
            &["8"],
            manager,
            GuildId(1),
        ),
    )
    .await;
    assert_that!(
        confirmation["content"].as_str().unwrap(),
        contains_substring("Remove <@8>")
    );
    assert_that!(
        confirmation["content"].as_str().unwrap(),
        contains_substring(MARKET)
    );
    assert_that!(store.view(GuildId(1)).await.unwrap().revision, eq(before));
    let confirm = control(&confirmation, "Confirm remove resolver");
    let rejected = send(
        &store,
        &server,
        &http,
        component(504, &confirm, &[], player(9), GuildId(1)),
    )
    .await;
    assert_that!(
        rejected["content"].as_str().unwrap(),
        contains_substring("creator or moderator")
    );
    assert_that!(store.view(GuildId(1)).await.unwrap().revision, eq(before));
    let committed = send(
        &store,
        &server,
        &http,
        component(504, &confirm, &[], manager, GuildId(1)),
    )
    .await;
    assert_that!(
        committed["content"].as_str().unwrap(),
        contains_substring("Removed")
    );
    assert_that!(
        store.view(GuildId(1)).await.unwrap().state.markets[MARKET].resolvers,
        is_empty()
    );
    let announcements: i64 =
        sqlx::query_scalar("SELECT count(*) FROM prediction_announcement_outbox")
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_that!(announcements, eq(0));
    store
        .execute_at(
            GuildId(1),
            "discord:cancel",
            manager,
            &Command::Cancel { id: MARKET.into() },
            1100,
        )
        .await
        .unwrap();
    let final_revision = store.view(GuildId(1)).await.unwrap().revision;
    let recovered = send(
        &store,
        &server,
        &http,
        component(504, &confirm, &[], player(9), GuildId(1)),
    )
    .await;
    assert_that!(recovered["content"], eq(&committed["content"]));
    assert_that!(
        store.view(GuildId(1)).await.unwrap().revision,
        eq(final_revision)
    );
    {
        let observations = recorder.0.lock().unwrap();
        assert_that!(observations.iter().filter(|event| matches!(event, AuditEvent::CommandCompleted { key: Some(key), .. } if key == "discord:504")).count(), eq(3));
        assert_that!(
            observations.iter().any(|event| matches!(
                event,
                AuditEvent::QueryCompleted {
                    query: QueryKind::Component,
                    outcome: Outcome::Succeeded,
                    ..
                }
            )),
            eq(true)
        );
    }
    let requests = server.received_requests().await.unwrap();
    assert_that!(
        requests
            .iter()
            .any(|request| request.method.as_str() == "GET"),
        eq(false)
    );
    assert_that!(
        requests[0].body_json::<Value>().unwrap()["data"]["flags"],
        eq(64)
    );
    for request in requests
        .iter()
        .filter(|request| request.method.as_str() == "PATCH")
    {
        assert_that!(
            request.body_json::<Value>().unwrap()["allowed_mentions"]["parse"],
            eq(&json!([]))
        );
    }
}

#[googletest::test]
#[tokio::test]
async fn removal_paginates_markets_and_departed_assignments_and_preserves_back_selections() {
    let (_container, store) = fixture().await;
    seed(&store).await;
    for number in 1..=27 {
        let id = uuid::Uuid::from_u128(number).to_string();
        let creator = if number == 27 { player(8) } else { player(7) };
        store
            .execute_at(
                GuildId(1),
                &format!("discord:market{number}"),
                creator,
                &create(&id),
                1000,
            )
            .await
            .unwrap();
        if number == 26 {
            store
                .execute_at(
                    GuildId(1),
                    "discord:terminal",
                    Actor {
                        moderator: true,
                        ..player(7)
                    },
                    &Command::Cancel { id: id.into() },
                    1100,
                )
                .await
                .unwrap();
        }
    }
    for user in 100..=125 {
        store
            .execute_at(
                GuildId(1),
                &format!("discord:join{user}"),
                player(user),
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
        store
            .execute_with_membership_at(
                GuildId(1),
                &format!("discord:add{user}"),
                player(7),
                &Command::AddResolver {
                    id: MARKET.into(),
                    user_id: UserId(user),
                },
                1000,
                async {
                    MembershipEvidence::Present {
                        user_id: UserId(user),
                        bot: false,
                    }
                },
            )
            .await
            .unwrap();
    }
    let before = store.view(GuildId(1)).await.unwrap().revision;
    let (server, http) = discord().await;
    let first = send(&store, &server, &http, slash(701, &[], player(7))).await;
    assert_that!(
        first["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(25)
    );
    let second = send(
        &store,
        &server,
        &http,
        component(
            702,
            &control(&first, "Next markets"),
            &[],
            player(7),
            GuildId(1),
        ),
    )
    .await;
    assert_that!(
        second["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
    assert_that!(
        second["components"][0]["components"][0]["options"][0]["value"],
        eq(MARKET)
    );
    let assignments = send(
        &store,
        &server,
        &http,
        component(
            703,
            &control(&second, "Choose a market"),
            &[MARKET],
            player(7),
            GuildId(1),
        ),
    )
    .await;
    assert_that!(
        assignments["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(25)
    );
    let last_assignments = send(
        &store,
        &server,
        &http,
        component(
            704,
            &control(&assignments, "Next assignments"),
            &[],
            player(7),
            GuildId(1),
        ),
    )
    .await;
    assert_that!(
        last_assignments["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(2)
    );
    let confirmation = send(
        &store,
        &server,
        &http,
        component(
            705,
            &control(&last_assignments, "Choose an existing assignment"),
            &["125"],
            player(7),
            GuildId(1),
        ),
    )
    .await;
    let back = send(
        &store,
        &server,
        &http,
        component(
            706,
            &control(&confirmation, "Back to assignments"),
            &[],
            player(7),
            GuildId(1),
        ),
    )
    .await;
    assert_that!(
        back["content"].as_str().unwrap(),
        contains_substring("Page 2 of 2")
    );
    assert_that!(
        back["components"][0]["components"][0]["options"][1]["default"],
        eq(true)
    );
    let back_markets = send(
        &store,
        &server,
        &http,
        component(
            707,
            &control(&back, "Back to markets"),
            &[],
            player(7),
            GuildId(1),
        ),
    )
    .await;
    assert_that!(
        back_markets["content"].as_str().unwrap(),
        contains_substring("Page 2 of 2")
    );
    assert_that!(
        back_markets["content"].as_str().unwrap(),
        contains_substring("<@125>")
    );
    assert_that!(
        back_markets["components"][0]["components"][0]["options"][0]["default"],
        eq(true)
    );
    let restored = send(
        &store,
        &server,
        &http,
        component(
            708,
            &control(&back_markets, "Choose a market"),
            &[MARKET],
            player(7),
            GuildId(1),
        ),
    )
    .await;
    assert_that!(
        restored["content"].as_str().unwrap(),
        contains_substring("Remove <@125>")
    );
    assert_that!(store.view(GuildId(1)).await.unwrap().revision, eq(before));
    assert_that!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.method.as_str() == "GET"),
        eq(false)
    );
}

fn shortcuts(market: bool, user: Option<u64>) -> Vec<Value> {
    let mut options = vec![];
    if market {
        options.push(json!({"name":"market","type":3,"value":MARKET}));
    }
    if let Some(user) = user {
        options.push(json!({"name":"user","type":6,"value":user.to_string()}));
    }
    options
}

#[googletest::test]
#[tokio::test]
async fn removal_shortcuts_require_confirmation_and_stale_assignment_becomes_receipted_noop() {
    let (_container, store) = fixture().await;
    seed(&store).await;
    let (server, http) = discord().await;
    let before = store.view(GuildId(1)).await.unwrap().revision;
    let mut confirmation = Value::Null;
    for (index, (market, user)) in [(false, false), (true, false), (false, true), (true, true)]
        .into_iter()
        .enumerate()
    {
        let sequence = 801 + index as u64 * 10;
        let mut panel = send(
            &store,
            &server,
            &http,
            slash(sequence, &shortcuts(market, user.then_some(8)), player(7)),
        )
        .await;
        if !market {
            panel = send(
                &store,
                &server,
                &http,
                component(
                    sequence + 1,
                    &control(&panel, "Choose a market"),
                    &[MARKET],
                    player(7),
                    GuildId(1),
                ),
            )
            .await;
        }
        if !user {
            panel = send(
                &store,
                &server,
                &http,
                component(
                    sequence + 2,
                    &control(&panel, "Choose an existing assignment"),
                    &["8"],
                    player(7),
                    GuildId(1),
                ),
            )
            .await;
        }
        assert_that!(
            panel["content"].as_str().unwrap(),
            contains_substring("Remove <@8>")
        );
        assert_that!(store.view(GuildId(1)).await.unwrap().revision, eq(before));
        confirmation = panel;
    }
    store
        .execute_at(
            GuildId(1),
            "discord:concurrent-remove",
            player(7),
            &Command::RemoveResolver {
                id: MARKET.into(),
                user_id: UserId(8),
            },
            1100,
        )
        .await
        .unwrap();
    let after = store.view(GuildId(1)).await.unwrap().revision;
    let confirm = control(&confirmation, "Confirm remove resolver");
    let noop = send(
        &store,
        &server,
        &http,
        component(851, &confirm, &[], player(7), GuildId(1)),
    )
    .await;
    assert_that!(
        noop["content"].as_str().unwrap(),
        contains_substring("not an additional resolver")
    );
    assert_that!(store.view(GuildId(1)).await.unwrap().revision, eq(after));
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM prediction_commands WHERE guild_id='1' AND command_key='discord:851'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_that!(receipts, eq(1));
    store
        .execute_at(
            GuildId(1),
            "discord:cancel",
            Actor {
                moderator: true,
                ..player(7)
            },
            &Command::Cancel { id: MARKET.into() },
            1100,
        )
        .await
        .unwrap();
    let terminal = send(
        &store,
        &server,
        &http,
        component(852, &confirm, &[], player(7), GuildId(1)),
    )
    .await;
    assert_that!(
        terminal["content"].as_str().unwrap(),
        contains_substring("already terminal")
    );
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM prediction_commands WHERE command_key='discord:852'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_that!(receipts, eq(0));
    assert_that!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.method.as_str() == "GET"),
        eq(false)
    );
}

#[googletest::test]
#[tokio::test]
async fn removal_controls_are_scoped_and_manager_self_removal_retains_implicit_authority() {
    let (_container, store) = fixture().await;
    seed(&store).await;
    store
        .execute_with_membership_at(
            GuildId(1),
            "discord:add-self",
            player(7),
            &Command::AddResolver {
                id: MARKET.into(),
                user_id: UserId(7),
            },
            1000,
            async {
                MembershipEvidence::Present {
                    user_id: UserId(7),
                    bot: false,
                }
            },
        )
        .await
        .unwrap();
    let (server, http) = discord().await;
    let denied = send(
        &store,
        &server,
        &http,
        slash(901, &shortcuts(true, Some(8)), player(8)),
    )
    .await;
    assert_that!(
        denied["content"].as_str().unwrap(),
        contains_substring("no longer have permission")
    );
    let confirmation = send(
        &store,
        &server,
        &http,
        slash(902, &shortcuts(true, Some(7)), player(7)),
    )
    .await;
    let confirm = control(&confirmation, "Confirm remove resolver");
    let before = store.view(GuildId(1)).await.unwrap().revision;
    for (id, actor, guild) in [(903, player(8), GuildId(1)), (904, player(7), GuildId(2))] {
        let denied = send(
            &store,
            &server,
            &http,
            component(id, &confirm, &[], actor, guild),
        )
        .await;
        assert_that!(
            denied["content"].as_str().unwrap(),
            contains_substring("belongs to another member or server")
        );
    }
    assert_that!(store.view(GuildId(1)).await.unwrap().revision, eq(before));
    assert_that!(store.view(GuildId(2)).await.unwrap().revision.0, eq(0));
    let removed = send(
        &store,
        &server,
        &http,
        component(905, &confirm, &[], player(7), GuildId(1)),
    )
    .await;
    assert_that!(
        removed["content"].as_str().unwrap(),
        contains_substring(
            "Separate authority as the market creator or a current moderator remains unchanged"
        )
    );
    let still_manageable = send(
        &store,
        &server,
        &http,
        slash(906, &shortcuts(true, Some(8)), player(7)),
    )
    .await;
    assert_that!(
        still_manageable["content"].as_str().unwrap(),
        contains_substring("Remove <@8>")
    );
    let view = store.view(GuildId(1)).await.unwrap();
    assert_that!(
        view.state.markets[MARKET].resolvers.contains(&UserId(7)),
        eq(false)
    );
    assert_that!(
        view.state.markets[MARKET].resolvers.contains(&UserId(8)),
        eq(true)
    );
    let requests = server.received_requests().await.unwrap();
    assert_that!(
        requests
            .iter()
            .any(|request| request.method.as_str() == "GET"),
        eq(false)
    );
    assert_that!(
        requests
            .iter()
            .filter(|request| request.method.as_str() == "POST")
            .all(|request| request.url.path().contains("/interactions/")),
        eq(true)
    );
}
