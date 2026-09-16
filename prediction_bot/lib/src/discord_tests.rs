use super::{Action, Input, InputOption, InputValue, parse};
use crate::domain::Command;

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
