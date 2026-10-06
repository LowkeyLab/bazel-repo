use super::{
    MARKET, button, denied, fixture, fresh, listing_http, listing_interaction, listing_response,
    persisted, player, scoped, seed, slash, slash_for,
};
use googletest::{
    assert_that,
    matchers::{contains_substring, eq},
};
use prediction_bot::{
    discord::handle_interaction,
    domain::Command,
    types::{OutcomeIndex, UserId},
};
use serde_json::{Value, json};
use serenity::all::Interaction;

fn selection(id: u64, control: &str, values: Value, guild: u64, user: u64) -> Interaction {
    let mut value = listing_interaction(
        id,
        &json!({"custom_id":control,"component_type":3,"values":values}),
    );
    scoped(&mut value, guild, user);
    Interaction::Component(serde_json::from_value(value).unwrap())
}
fn select_menu(panel: &Value) -> &Value {
    panel["components"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row["components"].as_array().unwrap())
        .find(|item| item["type"] == 3)
        .unwrap()
}

#[googletest::test]
#[tokio::test]
async fn positions_picker_and_direct_entry_show_identical_historical_results_after_restart() {
    let (_container, store) = fixture().await;
    seed(&store, MARKET, &[(2, 0, 1), (3, 0, 2), (4, 1, 2)]).await;
    sqlx::query("UPDATE prediction_events SET event=jsonb_set(event,'{data,payouts}',$1) WHERE command_key=$2")
        .bind(json!([{"user_id":"2","amount":3},{"user_id":"3","amount":2}]))
        .bind(format!("discord:resolve:{MARKET}")).execute(&store.pool).await.unwrap();
    let reader = fresh(&store);
    let saved = persisted(&store).await;
    let before = reader.view(1.into()).await.unwrap();
    let (server, http) = listing_http().await;
    handle_interaction(
        reader.clone(),
        &http,
        UserId(99),
        slash_for(1000, Some(1), 99, json!([]), "0"),
    )
    .await;
    let picker = listing_response(&server).await;
    assert_that!(
        picker["content"].as_str().unwrap(),
        contains_substring("Page 1 of 1")
    );
    let menu = select_menu(&picker);
    assert_that!(menu["options"][0]["value"], eq(&json!(MARKET)));
    handle_interaction(
        reader.clone(),
        &http,
        UserId(99),
        selection(
            1001,
            menu["custom_id"].as_str().unwrap(),
            json!([MARKET]),
            1,
            99,
        ),
    )
    .await;
    let selected = listing_response(&server).await;
    assert_that!(
        selected["embeds"][0]["description"].as_str().unwrap(),
        contains_substring("Total stake: 1 · Payout: 3 · Net: +2")
    );
    handle_interaction(fresh(&store), &http, UserId(99), slash(1002, MARKET)).await;
    assert_that!(listing_response(&server).await, eq(&selected));
    assert_that!(persisted(&store).await, eq(&saved));
    let after = reader.view(1.into()).await.unwrap();
    assert_that!(after.state, eq(&before.state));
    assert_that!(after.revision, eq(before.revision));
}

#[googletest::test]
#[tokio::test]
async fn positions_picker_reaches_every_resolved_market_with_safe_labels_and_returns_to_its_page() {
    use prediction_bot::{announcements::ConfigurationChange, domain::Actor, types::GuildId};
    let (_container, store) = fixture().await;
    let guild = GuildId(u64::MAX);
    let reader = u64::MAX;
    for user in [1, reader] {
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
            Actor {
                moderator: true,
                ..player(1)
            },
            ConfigurationChange::Set {
                channel_id: 20.into(),
            },
        )
        .await
        .unwrap();
    let expected: Vec<_> = (0..27)
        .map(|index| format!("00000000-0000-4000-8000-{index:012}"))
        .collect();
    for (index, id) in expected.iter().enumerate() {
        store
            .execute_at(
                guild,
                &format!("discord:create:{id}"),
                player(1),
                &Command::Create {
                    id: id.as_str().into(),
                    question: format!("{index:02}{}", "😀".repeat(198)),
                    options: vec!["Yes".into(), "No".into()],
                    closes_at: 2000,
                },
                1000,
            )
            .await
            .unwrap();
        store
            .execute_at(
                guild,
                &format!("discord:resolve:{id}"),
                player(1),
                &Command::Resolve {
                    id: id.as_str().into(),
                    outcome: OutcomeIndex(0),
                },
                2000,
            )
            .await
            .unwrap();
    }
    for (id, closes_at, cancel) in [
        ("00000000-0000-4000-8001-000000000000", i64::MAX, false),
        ("00000000-0000-4000-8001-000000000001", 2000, false),
        ("00000000-0000-4000-8001-000000000002", 2000, true),
    ] {
        store
            .execute_at(
                guild,
                &format!("discord:create:{id}"),
                player(1),
                &Command::Create {
                    id: id.into(),
                    question: "Ineligible".into(),
                    options: vec!["Yes".into(), "No".into()],
                    closes_at,
                },
                1000,
            )
            .await
            .unwrap();
        if cancel {
            store
                .execute_at(
                    guild,
                    &format!("discord:cancel:{id}"),
                    Actor {
                        moderator: true,
                        ..player(1)
                    },
                    &Command::Cancel { id: id.into() },
                    1500,
                )
                .await
                .unwrap();
        }
    }
    seed(&store, MARKET, &[]).await; // A resolved market in a different guild is also excluded.
    let saved = persisted(&store).await;
    assert_that!(
        saved["announcements"].as_array().unwrap().is_empty(),
        eq(false)
    );
    let before = store.view(guild).await.unwrap();
    let (server, http) = listing_http().await;
    handle_interaction(
        fresh(&store),
        &http,
        UserId(99),
        slash_for(1100, Some(guild.0), reader, json!([]), "0"),
    )
    .await;
    let mut panel = listing_response(&server).await;
    assert_that!(
        panel["content"].as_str().unwrap(),
        contains_substring("Page 1 of 2")
    );
    let first = panel.clone();
    let mut found = vec![];
    let mut page = 0;
    loop {
        page += 1;
        let options = select_menu(&panel)["options"].as_array().unwrap();
        assert_that!(options.len() <= 25, eq(true));
        assert_that!(
            panel["content"].as_str().unwrap(),
            contains_substring(format!("Page {page} of 2"))
        );
        for option in options {
            found.push(option["value"].as_str().unwrap().to_owned());
            assert_that!(
                option["label"].as_str().unwrap().encode_utf16().count() <= 100,
                eq(true)
            );
            assert_that!(option["label"].as_str().unwrap().contains('😀'), eq(true));
        }
        for row in panel["components"].as_array().unwrap() {
            for control in row["components"].as_array().unwrap() {
                assert_that!(
                    control["custom_id"].as_str().unwrap().len() <= 100,
                    eq(true)
                );
            }
        }
        assert_that!(panel["allowed_mentions"]["parse"], eq(&json!([])));
        let Some(next) = button(&panel, "Next markets") else {
            break;
        };
        handle_interaction(
            store.clone(),
            &http,
            UserId(99),
            super::component(1100 + page, &next, 2, guild.0, reader),
        )
        .await;
        panel = listing_response(&server).await;
    }
    assert_that!(found, eq(&expected));
    let last = panel.clone();
    let previous = button(&panel, "Previous markets").unwrap();
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        super::component(1110, &previous, 2, guild.0, reader),
    )
    .await;
    assert_that!(listing_response(&server).await, eq(&first));
    let boundary = format!("pm:{}:{reader}:pp:{}", guild.0, u32::MAX);
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        super::component(1111, &boundary, 2, guild.0, reader),
    )
    .await;
    assert_that!(listing_response(&server).await, eq(&last));
    let chosen = expected.last().unwrap();
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        selection(
            1112,
            select_menu(&last)["custom_id"].as_str().unwrap(),
            json!([chosen]),
            guild.0,
            reader,
        ),
    )
    .await;
    let selected = listing_response(&server).await;
    assert_that!(
        selected["embeds"][0]["description"].as_str().unwrap(),
        contains_substring(chosen.as_str())
    );
    let back = button(&selected, "Back to markets").unwrap();
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        super::component(1113, &back, 2, guild.0, reader),
    )
    .await;
    assert_that!(listing_response(&server).await, eq(&last));
    handle_interaction(
        fresh(&store),
        &http,
        UserId(99),
        slash_for(
            1114,
            Some(guild.0),
            reader,
            json!([{"name":"id","type":3,"value":chosen}]),
            "0",
        ),
    )
    .await;
    assert_that!(listing_response(&server).await, eq(&selected));
    assert_that!(persisted(&store).await, eq(&saved));
    let after = store.view(guild).await.unwrap();
    assert_that!(after.state, eq(&before.state));
    assert_that!(after.revision, eq(before.revision));
    for request in server.received_requests().await.unwrap() {
        if request.method == "POST" {
            let body: Value = request.body_json().unwrap();
            if body["type"] == 5 {
                assert_that!(body["data"]["flags"], eq(&json!(64)));
            } else {
                assert_that!(body["type"], eq(&json!(6)));
            }
        }
        assert_that!(request.method == "GET", eq(false));
    }
}

#[googletest::test]
#[tokio::test]
async fn positions_picker_empty_access_and_stale_controls_are_private_and_read_only() {
    use prediction_bot::domain::Actor;
    let (_container, store) = fixture().await;
    store
        .execute_at(
            1.into(),
            "discord:join:99",
            player(99),
            &Command::Join,
            1000,
        )
        .await
        .unwrap();
    let empty_before = persisted(&store).await;
    let (server, http) = listing_http().await;
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        slash_for(1200, Some(1), 99, json!([]), "0"),
    )
    .await;
    denied(&server, "No resolved markets exist in this server yet.").await;
    assert_that!(persisted(&store).await, eq(&empty_before));
    seed(&store, MARKET, &[]).await;
    let open = "00000000-0000-4000-8000-000000000010";
    let cancelled = "00000000-0000-4000-8000-000000000011";
    let other = "00000000-0000-4000-8000-000000000012";
    for id in [open, cancelled] {
        store
            .execute_at(
                1.into(),
                &format!("discord:create:{id}"),
                player(1),
                &super::super::create(id),
                1000,
            )
            .await
            .unwrap();
    }
    store
        .execute_at(
            1.into(),
            "discord:cancel",
            Actor {
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
    for user in [1, 101] {
        store
            .execute_at(
                2.into(),
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
            2.into(),
            "discord:create",
            player(1),
            &super::super::create(other),
            1000,
        )
        .await
        .unwrap();
    store
        .execute_at(
            2.into(),
            "discord:resolve",
            player(1),
            &Command::Resolve {
                id: other.into(),
                outcome: OutcomeIndex(0),
            },
            2000,
        )
        .await
        .unwrap();
    let saved = persisted(&store).await;
    let before = store.view(1.into()).await.unwrap();
    let other_before = store.view(2.into()).await.unwrap();
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        slash_for(1201, Some(1), 99, json!([]), "0"),
    )
    .await;
    let picker = listing_response(&server).await;
    let control = select_menu(&picker)["custom_id"].as_str().unwrap();
    assert_that!(
        select_menu(&picker)["options"].as_array().unwrap().len(),
        eq(1)
    );
    for (index, user) in [100, 101].into_iter().enumerate() {
        handle_interaction(
            store.clone(),
            &http,
            UserId(99),
            slash_for(1210 + index as u64, Some(1), user, json!([]), "8"),
        )
        .await;
        denied(&server, "You are not enrolled. Use /market join first.").await;
        for interaction in [
            super::component(
                1220 + index as u64,
                &format!("pm:1:{user}:pp:0"),
                2,
                1,
                user,
            ),
            selection(
                1230 + index as u64,
                &format!("pm:1:{user}:pk"),
                json!([MARKET]),
                1,
                user,
            ),
            super::component(
                1240 + index as u64,
                &format!("pm:1:{user}:ps:{MARKET}:0"),
                2,
                1,
                user,
            ),
        ] {
            handle_interaction(store.clone(), &http, UserId(99), interaction).await;
            denied(&server, "You are not enrolled. Use /market join first.").await;
        }
    }
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        slash_for(1250, None, 99, json!([]), "0"),
    )
    .await;
    denied(&server, "server").await;
    for (index, (guild, user)) in [(1, 100), (2, 99)].into_iter().enumerate() {
        handle_interaction(
            store.clone(),
            &http,
            UserId(99),
            selection(1251 + index as u64, control, json!([MARKET]), guild, user),
        )
        .await;
        denied(&server, "belongs to another member or server").await;
        handle_interaction(
            store.clone(),
            &http,
            UserId(99),
            super::component(1253 + index as u64, "pm:1:99:pp:0", 2, guild, user),
        )
        .await;
        denied(&server, "belongs to another member or server").await;
    }
    for (index, (values, reason)) in [
        (json!([]), "Choose exactly one market."),
        (json!([MARKET, MARKET]), "Choose exactly one market."),
        (json!(["unknown"]), "does not exist in this server"),
        (json!([other]), "does not exist in this server"),
        (json!([open]), "only for resolved markets"),
        (json!([cancelled]), "only for resolved markets"),
    ]
    .into_iter()
    .enumerate()
    {
        handle_interaction(
            store.clone(),
            &http,
            UserId(99),
            selection(1260 + index as u64, control, values, 1, 99),
        )
        .await;
        denied(&server, reason).await;
    }
    for (index, (control, kind)) in [
        ("pm:1:99:pp", 2),
        ("pm:1:99:pp:x", 2),
        ("pm:1:99:pp:4294967296", 2),
        ("pm:1:99:pp:0:extra", 2),
        ("pm:1:99:pp:0", 3),
        ("pm:1:99:pk", 2),
        ("pm:1:99:pk:extra", 3),
    ]
    .into_iter()
    .enumerate()
    {
        handle_interaction(
            store.clone(),
            &http,
            UserId(99),
            super::component(1270 + index as u64, control, kind, 1, 99),
        )
        .await;
        denied(&server, "positions control is invalid").await;
    }
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        super::component(1280, &format!("pm:1:99:pp:{}", "0".repeat(101)), 2, 1, 99),
    )
    .await;
    denied(&server, "belongs to another member or server").await;
    handle_interaction(
        store.clone(),
        &http,
        UserId(99),
        selection(1281, control, json!([MARKET]), 1, 99),
    )
    .await;
    assert_that!(
        listing_response(&server).await["embeds"][0]["description"]
            .as_str()
            .unwrap(),
        contains_substring("No bets were placed in this market.")
    );
    handle_interaction(
        fresh(&store),
        &http,
        UserId(99),
        slash_for(1282, Some(2), 101, json!([]), "0"),
    )
    .await;
    let other_picker = listing_response(&server).await;
    assert_that!(
        select_menu(&other_picker)["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
    assert_that!(
        select_menu(&other_picker)["options"][0]["value"],
        eq(&json!(other))
    );
    assert_that!(persisted(&store).await, eq(&saved));
    let after = store.view(1.into()).await.unwrap();
    assert_that!(after.state, eq(&before.state));
    assert_that!(after.revision, eq(before.revision));
    assert_that!(after.state.accounts.contains_key(&UserId(100)), eq(false));
    assert_that!(after.state.accounts.contains_key(&UserId(101)), eq(false));
    let other_after = store.view(2.into()).await.unwrap();
    assert_that!(other_after.state, eq(&other_before.state));
    assert_that!(other_after.revision, eq(other_before.revision));
    for request in server.received_requests().await.unwrap() {
        if request.method == "POST" {
            let body: Value = request.body_json().unwrap();
            if body["type"] == 5 {
                assert_that!(body["data"]["flags"], eq(&json!(64)));
            } else {
                assert_that!(body["type"], eq(&json!(6)));
            }
        }
        assert_that!(request.method == "GET", eq(false));
    }
}

#[googletest::test]
#[tokio::test]
async fn positions_picker_query_and_delivery_failures_are_private_structured_and_read_only() {
    use prediction_bot::{
        audit::{AuditEvent, FailureCategory, Outcome, QueryKind, Stage},
        domain::Policy,
        store::Store,
        types::Points,
    };
    use wiremock::{Mock, ResponseTemplate, matchers::method};
    let (audit, recorder) = super::super::recording_fixture();
    let (_container, store) = super::super::fixture_with_audit(audit).await;
    seed(&store, MARKET, &[]).await;
    let before = store.view(1.into()).await.unwrap();
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
    for (id, query, interaction) in [
        (
            1400,
            QueryKind::Positions,
            slash_for(1400, Some(1), 99, json!([]), "0"),
        ),
        (
            1401,
            QueryKind::Component,
            selection(1401, "pm:1:99:pk", json!([MARKET]), 1, 99),
        ),
        (
            1402,
            QueryKind::Component,
            super::component(1402, "pm:1:99:pp:0", 2, 1, 99),
        ),
    ] {
        handle_interaction(store.clone(), &http, UserId(99), interaction).await;
        let events = recorder.0.lock().unwrap();
        assert_that!(events.iter().any(|event| matches!(event, AuditEvent::QueryCompleted { interaction_id, query: observed, outcome: Outcome::Succeeded, .. } if *interaction_id == id && *observed == query)), eq(true));
        assert_that!(events.iter().any(|event| matches!(event, AuditEvent::InteractionCompleted { interaction_id, stage: Stage::Deliver, outcome: Outcome::Failed(failure), .. } if *interaction_id == id && failure.category == FailureCategory::Discord && failure.http_status == Some(403) && failure.discord_code == Some(50013))), eq(true));
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
    for (id, query, interaction) in [
        (
            1410,
            QueryKind::Positions,
            slash_for(1410, Some(1), 99, json!([]), "0"),
        ),
        (
            1411,
            QueryKind::Component,
            selection(1411, "pm:1:99:pk", json!([MARKET]), 1, 99),
        ),
        (
            1412,
            QueryKind::Component,
            super::component(1412, "pm:1:99:pp:0", 2, 1, 99),
        ),
    ] {
        handle_interaction(store.clone(), &http, UserId(99), interaction).await;
        denied(&server, "temporarily unavailable").await;
        assert_that!(recorder.0.lock().unwrap().iter().any(|event| matches!(event, AuditEvent::QueryCompleted { interaction_id, query: observed, outcome: Outcome::Failed(_), .. } if *interaction_id == id && *observed == query)), eq(true));
    }
    assert_that!(persisted(&witness).await, eq(&saved));
    let after = witness.view(1.into()).await.unwrap();
    assert_that!(after.state, eq(&before.state));
    assert_that!(after.revision, eq(before.revision));
    let (server, http) = listing_http().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"code":50013,"message":"acknowledgement failure"})),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    for (id, interaction) in [
        (1420, slash_for(1420, Some(1), 99, json!([]), "0")),
        (1421, selection(1421, "pm:1:99:pk", json!([MARKET]), 1, 99)),
        (1422, super::component(1422, "pm:1:99:pp:0", 2, 1, 99)),
    ] {
        handle_interaction(store.clone(), &http, UserId(99), interaction).await;
        let events = recorder.0.lock().unwrap();
        assert_that!(events.iter().any(|event| matches!(event, AuditEvent::QueryCompleted { interaction_id, .. } if *interaction_id == id)), eq(false));
        assert_that!(events.iter().any(|event| matches!(event, AuditEvent::InteractionCompleted { interaction_id, stage: Stage::Acknowledge, outcome: Outcome::Failed(failure), .. } if *interaction_id == id && failure.category == FailureCategory::Discord && failure.http_status == Some(403))), eq(true));
    }
    assert_that!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.method == "PATCH"),
        eq(false)
    );
    assert_that!(persisted(&witness).await, eq(&saved));
}
