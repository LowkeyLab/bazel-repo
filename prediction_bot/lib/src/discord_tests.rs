use super::{Action, Input, InputOption, InputValue, parse};
use crate::domain::Command;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn input(subcommand: &str, options: Vec<InputOption>) -> Input {
    Input {
        guild_id: Some(10),
        user_id: 20,
        bot: false,
        moderator: false,
        subcommand: subcommand.to_owned(),
        options,
    }
}

fn text(name: &str, value: &str) -> InputOption {
    InputOption {
        name: name.to_owned(),
        value: InputValue::String(value.to_owned()),
    }
}

fn number(name: &str, value: i64) -> InputOption {
    InputOption {
        name: name.to_owned(),
        value: InputValue::Integer(value),
    }
}

fn channel(name: &str, value: u64) -> InputOption {
    InputOption {
        name: name.to_owned(),
        value: InputValue::Channel(value),
    }
}

#[test]
fn announcement_configuration_requires_server_management() {
    let mut request = input("announcements.disable", vec![]);
    assert!(parse(&request).is_err());
    request.moderator = true;
    assert!(matches!(
        parse(&request),
        Ok((10, _, Action::AnnouncementsDisable))
    ));
}

#[test]
fn announcement_actions_require_exact_typed_options() {
    let mut set = input("announcements.set", vec![channel("channel", 55)]);
    assert!(parse(&set).is_err());
    set.moderator = true;
    assert_eq!(
        parse(&set).unwrap().2,
        Action::AnnouncementsSet { channel_id: 55 }
    );

    for malformed in [
        input("announcements.set", vec![]),
        input("announcements.set", vec![number("channel", 55)]),
        input(
            "announcements.set",
            vec![channel("channel", 55), text("extra", "value")],
        ),
        input("announcements.status", vec![text("extra", "value")]),
        input("announcements.disable", vec![text("extra", "value")]),
    ] {
        let mut malformed = malformed;
        malformed.moderator = true;
        assert!(parse(&malformed).is_err());
    }

    let mut status = input("announcements.status", vec![]);
    status.moderator = true;
    assert!(matches!(
        parse(&status),
        Ok((10, _, Action::AnnouncementsStatus))
    ));

    status.guild_id = None;
    assert!(parse(&status).is_err());
    status.guild_id = Some(10);
    status.bot = true;
    assert!(parse(&status).is_err());
}

#[test]
fn guild_and_bot_metadata_are_enforced() {
    let mut join = input("join", vec![]);
    assert!(matches!(
        parse(&join),
        Ok((10, _, Action::Write(Command::Join)))
    ));
    join.guild_id = None;
    assert_eq!(
        parse(&join),
        Err("This command is available only in a server.")
    );
    join.guild_id = Some(10);
    join.bot = true;
    assert_eq!(parse(&join), Err("Bots cannot use the prediction economy."));
}

#[test]
fn moderator_flag_is_passed_to_resolution_and_cancel() {
    let mut resolve = input("resolve", vec![text("id", "1234"), number("outcome", 2)]);
    resolve.moderator = true;
    let (_, actor, action) = parse(&resolve).unwrap();
    assert!(actor.moderator);
    assert_eq!(
        action,
        Action::Write(Command::Resolve {
            id: "1234".to_owned(),
            outcome: 1
        })
    );
    let cancel = input("cancel", vec![text("id", "1234")]);
    assert!(!parse(&cancel).unwrap().1.moderator);
}

#[test]
fn parses_market_inputs_and_one_based_outcomes() {
    let create = input(
        "create",
        vec![
            text("question", "Will it rain?"),
            text("options", "Yes | No"),
            text("closes_at", "2030-01-02T03:04:05Z"),
        ],
    );
    let (
        _,
        _,
        Action::Write(Command::Create {
            question,
            options,
            closes_at,
            ..
        }),
    ) = parse(&create).unwrap()
    else {
        panic!("expected create command");
    };
    assert_eq!(question, "Will it rain?");
    assert_eq!(options, vec!["Yes", "No"]);
    assert_eq!(closes_at, 1_893_553_445);

    let bet = input(
        "bet",
        vec![
            text("id", "1234"),
            number("outcome", 1),
            number("amount", 25),
        ],
    );
    assert_eq!(
        parse(&bet).unwrap().2,
        Action::Write(Command::Bet {
            id: "1234".to_owned(),
            outcome: 0,
            amount: 25
        })
    );
}

#[test]
fn rejects_malformed_amounts_outcomes_and_times() {
    for bet in [
        input(
            "bet",
            vec![
                text("id", "1234"),
                number("outcome", 0),
                number("amount", 1),
            ],
        ),
        input(
            "bet",
            vec![
                text("id", "1234"),
                number("outcome", -1),
                number("amount", 1),
            ],
        ),
        input(
            "bet",
            vec![
                text("id", "1234"),
                number("outcome", 1),
                number("amount", 0),
            ],
        ),
        input(
            "bet",
            vec![
                text("id", "1234"),
                number("outcome", 1),
                text("amount", "1.5"),
            ],
        ),
    ] {
        assert!(parse(&bet).is_err());
    }
    for create in [
        input(
            "create",
            vec![
                text("question", "Q"),
                text("options", "One"),
                text("closes_at", "2030-01-02T03:04:05Z"),
            ],
        ),
        input(
            "create",
            vec![
                text("question", "Q"),
                text("options", "One|Two"),
                text("closes_at", "tomorrow"),
            ],
        ),
    ] {
        assert!(parse(&create).is_err());
    }
}

#[test]
fn query_outputs_are_scoped_ranked_and_bounded() {
    use super::{render_query, reply, truncate};
    use crate::domain::{Account, Actor, Bet, Market, State, Status};
    use crate::store::View;

    let mut state = State::default();
    state.accounts.insert(
        20,
        Account {
            balance: 75,
            next_grant: 1_900_000_000,
        },
    );
    state.accounts.insert(
        9,
        Account {
            balance: 75,
            next_grant: 1_900_000_000,
        },
    );
    state.accounts.insert(
        99,
        Account {
            balance: 1,
            next_grant: 1_900_000_000,
        },
    );
    state.markets.insert(
        "one".to_owned(),
        Market {
            creator: 20,
            question: "@everyone wins?".to_owned(),
            options: vec!["Yes".to_owned(), "No".to_owned()],
            closes_at: 1_900_000_000,
            created_at: 1_800_000_000,
            status: Status::Open,
            bets: vec![Bet {
                user_id: 20,
                outcome: 0,
                amount: 25,
            }],
            total_staked: 25,
        },
    );
    let view = View { revision: 7, state };
    let actor = Actor {
        user_id: 20,
        moderator: false,
        bot: false,
    };
    let leaderboard = render_query(&view, &Action::Leaderboard, actor, 1_850_000_000);
    assert!(leaderboard.find("User ID 9").unwrap() < leaderboard.find("User ID 20").unwrap());
    assert!(render_query(&view, &Action::Balance, actor, 1_850_000_000).contains("75 points"));
    assert!(
        render_query(
            &view,
            &Action::Show {
                id: "one".to_owned()
            },
            actor,
            1_850_000_000
        )
        .contains("25 points pooled")
    );
    assert!(
        render_query(
            &view,
            &Action::Show {
                id: "one".to_owned()
            },
            actor,
            1_900_000_000
        )
        .contains("Closed; awaiting outcome")
    );
    assert!(render_query(&view, &Action::List, actor, 1_900_000_000).contains("No markets"));
    assert!(
        render_query(
            &view,
            &Action::Show {
                id: "other-server".to_owned()
            },
            actor,
            1_850_000_000
        )
        .contains("in this server")
    );

    let content = truncate(&"🪙".repeat(2_000));
    assert!(content.encode_utf16().count() <= 2_000);
    assert!(content.ends_with('…'));
    let value = serde_json::to_value(reply("@everyone <@123>")).unwrap();
    assert_eq!(value["allowed_mentions"]["parse"], serde_json::json!([]));
}

#[test]
fn storage_failures_do_not_expose_database_details() {
    use super::safe_error;
    use crate::store::StoreError;
    let error = StoreError::Database(sqlx::Error::Configuration("secret database URL".into()));
    assert_eq!(
        safe_error(&error),
        "The prediction economy is temporarily unavailable. Please try again."
    );
}

#[test]
fn list_keeps_ten_ids_when_questions_use_long_emoji_text() {
    use super::{render_query, truncate};
    use crate::domain::{Actor, Market, State, Status};
    use crate::store::View;

    let mut state = State::default();
    for index in 0..10 {
        let id = format!("{index:08x}-0000-0000-0000-000000000000");
        state.markets.insert(
            id,
            Market {
                creator: 20,
                question: "🪙".repeat(200),
                options: vec!["Yes".to_owned(), "No".to_owned()],
                created_at: 1_800_000_000 + index,
                closes_at: 1_900_000_000,
                status: Status::Open,
                bets: vec![],
                total_staked: 0,
            },
        );
    }
    let view = View {
        revision: 10,
        state,
    };
    let actor = Actor {
        user_id: 20,
        moderator: false,
        bot: false,
    };
    let message = render_query(&view, &Action::List, actor, 1_850_000_000);
    assert!(message.encode_utf16().count() <= 2_000);
    let reply = truncate(&message);
    for index in 0..10 {
        let id = format!("{index:08x}-0000-0000-0000-000000000000");
        assert!(reply.contains(&id), "missing market ID {id}");
    }
}

#[tokio::test]
async fn already_acknowledged_defer_still_runs_receipt_lookup_and_edit_content() {
    use super::{content_after_defer, recoverable_defer_code};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let replayed = content_after_defer(recoverable_defer_code(40_060), {
        let calls = Arc::clone(&calls);
        move || async move {
            calls.fetch_add(1, Ordering::SeqCst);
            "Recorded market response".to_owned()
        }
    })
    .await;
    assert_eq!(replayed.as_deref(), Some("Recorded market response"));
    let edit = serde_json::to_value(super::reply(replayed.as_deref().unwrap())).unwrap();
    assert_eq!(edit["content"], "Recorded market response");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let rejected = content_after_defer(recoverable_defer_code(40_061), {
        let calls = Arc::clone(&calls);
        move || async move {
            calls.fetch_add(1, Ordering::SeqCst);
            "Should not run".to_owned()
        }
    })
    .await;
    assert_eq!(rejected, None);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn expected_application_id_must_match_authenticated_identity() {
    use super::verify_application_id;
    assert_eq!(verify_application_id(123, None), Ok(123));
    assert_eq!(verify_application_id(123, Some(123)), Ok(123));
    assert_eq!(
        verify_application_id(123, Some(456)),
        Err("configured application ID does not match Discord token")
    );
    assert!(verify_application_id(0, None).is_err());
}

#[test]
fn creation_can_start_without_typing_slash_command_fields() {
    assert!(parse(&input("create", vec![])).is_ok());
    let registration = serde_json::to_value(super::market_command()).unwrap();
    let create = registration["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["name"] == "create")
        .unwrap();
    assert!(
        create["options"]
            .as_array()
            .unwrap()
            .iter()
            .all(|option| option["required"] != true)
    );
}

fn ui_actor() -> crate::domain::Actor {
    crate::domain::Actor {
        user_id: 20,
        moderator: false,
        bot: false,
    }
}

fn ui_view() -> crate::store::View {
    use crate::domain::{Account, Market, State, Status};
    let mut state = State::default();
    state.accounts.insert(
        20,
        Account {
            balance: 100,
            next_grant: 90_000,
        },
    );
    state.markets.insert(
        "78e82954-4c67-4e0d-8c80-8ab95a527ae5".into(),
        Market {
            creator: 20,
            question: "Who wins?".into(),
            options: vec!["Red".into(), "Blue".into()],
            created_at: 1_000,
            closes_at: 10_000,
            status: Status::Open,
            bets: vec![],
            total_staked: 0,
        },
    );
    crate::store::View { revision: 0, state }
}

#[test]
fn preset_picker_opens_forms_that_create_the_selected_outcomes() {
    let view = ui_view();
    let picker = serde_json::to_value(
        super::ui::query(&view, &Action::CreateForm, ui_actor(), 10, 2_000).message(),
    )
    .unwrap();
    let select = &picker["components"][0]["components"][0];
    assert_eq!(picker["flags"], 64);
    assert_eq!(picker["allowed_mentions"]["parse"], serde_json::json!([]));
    for (preset, expected) in [
        ("yesno", vec!["Yes", "No"]),
        ("result", vec!["Win", "Lose", "Draw"]),
        ("custom", vec!["First", "Second"]),
    ] {
        assert!(
            select["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option["value"] == preset)
        );
        let response = super::ui::component(
            10,
            ui_actor(),
            select["custom_id"].as_str().unwrap(),
            &[preset.into()],
            &view,
            2_000,
        )
        .unwrap();
        let modal = serde_json::to_value(response).unwrap();
        assert_eq!(
            modal["type"], 9,
            "selection should open a modal immediately"
        );
        let mut fields = vec![text("question", "Will it happen?"), text("closes_at", "1h")];
        if preset == "custom" {
            fields.push(text("options", " First \nSecond "));
        }
        let command = super::ui::modal_command(
            10,
            ui_actor(),
            modal["data"]["custom_id"].as_str().unwrap(),
            fields,
            2_000,
        )
        .unwrap();
        let Command::Create {
            question,
            options,
            closes_at,
            ..
        } = command
        else {
            panic!("expected creation");
        };
        assert_eq!(question, "Will it happen?");
        assert_eq!(options, expected);
        assert_eq!(closes_at, 5_600);
    }
}

#[test]
fn closing_times_discard_fractional_seconds_in_commands_and_forms() {
    for value in ["2030-01-02T03:04:05.900Z", "2030-01-02T04:04:05.900+01:00"] {
        let (_, _, action) = parse(&input(
            "create",
            vec![
                text("question", "Will it rain?"),
                text("options", "Yes | No"),
                text("closes_at", value),
            ],
        ))
        .unwrap();
        let Action::Write(Command::Create { closes_at, .. }) = action else {
            panic!("expected create command");
        };
        assert_eq!(closes_at, 1_893_553_445);

        let command = super::ui::modal_command(
            10,
            ui_actor(),
            "pm:10:20:new:yesno",
            vec![text("question", "Will it rain?"), text("closes_at", value)],
            2_000,
        )
        .unwrap();
        let Command::Create { closes_at, .. } = command else {
            panic!("expected create command");
        };
        assert_eq!(closes_at, 1_893_553_445);
    }
}

#[test]
fn browsing_and_selecting_an_outcome_preserves_market_and_stake() {
    let view = ui_view();
    let list = serde_json::to_value(
        super::ui::query(&view, &Action::List, ui_actor(), 10, 2_000).message(),
    )
    .unwrap();
    let select = &list["components"][0]["components"][0];
    let market_id = select["options"][0]["value"].as_str().unwrap();
    let card = serde_json::to_value(
        super::ui::component(
            10,
            ui_actor(),
            select["custom_id"].as_str().unwrap(),
            &[market_id.into()],
            &view,
            2_000,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(card["data"]["embeds"][0]["title"], "Who wins?");
    let outcomes = &card["data"]["components"][0]["components"][0];
    let blue = outcomes["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["label"] == "Blue")
        .unwrap();
    let modal = serde_json::to_value(
        super::ui::component(
            10,
            ui_actor(),
            outcomes["custom_id"].as_str().unwrap(),
            &[blue["value"].as_str().unwrap().into()],
            &view,
            2_000,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(modal["type"], 9);
    let command = super::ui::modal_command(
        10,
        ui_actor(),
        modal["data"]["custom_id"].as_str().unwrap(),
        vec![text("amount", "25")],
        2_000,
    )
    .unwrap();
    assert_eq!(
        command,
        Command::Bet {
            id: market_id.into(),
            outcome: 1,
            amount: 25
        }
    );
}

#[test]
fn forms_reject_other_members_servers_bots_and_malformed_values() {
    let view = ui_view();
    for (guild, actor) in [
        (11, ui_actor()),
        (
            10,
            crate::domain::Actor {
                user_id: 21,
                ..ui_actor()
            },
        ),
        (
            10,
            crate::domain::Actor {
                bot: true,
                ..ui_actor()
            },
        ),
    ] {
        assert!(
            super::ui::component(
                guild,
                actor,
                "pm:10:20:create",
                &["yesno".into()],
                &view,
                2_000
            )
            .is_err()
        );
        assert!(
            super::ui::modal_command(
                guild,
                actor,
                "pm:10:20:new:yesno",
                vec![text("question", "Q"), text("closes_at", "1h")],
                2_000
            )
            .is_err()
        );
    }
    for time in [
        "0h",
        "-1h",
        "999999999999999999999999d",
        "yesterday",
        "1970-01-01T00:00:00Z",
    ] {
        assert!(
            super::ui::modal_command(
                10,
                ui_actor(),
                "pm:10:20:new:yesno",
                vec![text("question", "Q"), text("closes_at", time)],
                2_000
            )
            .is_err()
        );
    }
    for amount in ["0", "-1", "2.5", "NaN", "9223372036854775808"] {
        assert!(
            super::ui::modal_command(
                10,
                ui_actor(),
                "pm:10:20:stake:78e82954-4c67-4e0d-8c80-8ab95a527ae5:1",
                vec![text("amount", amount)],
                2_000
            )
            .is_err()
        );
    }
    assert!(
        super::ui::component(
            10,
            ui_actor(),
            "pm:10:20:create",
            &["unknown".into()],
            &view,
            2_000
        )
        .is_err()
    );
    assert!(super::ui::component(10, ui_actor(), "pm:10:20:create", &[], &view, 2_000).is_err());
    for options in ["Only one", "Yes\nyes", "Yes\n\nNo"] {
        assert!(
            super::ui::modal_command(
                10,
                ui_actor(),
                "pm:10:20:new:custom",
                vec![
                    text("question", "Q"),
                    text("closes_at", "1h"),
                    text("options", options)
                ],
                2_000
            )
            .is_err()
        );
    }
}

#[test]
fn closed_market_cards_and_stale_outcome_selections_cannot_open_bet_forms() {
    let view = ui_view();
    let action = Action::Show {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".into(),
    };
    let card =
        serde_json::to_value(super::ui::query(&view, &action, ui_actor(), 10, 10_000).message())
            .unwrap();
    assert_eq!(card["components"], serde_json::json!([]));
    assert!(
        super::ui::component(
            10,
            ui_actor(),
            "pm:10:20:bet:78e82954-4c67-4e0d-8c80-8ab95a527ae5",
            &["0".into()],
            &view,
            10_000
        )
        .is_err()
    );
    assert!(
        super::ui::component(
            10,
            ui_actor(),
            "pm:10:20:bet:78e82954-4c67-4e0d-8c80-8ab95a527ae5",
            &["9".into()],
            &view,
            2_000
        )
        .is_err()
    );
}

#[test]
fn announcements_are_registered_as_an_admin_subcommand_group_without_restricting_market() {
    let registration = serde_json::to_value(super::market_command()).unwrap();
    assert!(registration["default_member_permissions"].is_null());

    let announcements = registration["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["name"] == "announcements")
        .expect("announcement configuration must be discoverable");
    assert_eq!(announcements["type"], 2);
    let commands = announcements["options"].as_array().unwrap();
    assert_eq!(
        commands
            .iter()
            .map(|command| command["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["set", "status", "disable"]
    );
    let channel = &commands[0]["options"][0];
    assert_eq!(channel["name"], "channel");
    assert_eq!(channel["type"], 7);
    assert_eq!(channel["required"], true);
    assert_eq!(channel["channel_types"], serde_json::json!([0]));
}

#[test]
fn discord_adapter_flattens_only_the_supported_announcement_group() {
    let mut payload = interaction_json(
        200,
        serde_json::json!({
            "id": "1",
            "name": "market",
            "type": 1,
            "options": [{
                "name": "announcements",
                "type": 2,
                "options": [{
                    "name": "set",
                    "type": 1,
                    "options": [{"name": "channel", "type": 7, "value": "55"}]
                }]
            }]
        }),
    );
    payload["member"] = serde_json::json!({
        "permissions": "32",
        "roles": [],
        "deaf": false,
        "mute": false,
        "flags": 0,
        "joined_at": null,
        "premium_since": null,
        "user": {
            "id": "7",
            "username": "player",
            "discriminator": "0",
            "avatar": null
        }
    });
    let command: serenity::all::CommandInteraction = serde_json::from_value(payload).unwrap();
    assert_eq!(
        super::from_discord(&command).unwrap(),
        Input {
            guild_id: Some(10),
            user_id: 7,
            bot: false,
            moderator: true,
            subcommand: "announcements.set".into(),
            options: vec![channel("channel", 55)],
        }
    );
    for name in ["status", "disable"] {
        let mut command = command.clone();
        command.data.options = serde_json::from_value(serde_json::json!([{
            "name": "announcements",
            "type": 2,
            "options": [{"name": name, "type": 1, "options": []}]
        }]))
        .unwrap();
        let input = super::from_discord(&command).unwrap();
        assert_eq!(input.subcommand, format!("announcements.{name}"));
        assert!(input.options.is_empty());
    }

    for options in [
        serde_json::json!([]),
        serde_json::json!([
            {"name": "set", "type": 1, "options": []},
            {"name": "status", "type": 1, "options": []}
        ]),
        serde_json::json!([{"name": "unknown", "type": 1, "options": []}]),
        serde_json::json!([{
            "name": "nested",
            "type": 2,
            "options": [{"name": "status", "type": 1, "options": []}]
        }]),
    ] {
        let mut malformed = command.clone();
        malformed.data.options = serde_json::from_value(serde_json::json!([{
            "name": "announcements",
            "type": 2,
            "options": options
        }]))
        .unwrap();
        assert!(super::from_discord(&malformed).is_err());
    }
}

#[test]
fn help_is_registered_without_arguments_and_parses_before_enrollment() {
    let registration = serde_json::to_value(super::market_command()).unwrap();
    let help = registration["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["name"] == "help")
        .expect("help must be discoverable");
    assert!(help["options"].as_array().is_none_or(Vec::is_empty));
    assert!(parse(&input("help", vec![])).is_ok());
    assert!(parse(&input("help", vec![text("unexpected", "value")])).is_err());
}

#[test]
fn help_explains_getting_started_without_an_account() {
    let action = parse(&input("help", vec![])).unwrap().2;
    let view = crate::store::View {
        revision: 0,
        state: Default::default(),
    };
    let content = super::render_query(&view, &action, ui_actor(), 0);
    for guidance in [
        "play points",
        "no real money",
        "/market join",
        "/market create",
        "/market list",
        "/market balance",
        "/market leaderboard",
        "/market announcements",
        "Manage Guild",
    ] {
        assert!(content.contains(guidance), "missing guidance: {guidance}");
    }
    let response = serde_json::to_value(super::reply(&content)).unwrap();
    assert_eq!(response["allowed_mentions"]["parse"], serde_json::json!([]));
    assert_eq!(response["allowed_mentions"]["replied_user"], false);
}

fn mentioned_message() -> serenity::all::Message {
    let mut message = serenity::all::Message::default();
    message.guild_id = Some(serenity::all::GuildId::new(10));
    message.author.id = serenity::all::UserId::new(20);
    let mut bot = serenity::all::User::default();
    bot.id = serenity::all::UserId::new(99);
    bot.bot = true;
    message.mentions = vec![bot.clone(), bot];
    message
}

#[test]
fn mentioning_this_bot_produces_one_help_prompt_without_pings() {
    let response = super::mention_reply(&mentioned_message(), 99)
        .expect("a human mentioning this bot should receive help");
    let payload = serde_json::to_value(response).unwrap();
    assert!(
        payload["content"]
            .as_str()
            .unwrap()
            .contains("/market help")
    );
    assert_eq!(payload["allowed_mentions"]["parse"], serde_json::json!([]));
    assert_eq!(payload["allowed_mentions"]["replied_user"], false);
}

#[test]
fn mention_help_ignores_other_mentions_bots_and_private_messages() {
    assert!(super::mention_reply(&mentioned_message(), 98).is_none());
    assert!(super::mention_reply(&mentioned_message(), 0).is_none());
    let mut ordinary = mentioned_message();
    ordinary.mentions.clear();
    ordinary.content = "bot, help please".to_owned();
    assert!(super::mention_reply(&ordinary, 99).is_none());
    let mut from_bot = mentioned_message();
    from_bot.author.bot = true;
    assert!(super::mention_reply(&from_bot, 99).is_none());
    let mut private = mentioned_message();
    private.guild_id = None;
    assert!(super::mention_reply(&private, 99).is_none());
}

#[derive(Default)]
struct AuditRecorder(std::sync::Mutex<Vec<crate::audit::AuditEvent>>);
impl crate::audit::AuditListener for AuditRecorder {
    fn on_event(&self, event: &crate::audit::AuditEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

#[tokio::test]
async fn component_timeout_keeps_private_retry_response_and_reports_correlated_failure() {
    use crate::audit::{AuditEvent, Failure, FailureCategory, Outcome, QueryKind, Stage};
    let recorder = AuditRecorder::default();
    let result = super::read_query(
        &recorder,
        10,
        123,
        QueryKind::Component,
        std::future::pending(),
        Some(std::time::Duration::ZERO),
    )
    .await;
    let response = super::interaction_error(&result.err().unwrap());
    let value = serde_json::to_value(response).unwrap();
    assert_eq!(
        value["data"]["content"],
        "Loading took too long. Please select the option again."
    );
    assert_eq!(value["data"]["flags"], 64);
    assert!(matches!(
        recorder.0.lock().unwrap().as_slice(),
        [AuditEvent::QueryCompleted {
            guild: 10,
            interaction_id: 123,
            query: QueryKind::Component,
            stage: Stage::Query,
            outcome: Outcome::Failed(Failure {
                category: FailureCategory::Timeout,
                ..
            }),
            ..
        }]
    ));
}

#[tokio::test]
async fn failed_query_keeps_safe_response_and_reports_correlated_failure() {
    use crate::audit::{AuditEvent, Failure, FailureCategory, Outcome, QueryKind, Stage};
    let recorder = AuditRecorder::default();
    let read = async {
        Err(crate::store::StoreError::Database(
            sqlx::Error::Configuration("sentinel secret URL".into()),
        ))
    };
    let error = super::read_query(&recorder, 10, 124, QueryKind::Balance, read, None)
        .await
        .err()
        .unwrap();
    assert_eq!(
        serde_json::to_value(super::reply(&error)).unwrap()["content"],
        "The prediction economy is temporarily unavailable. Please try again."
    );
    assert!(matches!(
        recorder.0.lock().unwrap().as_slice(),
        [AuditEvent::QueryCompleted {
            guild: 10,
            interaction_id: 124,
            query: QueryKind::Balance,
            stage: Stage::Query,
            outcome: Outcome::Failed(Failure {
                category: FailureCategory::Configuration,
                sqlstate: None,
                http_status: None,
                discord_code: None
            }),
            ..
        }]
    ));
}

fn discord_http(server: &MockServer) -> serenity::http::Http {
    serenity::http::HttpBuilder::new("test-token")
        .application_id(42.into())
        .proxy(server.uri())
        .ratelimiter_disabled(true)
        .build()
}

fn interaction_json(id: u64, data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": id.to_string(), "application_id": "42", "guild_id": "10", "channel_id": "20",
        "token": "test-interaction-token", "version": 1, "locale": "en-US", "entitlements": [],
        "attachment_size_limit": 1000, "data": data,
        "user": { "id": "7", "username": "player", "discriminator": "0", "avatar": null },
        "message": serenity::all::Message::default(),
    })
}

async fn unavailable_handler(recorder: std::sync::Arc<AuditRecorder>) -> super::Handler {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@localhost/unused")
        .unwrap();
    pool.close().await;
    super::Handler {
        bot_user_id: 99,
        store: std::sync::Arc::new(crate::store::Store::new_with_audit(
            pool,
            42,
            crate::domain::Policy {
                amount: 100,
                interval: 86400,
            },
            recorder,
        )),
    }
}

#[tokio::test]
async fn real_slash_adapter_recovers_acknowledgement_reads_store_and_reports_delivery_failure() {
    use crate::audit::{AuditEvent, Failure, FailureCategory, Outcome, QueryKind, Stage};
    let recorder = std::sync::Arc::new(AuditRecorder::default());
    let handler = unavailable_handler(recorder.clone()).await;
    let server = MockServer::start().await;
    let http = discord_http(&server);
    Mock::given(method("POST"))
        .and(path(
            "/api/v10/interactions/125/test-interaction-token/callback",
        ))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            serde_json::json!({"code": 40060, "message": "sentinel secret acknowledgement text"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(
            "/api/v10/webhooks/42/test-interaction-token/messages/@original",
        ))
        .respond_with(ResponseTemplate::new(503).set_body_json(
            serde_json::json!({"code": 10015, "message": "sentinel secret provider text"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let command = serde_json::from_value(interaction_json(125, serde_json::json!({"id": "1", "name": "market", "type": 1, "options": [{"name": "balance", "type": 1, "options": []}]}))).unwrap();
    handler.handle(&http, command).await;
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method.as_str(), "POST");
    assert_eq!(
        requests[0].url.path(),
        "/api/v10/interactions/125/test-interaction-token/callback"
    );
    assert_eq!(
        requests[0].body_json::<serde_json::Value>().unwrap()["type"],
        5
    );
    assert_eq!(
        requests[0].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
        64
    );
    assert_eq!(requests[1].method.as_str(), "PATCH");
    assert_eq!(
        requests[1].url.path(),
        "/api/v10/webhooks/42/test-interaction-token/messages/@original"
    );
    assert_eq!(
        requests[1].body_json::<serde_json::Value>().unwrap()["content"],
        "The prediction economy is temporarily unavailable. Please try again."
    );
    let events = recorder.0.lock().unwrap();
    assert!(matches!(
        events.as_slice(),
        [
            AuditEvent::InteractionCompleted {
                interaction_id: 125,
                stage: Stage::Acknowledge,
                outcome: Outcome::Succeeded,
                ..
            },
            AuditEvent::QueryCompleted {
                guild: 10,
                interaction_id: 125,
                query: QueryKind::Balance,
                stage: Stage::Query,
                outcome: Outcome::Failed(Failure {
                    category: FailureCategory::Connection,
                    ..
                }),
                ..
            },
            AuditEvent::InteractionCompleted {
                guild: Some(10),
                interaction_id: 125,
                stage: Stage::Deliver,
                outcome: Outcome::Failed(Failure {
                    category: FailureCategory::Discord,
                    sqlstate: None,
                    http_status: Some(503),
                    discord_code: Some(10015)
                })
            },
        ]
    ));
}

#[tokio::test]
async fn real_modal_and_component_adapters_preserve_private_validation_responses() {
    use crate::audit::{AuditEvent, Outcome, Rejection, Stage};
    let recorder = std::sync::Arc::new(AuditRecorder::default());
    let handler = unavailable_handler(recorder.clone()).await;
    let server = MockServer::start().await;
    let http = discord_http(&server);
    Mock::given(method("POST"))
        .and(path(
            "/api/v10/interactions/126/test-interaction-token/callback",
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(
            "/api/v10/webhooks/42/test-interaction-token/messages/@original",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serenity::all::Message::default()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/api/v10/interactions/127/test-interaction-token/callback",
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let modal = serde_json::from_value(interaction_json(
        126,
        serde_json::json!({"custom_id": "pm:invalid", "components": []}),
    ))
    .unwrap();
    handler.handle_modal(&http, modal).await;
    let component = serde_json::from_value(interaction_json(
        127,
        serde_json::json!({"custom_id": "pm:invalid", "component_type": 2}),
    ))
    .unwrap();
    handler.handle_component(&http, component).await;
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[0].body_json::<serde_json::Value>().unwrap()["type"],
        5
    );
    assert_eq!(
        requests[0].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
        64
    );
    assert_eq!(requests[1].method.as_str(), "PATCH");
    assert_eq!(
        requests[1].body_json::<serde_json::Value>().unwrap()["content"],
        "This control belongs to another member or server. Run /market list or /market create."
    );
    assert_eq!(requests[2].method.as_str(), "POST");
    assert_eq!(
        requests[2].body_json::<serde_json::Value>().unwrap()["type"],
        4
    );
    assert_eq!(
        requests[2].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
        64
    );
    assert_eq!(
        requests[2].body_json::<serde_json::Value>().unwrap()["data"]["content"],
        "Choose an option from the market menu."
    );
    let events = recorder.0.lock().unwrap();
    for id in [126, 127] {
        assert!(events.iter().any(|event| matches!(event, AuditEvent::InteractionCompleted { guild: Some(10), interaction_id, stage: Stage::Validate, outcome: Outcome::Rejected(Rejection::InvalidInput) } if *interaction_id == id)));
        assert!(events.iter().any(|event| matches!(event, AuditEvent::InteractionCompleted { guild: Some(10), interaction_id, stage: Stage::Deliver, outcome: Outcome::Succeeded } if *interaction_id == id)));
    }
}

#[tokio::test]
async fn component_read_failure_delivers_initial_private_response_and_registration_is_audited() {
    use crate::audit::{AuditEvent, Outcome, QueryKind, Stage};
    let recorder = std::sync::Arc::new(AuditRecorder::default());
    let handler = unavailable_handler(recorder.clone()).await;
    let server = MockServer::start().await;
    let http = discord_http(&server);
    Mock::given(method("POST"))
        .and(path(
            "/api/v10/interactions/128/test-interaction-token/callback",
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/api/v10/applications/42/guilds/10/commands"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let component = serde_json::from_value(interaction_json(
        128,
        serde_json::json!({"custom_id": "pm:bet", "component_type": 3, "values": ["anything"]}),
    ))
    .unwrap();
    handler.handle_component(&http, component).await;
    handler.register(&http, 10.into()).await;
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests[0].body_json::<serde_json::Value>().unwrap()["type"],
        4
    );
    assert_eq!(
        requests[0].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
        64
    );
    assert_eq!(
        requests[0].body_json::<serde_json::Value>().unwrap()["data"]["content"],
        "The prediction economy is temporarily unavailable. Please try again."
    );
    let events = recorder.0.lock().unwrap();
    assert!(matches!(
        events.as_slice(),
        [
            AuditEvent::QueryCompleted {
                guild: 10,
                interaction_id: 128,
                query: QueryKind::Component,
                outcome: Outcome::Failed(_),
                stage: Stage::Query,
                ..
            },
            AuditEvent::InteractionCompleted {
                interaction_id: 128,
                stage: Stage::Deliver,
                outcome: Outcome::Succeeded,
                ..
            },
            AuditEvent::RegistrationCompleted {
                guild: 10,
                stage: Stage::Register,
                outcome: Outcome::Succeeded
            },
        ]
    ));
}

#[tokio::test]
async fn grant_worker_reports_discovery_failure_and_accepts_shutdown() {
    use crate::audit::{AuditEvent, AuditListener, Failure, FailureCategory, Outcome, Stage};
    struct StopAfterDiscovery {
        events: std::sync::Mutex<Vec<AuditEvent>>,
        shutdown: tokio::sync::watch::Sender<bool>,
    }
    impl AuditListener for StopAfterDiscovery {
        fn on_event(&self, event: &AuditEvent) {
            self.events.lock().unwrap().push(event.clone());
            if matches!(event, AuditEvent::GrantFailed { .. }) {
                self.shutdown.send(true).unwrap();
            }
        }
    }
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let recorder = std::sync::Arc::new(StopAfterDiscovery {
        events: Default::default(),
        shutdown,
    });
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@localhost/unused")
        .unwrap();
    pool.close().await;
    let store = std::sync::Arc::new(crate::store::Store::new_with_audit(
        pool,
        42,
        crate::domain::Policy {
            amount: 100,
            interval: 86400,
        },
        recorder.clone(),
    ));
    super::grant_worker(store, receiver).await;
    assert_eq!(
        *recorder.events.lock().unwrap(),
        vec![AuditEvent::GrantFailed {
            guild: None,
            stage: Stage::Discover,
            outcome: Outcome::Failed(Failure {
                category: FailureCategory::Connection,
                sqlstate: None,
                http_status: None,
                discord_code: None
            }),
        }]
    );
}

#[tokio::test]
async fn gateway_guard_failure_is_reported_before_run_returns() {
    use crate::audit::{AuditEvent, Failure, FailureCategory, LifecycleKind, Outcome, Stage};
    let recorder = std::sync::Arc::new(AuditRecorder::default());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused:unused@localhost/unused")
        .unwrap();
    pool.close().await;
    let store = std::sync::Arc::new(crate::store::Store::new_with_audit(
        pool,
        42,
        crate::domain::Policy {
            amount: 100,
            interval: 86_400,
        },
        recorder.clone(),
    ));

    let result = super::run(store, "unused".into()).await;

    assert!(matches!(
        result,
        Err(super::DiscordError::Store(
            crate::store::StoreError::Database(sqlx::Error::PoolClosed)
        ))
    ));
    assert!(matches!(
        recorder.0.lock().unwrap().as_slice(),
        [AuditEvent::Lifecycle {
            kind: LifecycleKind::Startup,
            application_id: Some(42),
            stage: Stage::Acquire,
            outcome: Outcome::Failed(Failure {
                category: FailureCategory::Connection,
                sqlstate: None,
                http_status: None,
                discord_code: None,
            }),
        }]
    ));
}

#[tokio::test]
async fn query_success_and_corrupt_history_have_distinct_operational_outcomes() {
    use crate::audit::{AuditEvent, Failure, FailureCategory, Outcome, QueryKind};
    let recorder = AuditRecorder::default();
    let view = std::sync::Arc::new(crate::store::View {
        revision: 5,
        state: Default::default(),
    });
    let result = super::read_query(
        &recorder,
        10,
        130,
        QueryKind::List,
        async { Ok(view) },
        None,
    )
    .await
    .unwrap();
    assert_eq!(result.revision, 5);
    let read = async {
        Err(crate::store::StoreError::Domain(
            crate::domain::DomainError::Invalid("corrupt state"),
        ))
    };
    assert!(
        super::read_query(&recorder, 10, 131, QueryKind::Show, read, None)
            .await
            .is_err()
    );
    assert!(matches!(
        recorder.0.lock().unwrap().as_slice(),
        [
            AuditEvent::QueryCompleted {
                interaction_id: 130,
                outcome: Outcome::Succeeded,
                ..
            },
            AuditEvent::QueryCompleted {
                interaction_id: 131,
                outcome: Outcome::Failed(Failure {
                    category: FailureCategory::History,
                    ..
                }),
                ..
            },
        ]
    ));
}

#[tokio::test]
async fn gateway_http_failure_retains_safe_details_after_shutdown() {
    use crate::audit::{AuditEvent, Failure, FailureCategory, LifecycleKind, Outcome, Stage};

    let server = MockServer::start().await;
    let http = discord_http(&server);
    Mock::given(method("GET"))
        .and(path("/api/v10/gateway/bot"))
        .respond_with(ResponseTemplate::new(403).set_body_json(
            serde_json::json!({"code": 50001, "message": "sentinel private gateway error"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let client =
        serenity::client::ClientBuilder::new_with_http(http, serenity::all::GatewayIntents::GUILDS)
            .await
            .unwrap();
    let recorder = std::sync::Arc::new(AuditRecorder::default());
    let handler = unavailable_handler(recorder.clone()).await;

    let result = super::run_gateway_client(handler.store, client).await;

    assert!(matches!(result, Err(super::DiscordError::Gateway)));
    let events = recorder.0.lock().unwrap();
    let lifecycle_events: Vec<_> = events
        .iter()
        .filter(|event| matches!(event, AuditEvent::Lifecycle { .. }))
        .collect();
    assert_eq!(
        lifecycle_events,
        vec![&AuditEvent::Lifecycle {
            kind: LifecycleKind::Shutdown,
            application_id: Some(42),
            stage: Stage::Shutdown,
            outcome: Outcome::Failed(Failure {
                category: FailureCategory::Discord,
                sqlstate: None,
                http_status: Some(403),
                discord_code: Some(50001),
            }),
        }]
    );
}

#[tokio::test]
async fn mention_delivery_reports_one_audit_event_for_success_and_failure() {
    use crate::audit::{AuditEvent, Failure, FailureCategory, Outcome, Stage};
    for failed in [false, true] {
        let recorder = std::sync::Arc::new(AuditRecorder::default());
        let handler = unavailable_handler(recorder.clone()).await;
        let server = MockServer::start().await;
        let http = discord_http(&server);
        let response = if failed {
            ResponseTemplate::new(403).set_body_json(
                serde_json::json!({"code": 50013, "message": "sentinel secret provider text"}),
            )
        } else {
            ResponseTemplate::new(200).set_body_json(serenity::all::Message::default())
        };
        Mock::given(method("POST"))
            .and(path("/api/v10/channels/20/messages"))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
        let mut message = mentioned_message();
        message.id = 123.into();
        message.channel_id = 20.into();
        handler.handle_message(&http, &message).await;
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method.as_str(), "POST");
        assert_eq!(requests[0].url.path(), "/api/v10/channels/20/messages");
        assert!(
            requests[0].body_json::<serde_json::Value>().unwrap()["content"]
                .as_str()
                .unwrap()
                .contains("/market help")
        );
        assert_eq!(
            requests[0].body_json::<serde_json::Value>().unwrap()["allowed_mentions"]["parse"],
            serde_json::json!([])
        );
        assert_eq!(
            requests[0].body_json::<serde_json::Value>().unwrap()["allowed_mentions"]["replied_user"],
            false
        );

        let outcome = if failed {
            Outcome::Failed(Failure {
                category: FailureCategory::Discord,
                sqlstate: None,
                http_status: Some(403),
                discord_code: Some(50013),
            })
        } else {
            Outcome::Succeeded
        };
        assert_eq!(
            *recorder.0.lock().unwrap(),
            vec![AuditEvent::MentionReplyCompleted {
                guild: 10,
                channel_id: 20,
                message_id: 123,
                outcome,
                stage: Stage::Deliver,
            }]
        );
    }
}

#[tokio::test]
async fn ignored_messages_neither_send_nor_emit_audit_events() {
    let recorder = std::sync::Arc::new(AuditRecorder::default());
    let handler = unavailable_handler(recorder.clone()).await;
    let server = MockServer::start().await;
    let http = discord_http(&server);
    let mut ordinary = mentioned_message();
    ordinary.mentions.clear();
    let mut other = mentioned_message();
    for mention in &mut other.mentions {
        mention.id = 98.into();
    }
    let mut bot = mentioned_message();
    bot.author.bot = true;
    let mut private = mentioned_message();
    private.guild_id = None;
    for message in [ordinary, other, bot, private] {
        handler.handle_message(&http, &message).await;
    }

    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(recorder.0.lock().unwrap().is_empty());
}

fn guild_json(permissions: u64) -> serde_json::Value {
    serde_json::json!({
        "id": "10",
        "name": "Test guild",
        "icon": null,
        "icon_hash": null,
        "splash": null,
        "discovery_splash": null,
        "owner_id": "7",
        "afk_channel_id": null,
        "afk_timeout": 60,
        "widget_enabled": false,
        "widget_channel_id": null,
        "verification_level": 0,
        "default_message_notifications": 0,
        "explicit_content_filter": 0,
        "roles": [{
            "id": "10",
            "name": "@everyone",
            "color": 0,
            "colors": {"primary_color": 0, "secondary_color": null, "tertiary_color": null},
            "hoist": false,
            "managed": false,
            "mentionable": false,
            "permissions": permissions.to_string(),
            "position": 0,
            "tags": {},
            "icon": null,
            "unicode_emoji": null
        }],
        "emojis": [],
        "features": [],
        "mfa_level": 0,
        "application_id": null,
        "system_channel_id": null,
        "system_channel_flags": 0,
        "rules_channel_id": null,
        "max_presences": null,
        "max_members": null,
        "vanity_url_code": null,
        "description": null,
        "banner": null,
        "premium_tier": 0,
        "premium_subscription_count": null,
        "preferred_locale": "en-US",
        "public_updates_channel_id": null,
        "max_video_channel_users": null,
        "max_stage_video_channel_users": null,
        "approximate_member_count": null,
        "approximate_presence_count": null,
        "welcome_screen": null,
        "nsfw_level": 0,
        "stickers": [],
        "premium_progress_bar_enabled": false,
        "safety_alerts_channel_id": null,
        "incidents_data": null
    })
}

fn channel_json(guild: u64, kind: u8, deny: u64) -> serde_json::Value {
    serde_json::json!({
        "id": "55",
        "guild_id": guild.to_string(),
        "type": kind,
        "name": "announcements",
        "position": 0,
        "permission_overwrites": [{
            "id": "10",
            "type": 0,
            "allow": "0",
            "deny": deny.to_string()
        }]
    })
}

fn bot_member_json() -> serde_json::Value {
    let mut user = serenity::all::User::default();
    user.id = serenity::all::UserId::new(99);
    user.bot = true;
    serde_json::json!({
        "user": user,
        "nick": null,
        "avatar": null,
        "banner": null,
        "roles": [],
        "joined_at": null,
        "premium_since": null,
        "deaf": false,
        "mute": false,
        "flags": 0,
        "pending": false,
        "permissions": null,
        "communication_disabled_until": null,
        "unusual_dm_activity_until": null,
        "avatar_decoration_data": null
    })
}

async fn mount_destination_reads(
    server: &MockServer,
    channel: serde_json::Value,
    permissions: u64,
) {
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/55"))
        .respond_with(ResponseTemplate::new(200).set_body_json(channel))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(guild_json(permissions)))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v10/guilds/10/members/99"))
        .respond_with(ResponseTemplate::new(200).set_body_json(bot_member_json()))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn destination_validation_uses_real_http_and_effective_bot_permissions() {
    let server = MockServer::start().await;
    mount_destination_reads(&server, channel_json(10, 0, 0), 1024 | 2048).await;

    assert_eq!(
        super::announcements::validate_destination(&discord_http(&server), 10, 99, 55).await,
        Ok(())
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn destination_validation_rejects_cross_guild_non_text_missing_permissions_and_reads() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/55"))
        .respond_with(ResponseTemplate::new(200).set_body_json(channel_json(11, 0, 0)))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        super::announcements::validate_destination(&discord_http(&server), 10, 99, 55).await,
        Err("Choose a text channel in this server.")
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/55"))
        .respond_with(ResponseTemplate::new(200).set_body_json(channel_json(10, 2, 0)))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        super::announcements::validate_destination(&discord_http(&server), 10, 99, 55).await,
        Err("Choose a text channel in this server.")
    );

    let server = MockServer::start().await;
    mount_destination_reads(&server, channel_json(10, 0, 2048), 1024 | 2048).await;
    assert_eq!(
        super::announcements::validate_destination(&discord_http(&server), 10, 99, 55).await,
        Err("I need View Channel and Send Messages in that channel.")
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/55"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        super::announcements::validate_destination(&discord_http(&server), 10, 99, 55).await,
        Err("I could not verify that channel. Please try again.")
    );
}

fn admin_announcement_command(
    id: u64,
    subcommand: &str,
    options: serde_json::Value,
) -> serenity::all::CommandInteraction {
    let mut payload = interaction_json(
        id,
        serde_json::json!({
            "id": "1",
            "name": "market",
            "type": 1,
            "options": [{
                "name": "announcements",
                "type": 2,
                "options": [{
                    "name": subcommand,
                    "type": 1,
                    "options": options
                }]
            }]
        }),
    );
    payload["member"] = serde_json::json!({
        "permissions": "32",
        "roles": [],
        "deaf": false,
        "mute": false,
        "flags": 0,
        "joined_at": null,
        "premium_since": null,
        "user": {
            "id": "7",
            "username": "admin",
            "discriminator": "0",
            "avatar": null
        }
    });
    serde_json::from_value(payload).unwrap()
}

async fn mount_deferred_reply(server: &MockServer, id: u64) {
    Mock::given(method("POST"))
        .and(path(format!(
            "/api/v10/interactions/{id}/test-interaction-token/callback"
        )))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(
            "/api/v10/webhooks/42/test-interaction-token/messages/@original",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serenity::all::Message::default()))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn set_handler_keeps_receipt_lookup_failures_private_without_destination_reads() {
    let recorder = std::sync::Arc::new(AuditRecorder::default());
    let handler = unavailable_handler(recorder).await;
    let server = MockServer::start().await;
    mount_deferred_reply(&server, 201).await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/55"))
        .respond_with(ResponseTemplate::new(200).set_body_json(channel_json(11, 0, 0)))
        .expect(0)
        .mount(&server)
        .await;

    handler
        .handle(
            &discord_http(&server),
            admin_announcement_command(
                201,
                "set",
                serde_json::json!([{"name": "channel", "type": 7, "value": "55"}]),
            ),
        )
        .await;

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].body_json::<serde_json::Value>().unwrap()["type"],
        5
    );
    assert_eq!(
        requests[0].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
        64
    );
    let response = requests[1].body_json::<serde_json::Value>().unwrap();
    assert_eq!(
        response["content"],
        "The prediction economy is temporarily unavailable. Please try again."
    );
    assert_eq!(response["allowed_mentions"]["parse"], serde_json::json!([]));
    assert!(!requests.iter().any(|request| {
        request.method.as_str() == "POST" && request.url.path() == "/api/v10/channels/55/messages"
    }));
}

#[test]
fn announcement_status_and_receipts_explain_delivery_state() {
    let status = crate::announcements::AnnouncementStatus {
        channel_id: Some(55),
        enabled: true,
        version: 3,
        pause_reason: Some("The configured channel is unavailable.".into()),
        pending: 4,
    };
    let rendered = super::announcements::render_status(&status);
    for detail in [
        "<#55>",
        "Status: paused",
        "Pending: 4",
        "Reason: The configured channel is unavailable.",
    ] {
        assert!(rendered.contains(detail), "missing status detail: {detail}");
    }

    let changed =
        super::announcements::configuration_receipt("Announcements enabled for <#55>.", false);
    assert!(changed.contains("Pending announcements will use this destination."));
    assert!(changed.contains("already in flight"));
    let disabled = super::announcements::configuration_receipt("Announcements disabled.", true);
    assert!(disabled.contains("Pending announcements were discarded."));
    assert!(disabled.contains("already in flight"));
}

#[tokio::test]
async fn status_and_disable_handlers_use_private_deferred_store_paths() {
    use crate::audit::AuditEvent;

    for (id, action) in [(203, "status"), (204, "disable")] {
        let recorder = std::sync::Arc::new(AuditRecorder::default());
        let handler = unavailable_handler(recorder.clone()).await;
        let server = MockServer::start().await;
        mount_deferred_reply(&server, id).await;

        handler
            .handle(
                &discord_http(&server),
                admin_announcement_command(id, action, serde_json::json!([])),
            )
            .await;

        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0].body_json::<serde_json::Value>().unwrap()["type"],
            5
        );
        assert_eq!(
            requests[0].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
            64
        );
        let response = requests[1].body_json::<serde_json::Value>().unwrap();
        assert_eq!(
            response["content"],
            "The prediction economy is temporarily unavailable. Please try again."
        );
        assert_eq!(response["allowed_mentions"]["parse"], serde_json::json!([]));
        assert!(
            !recorder
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|event| matches!(event, AuditEvent::QueryCompleted { .. })),
            "{action} fell through to the economy query path"
        );
    }
}

#[tokio::test]
async fn gateway_supervises_announcement_worker_panic_and_returns_failure() {
    use crate::audit::{AuditEvent, LifecycleKind, Outcome, Stage};
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/gateway/bot"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(serde_json::json!({"code":50001,"message":"private gateway error"}))
                .set_delay(std::time::Duration::from_secs(10)),
        )
        .mount(&server)
        .await;
    let client = serenity::client::ClientBuilder::new_with_http(
        discord_http(&server),
        serenity::all::GatewayIntents::GUILDS,
    )
    .await
    .unwrap();
    let recorder = std::sync::Arc::new(AuditRecorder::default());
    let handler = unavailable_handler(recorder.clone()).await;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        super::run_gateway_client_with_clock(
            handler.store,
            client,
            std::sync::Arc::new(|| panic!("worker clock panic")),
        ),
    )
    .await
    .expect("a dead worker must stop the gateway");
    assert!(matches!(result, Err(super::DiscordError::Gateway)));
    assert!(recorder.0.lock().unwrap().iter().any(|event| matches!(
        event,
        AuditEvent::Lifecycle {
            kind: LifecycleKind::Shutdown,
            stage: Stage::AnnouncementWorker,
            outcome: Outcome::Failed(_),
            ..
        }
    )));
}
