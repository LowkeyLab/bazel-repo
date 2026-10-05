use super::{create, fixture, fixture_with_audit, player, recording_fixture};
use googletest::{
    assert_that,
    matchers::{contains_substring, eq},
};
use prediction_bot::discord::handle_interaction;
use prediction_bot::{
    announcements::ConfigurationChange,
    audit::{AuditEvent, Outcome, Rejection, Stage},
    domain::{Actor, Command},
    types::{ChannelId, UserId},
};
use serde_json::{Value, json};
use serenity::all::Interaction;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const MARKET: &str = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";

fn interaction(id: u64, data: &Value) -> Value {
    json!({"id":id.to_string(),"application_id":"42","guild_id":"1","channel_id":"20",
        "token":"test-token","version":1,"locale":"en-US","entitlements":[],"attachment_size_limit":1000,
        "data":data,"user":{"id":"1","username":"manager","discriminator":"0","avatar":null},
        "message":serenity::all::Message::default()})
}
fn slash(id: u64, options: &Value) -> Interaction {
    Interaction::Command(serde_json::from_value(interaction(id, &json!({"id":"42","name":"market","type":1,
        "options":[{"name":"resolver","type":2,"options":[{"name":"add","type":1,"options":options}]}]}))).unwrap())
}
fn component(id: u64, custom_id: &str, kind: u8, values: &Value) -> Interaction {
    Interaction::Component(
        serde_json::from_value(interaction(
            id,
            &json!({"custom_id":custom_id,"component_type":kind,"values":values}),
        ))
        .unwrap(),
    )
}
async fn panel(server: &MockServer) -> Value {
    let requests = server.received_requests().await.unwrap();
    serde_json::from_slice(
        &requests
            .iter()
            .rev()
            .find(|request| request.method == "PATCH")
            .unwrap()
            .body,
    )
    .unwrap()
}
fn control(panel: &Value, row: usize, index: usize) -> &str {
    panel["components"][row]["components"][index]["custom_id"]
        .as_str()
        .unwrap()
}
async fn server() -> (MockServer, serenity::http::Http) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let mut message = serenity::all::Message::default();
    message.id = 99.into();
    message.channel_id = 20.into();
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

#[googletest::test]
#[tokio::test]
async fn guided_add_confirms_only_at_execution_and_recovers_after_delivery_failure() {
    use prediction_bot::audit::{CommandKind, FailureCategory};
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    store
        .execute_at(1.into(), "discord:join", player(1), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(1.into(), "discord:create", player(1), &create(MARKET), 1000)
        .await
        .unwrap();
    store
        .execute_at(1.into(), "discord:join2", player(2), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .configure_announcements(
            1.into(),
            "discord:configuration",
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
    let before = store.view(1.into()).await.unwrap().revision;
    let (server, http) = server().await;
    handle_interaction(store.clone(), &http, UserId(99), slash(700, &json!([]))).await;
    let markets = panel(&server).await;
    assert_that!(
        markets["content"].as_str().unwrap(),
        contains_substring("Choose a market")
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(701, control(&markets, 0, 0), 3, &json!([MARKET])),
    )
    .await;
    let people = panel(&server).await;
    assert_that!(
        people["components"][0]["components"][0]["type"].as_u64(),
        eq(Some(5))
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(702, control(&people, 0, 0), 5, &json!(["2"])),
    )
    .await;
    let confirmation = panel(&server).await;
    assert_that!(
        confirmation["content"].as_str().unwrap(),
        contains_substring("Add <@2>")
    );
    assert_that!(
        confirmation["content"].as_str().unwrap(),
        contains_substring(MARKET)
    );
    assert_that!(store.view(1.into()).await.unwrap().revision, eq(before));
    let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM prediction_commands WHERE command_key IN ('discord:700', 'discord:701', 'discord:702')").fetch_one(&store.pool).await.unwrap();
    assert_that!(receipts, eq(0));
    assert_that!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.method == "GET"),
        eq(false)
    );

    let confirm = button(&confirmation, "Confirm add resolver");
    let unavailable = Mock::given(method("GET"))
        .and(path("/api/v10/guilds/1/members/2"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50013,"message":"missing permission"})),
        )
        .mount_as_scoped(&server)
        .await;
    recorder.0.lock().unwrap().clear();
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(703, confirm, 2, &json!([])),
    )
    .await;
    assert_that!(
        panel(&server).await["content"].as_str().unwrap(),
        contains_substring("try again")
    );
    assert_that!(store.view(1.into()).await.unwrap().revision, eq(before));
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM prediction_commands WHERE command_key = 'discord:703'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_that!(receipts, eq(0));
    assert_that!(recorder.0.lock().unwrap().iter().any(|event| matches!(event, AuditEvent::CommandCompleted { command:CommandKind::AddResolver, stage:Stage::Validate, outcome:Outcome::Failed(failure), .. } if failure.category == FailureCategory::Discord)), eq(true));
    drop(unavailable);

    let membership = Mock::given(method("GET")).and(path("/api/v10/guilds/1/members/2")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"user":{"id":"2","username":"resolver","discriminator":"0","avatar":null},"roles":[],"joined_at":"2020-01-01T00:00:00Z","deaf":false,"mute":false,"flags":0}))).expect(1).mount_as_scoped(&server).await;
    let lost = Mock::given(method("PATCH"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50013,"message":"lost response"})),
        )
        .with_priority(1)
        .mount_as_scoped(&server)
        .await;
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(703, confirm, 2, &json!([])),
    )
    .await;
    let original = panel(&server).await;
    assert_that!(
        original["content"].as_str().unwrap(),
        contains_substring("Added")
    );
    let committed = store.view(1.into()).await.unwrap();
    assert_that!(committed.revision, eq(before.next().unwrap()));
    assert_that!(
        committed.state.markets[MARKET]
            .resolvers
            .contains(&UserId(2)),
        eq(true)
    );
    assert_that!(
        recorder.0.lock().unwrap().iter().any(|event| matches!(
            event,
            AuditEvent::InteractionCompleted {
                interaction_id: 703,
                stage: Stage::Deliver,
                outcome: Outcome::Failed(_),
                ..
            }
        )),
        eq(true)
    );
    drop(lost);
    drop(membership);

    let jobs_after: i64 = sqlx::query_scalar("SELECT count(*) FROM prediction_announcement_outbox")
        .fetch_one(&store.pool)
        .await
        .unwrap();
    assert_that!(jobs_after, eq(jobs_before));
    store
        .execute_at(
            1.into(),
            "discord:cancel",
            Actor {
                moderator: true,
                ..player(1)
            },
            &Command::Cancel { id: MARKET.into() },
            2100,
        )
        .await
        .unwrap();
    let terminal = store.view(1.into()).await.unwrap().revision;
    let no_verification = Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403))
        .expect(0)
        .mount_as_scoped(&server)
        .await;
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(703, confirm, 2, &json!([])),
    )
    .await;
    assert_that!(panel(&server).await, eq(&original));
    assert_that!(store.view(1.into()).await.unwrap().revision, eq(terminal));
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(704, control(&people, 0, 0), 5, &json!(["2"])),
    )
    .await;
    assert_that!(
        panel(&server).await["content"].as_str().unwrap(),
        contains_substring("completed")
    );
    drop(no_verification);
    {
        let events = recorder.0.lock().unwrap();
        assert_that!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    AuditEvent::CommandCompleted {
                        command: CommandKind::AddResolver,
                        outcome: Outcome::Succeeded,
                        ..
                    }
                ))
                .count(),
            eq(2)
        );
    }
    let requests = server.received_requests().await.unwrap();
    for request in requests.iter().filter(|request| request.method == "POST") {
        assert_that!(
            request.url.path(),
            googletest::matchers::starts_with("/api/v10/interactions/")
        );
    }
    let initial: Value = serde_json::from_slice(
        &requests
            .iter()
            .find(|request| request.method == "POST")
            .unwrap()
            .body,
    )
    .unwrap();
    assert_that!(initial["data"]["flags"].as_u64(), eq(Some(64)));
    assert_that!(
        original["allowed_mentions"]["parse"]
            .as_array()
            .unwrap()
            .len(),
        eq(0)
    );
}

fn button<'a>(panel: &'a Value, label: &str) -> &'a str {
    panel["components"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row["components"].as_array().unwrap())
        .find(|control| control["label"] == label)
        .unwrap()["custom_id"]
        .as_str()
        .unwrap()
}

#[googletest::test]
#[tokio::test]
async fn guided_add_shortcuts_paginate_and_keep_choices_on_back() {
    let (_container, store) = fixture().await;
    for user in [1, 2] {
        store
            .execute_at(
                1.into(),
                &format!("discord:join{user}"),
                player(user),
                &Command::Join,
                1000,
            )
            .await
            .unwrap();
    }
    let mut ids = vec![];
    for i in 0..27 {
        let id = format!("00000000-0000-4000-8000-{i:012}");
        store
            .execute_at(
                1.into(),
                &format!("discord:create{i}"),
                player(1),
                &create(&id),
                1000,
            )
            .await
            .unwrap();
        ids.push(id);
    }
    store
        .execute_at(1.into(), "discord:other", player(2), &create(MARKET), 1000)
        .await
        .unwrap();
    let before = store.view(1.into()).await.unwrap().revision;
    let (server, http) = server().await;
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        slash(710, &json!([{"name":"user","type":6,"value":"2"}])),
    )
    .await;
    let first = panel(&server).await;
    assert_that!(
        first["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(25)
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(711, button(&first, "Next"), 2, &json!([])),
    )
    .await;
    let last = panel(&server).await;
    assert_that!(
        last["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(2)
    );
    assert_that!(
        last["content"].as_str().unwrap(),
        contains_substring("Page 2 of 2")
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(712, control(&last, 0, 0), 3, &json!([ids[26]])),
    )
    .await;
    let confirmation = panel(&server).await;
    assert_that!(
        confirmation["content"].as_str().unwrap(),
        contains_substring("Add <@2>")
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(713, button(&confirmation, "Back"), 2, &json!([])),
    )
    .await;
    let people = panel(&server).await;
    assert_that!(
        people["components"][0]["components"][0]["type"].as_u64(),
        eq(Some(5))
    );
    assert_that!(
        people["components"][0]["components"][0]["default_values"][0]["id"].as_u64(),
        eq(Some(2))
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(714, button(&people, "Back to markets"), 2, &json!([])),
    )
    .await;
    let retained = panel(&server).await;
    assert_that!(
        retained["content"].as_str().unwrap(),
        contains_substring("Page 2 of 2")
    );
    assert_that!(
        retained["components"][0]["components"][0]["options"][1]["default"].as_bool(),
        eq(Some(true))
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(715, control(&retained, 0, 0), 3, &json!([ids[25]])),
    )
    .await;
    assert_that!(
        panel(&server).await["content"].as_str().unwrap(),
        contains_substring("Add <@2>")
    );
    assert_that!(store.view(1.into()).await.unwrap().revision, eq(before));

    // Terminality changes between pages: clamp the old last-page control to reachable choices.
    for i in [25, 26] {
        store
            .execute_at(
                1.into(),
                &format!("discord:cancel{i}"),
                Actor {
                    moderator: true,
                    ..player(1)
                },
                &Command::Cancel {
                    id: ids[i].clone().into(),
                },
                2100,
            )
            .await
            .unwrap();
    }
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(716, button(&first, "Next"), 2, &json!([])),
    )
    .await;
    let refreshed = panel(&server).await;
    assert_that!(
        refreshed["content"].as_str().unwrap(),
        contains_substring("Page 1 of 1")
    );
    assert_that!(
        refreshed["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(25)
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(717, control(&last, 0, 0), 3, &json!([ids[26]])),
    )
    .await;
    assert_that!(
        panel(&server).await["content"].as_str().unwrap(),
        contains_substring("completed")
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        slash(718, &json!([{"name":"market","type":3,"value":ids[0]}])),
    )
    .await;
    assert_that!(
        panel(&server).await["components"][0]["components"][0]["type"].as_u64(),
        eq(Some(5))
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        slash(
            719,
            &json!([{"name":"market","type":3,"value":ids[0]},{"name":"user","type":6,"value":"2"}]),
        ),
    )
    .await;
    assert_that!(
        panel(&server).await["content"].as_str().unwrap(),
        contains_substring("Add <@2>")
    );
}

#[googletest::test]
#[tokio::test]
async fn guided_add_rejects_foreign_invalid_and_stale_controls_privately() {
    let (audit, recorder) = recording_fixture();
    let (_container, store) = fixture_with_audit(audit).await;
    store
        .execute_at(1.into(), "discord:join", player(2), &Command::Join, 1000)
        .await
        .unwrap();
    store
        .execute_at(1.into(), "discord:create", player(2), &create(MARKET), 1000)
        .await
        .unwrap();
    let before = store.view(1.into()).await.unwrap().revision;
    let (server, http) = server().await;
    let mut opening = slash(
        720,
        &json!([{"name":"market","type":3,"value":MARKET},{"name":"user","type":6,"value":"2"}]),
    );
    if let Interaction::Command(command) = &mut opening {
        let mut member = serenity::all::Member::default();
        member.user = command.user.clone();
        member.guild_id = 1.into();
        member.permissions = Some(serenity::all::Permissions::MANAGE_GUILD);
        command.member = Some(Box::new(member));
    }
    handle_interaction(store.clone(), &http, UserId(99), opening).await;
    let confirmation = panel(&server).await;
    // A new interaction has lost the management role used to open the prompt.
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(721, button(&confirmation, "Back"), 2, &json!([])),
    )
    .await;
    assert_that!(
        panel(&server).await["content"].as_str().unwrap(),
        contains_substring("no longer have permission")
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(
            722,
            button(&confirmation, "Confirm add resolver"),
            2,
            &json!([]),
        ),
    )
    .await;
    assert_that!(
        panel(&server).await["content"].as_str().unwrap(),
        contains_substring("creator or moderator required")
    );
    for (index, (id, kind, values, message)) in [
        (
            "pm:1:9:m:0",
            3,
            &json!([MARKET]),
            "another member or server",
        ),
        (
            "pm:9:1:m:0",
            3,
            &json!([MARKET]),
            "another member or server",
        ),
        ("pm:1:1:m:0", 2, &json!([]), "invalid"),
        ("pm:1:1:m:0", 3, &json!([]), "exactly one market"),
        (
            "pm:1:1:m:0",
            3,
            &json!([MARKET, MARKET]),
            "exactly one market",
        ),
        ("pm:1:1:p:invalid:0", 2, &json!([]), "invalid"),
        ("pm:1:1:m:invalid", 3, &json!([MARKET]), "valid person"),
        ("pm:1:1:u:invalid", 5, &json!(["2"]), "Invalid market ID"),
        (
            "pm:1:1:u:78e829544c674e0d8c808ab95a527ae5",
            5,
            &json!([]),
            "exactly one person",
        ),
        (
            "pm:1:1:u:78e829544c674e0d8c808ab95a527ae5",
            5,
            &json!(["2", "3"]),
            "exactly one person",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        handle_interaction(
            store.clone(),
            &http,
            UserId(99),
            component(730 + index as u64, id, kind, values),
        )
        .await;
        assert_that!(
            panel(&server).await["content"].as_str().unwrap(),
            contains_substring(message)
        );
        let requests = server.received_requests().await.unwrap();
        let ack: Value = serde_json::from_slice(
            &requests
                .iter()
                .rev()
                .find(|request| request.method == "POST")
                .unwrap()
                .body,
        )
        .unwrap();
        assert_that!(ack["data"]["flags"].as_u64(), eq(Some(64)));
    }
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        component(
            741,
            "pm:1:1:m:0",
            3,
            &json!(["00000000-0000-4000-8000-000000000099"]),
        ),
    )
    .await;
    assert_that!(
        panel(&server).await["content"].as_str().unwrap(),
        contains_substring("no longer exists")
    );
    handle_interaction(store.clone(), &http, UserId(99), slash(742, &json!([]))).await;
    assert_that!(
        panel(&server).await["content"].as_str().unwrap(),
        contains_substring("No markets")
    );
    assert_that!(store.view(1.into()).await.unwrap().revision, eq(before));
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM prediction_commands WHERE command_key ~ '^discord:7[0-9]+$'",
    )
    .fetch_one(&store.pool)
    .await
    .unwrap();
    assert_that!(receipts, eq(0));
    assert_that!(
        recorder.0.lock().unwrap().iter().any(|event| matches!(
            event,
            AuditEvent::CommandCompleted {
                outcome: Outcome::Rejected(Rejection::PermissionDenied),
                ..
            }
        )),
        eq(true)
    );
}

fn maximum_scope(mut interaction: Interaction) -> Interaction {
    match &mut interaction {
        Interaction::Command(command) => {
            command.guild_id = Some(u64::MAX.into());
            command.user.id = u64::MAX.into();
        }
        Interaction::Component(component) => {
            component.guild_id = Some(u64::MAX.into());
            component.user.id = u64::MAX.into();
        }
        _ => unreachable!(),
    }
    interaction
}

#[googletest::test]
#[tokio::test]
async fn guided_add_maximum_ids_preserve_historical_uuid_spelling_through_back() {
    let (_container, store) = fixture().await;
    let id = "78E82954-4C67-4E0D-8C80-8AB95A527AE5";
    store
        .execute_at(
            u64::MAX.into(),
            "discord:join",
            player(u64::MAX),
            &Command::Join,
            1000,
        )
        .await
        .unwrap();
    store
        .execute_at(
            u64::MAX.into(),
            "discord:create",
            player(u64::MAX),
            &create(id),
            1000,
        )
        .await
        .unwrap();
    let (server, http) = server().await;
    handle_interaction(store.clone(),&http,UserId(99),maximum_scope(slash(750,&json!([{"name":"market","type":3,"value":id},{"name":"user","type":6,"value":u64::MAX.to_string()}])))).await;
    let confirmation = panel(&server).await;
    assert_that!(button(&confirmation, "Confirm add resolver").len(), eq(100));
    assert_that!(button(&confirmation, "Back").len(), eq(100));
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        maximum_scope(component(751, button(&confirmation, "Back"), 2, &json!([]))),
    )
    .await;
    let people = panel(&server).await;
    assert_that!(
        people["components"][0]["components"][0]["default_values"][0]["id"].as_u64(),
        eq(Some(u64::MAX))
    );
    assert_that!(button(&people, "Back to markets").len(), eq(100));
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        maximum_scope(component(
            752,
            button(&people, "Back to markets"),
            2,
            &json!([]),
        )),
    )
    .await;
    let markets = panel(&server).await;
    assert_that!(
        markets["components"][0]["components"][0]["options"][0]["value"].as_str(),
        eq(Some(id))
    );
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        maximum_scope(component(753, control(&markets, 0, 0), 3, &json!([id]))),
    )
    .await;
    assert_that!(
        panel(&server).await["content"].as_str().unwrap(),
        contains_substring(id)
    );
}
