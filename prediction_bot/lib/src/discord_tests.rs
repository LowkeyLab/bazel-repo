use super::{Action, Input, InputOption, InputValue, parse};
use crate::domain::Command;
use crate::types::{
    ApplicationId, ChannelId, ConfigurationVersion, EventRevision, GuildId, OutcomeIndex, Points,
    UserId,
};
use googletest::{
    assert_that,
    matchers::{
        anything, contains, contains_substring, elements_are, ends_with, eq, err, is_empty, le, lt,
        matches_pattern, none, not, ok, some,
    },
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn input(subcommand: &str, options: Vec<InputOption>) -> Input {
    Input {
        guild_id: Some(GuildId(10)),
        user_id: UserId(20),
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
        value: InputValue::Channel(ChannelId(value)),
    }
}

#[googletest::test]
fn announcement_configuration_requires_server_management() {
    let mut request = input("announcements.disable", vec![]);
    assert_that!(parse(&request), err(anything()));
    request.moderator = true;
    assert_that!(
        parse(&request),
        ok((
            eq(&GuildId(10)),
            anything(),
            eq(&Action::AnnouncementsDisable)
        ))
    );
}

#[googletest::test]
fn announcement_actions_require_exact_typed_options() {
    let mut set = input("announcements.set", vec![channel("channel", 55)]);
    assert_that!(parse(&set), err(anything()));
    set.moderator = true;
    assert_that!(
        parse(&set).unwrap().2,
        eq(&Action::AnnouncementsSet {
            channel_id: ChannelId(55)
        })
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
        assert_that!(parse(&malformed), err(anything()));
    }

    let mut status = input("announcements.status", vec![]);
    status.moderator = true;
    assert_that!(
        parse(&status),
        ok((
            eq(&GuildId(10)),
            anything(),
            eq(&Action::AnnouncementsStatus)
        ))
    );

    status.guild_id = None;
    assert_that!(parse(&status), err(anything()));
    status.guild_id = Some(GuildId(10));
    status.bot = true;
    assert_that!(parse(&status), err(anything()));
}

#[googletest::test]
fn guild_and_bot_metadata_are_enforced() {
    let mut join = input("join", vec![]);
    assert_that!(
        parse(&join),
        ok((
            eq(&GuildId(10)),
            anything(),
            matches_pattern!(Action::Write(eq(&Command::Join)))
        ))
    );
    join.guild_id = None;
    assert_that!(
        parse(&join),
        eq(&Err("This command is available only in a server."))
    );
    join.guild_id = Some(GuildId(10));
    join.bot = true;
    assert_that!(
        parse(&join),
        eq(&Err("Bots cannot use the prediction economy."))
    );
}

#[googletest::test]
fn moderator_flag_is_passed_to_resolution_and_cancel() {
    for moderator in [false, true] {
        let mut resolve = input("resolve", vec![text("id", "1234"), number("outcome", 2)]);
        resolve.moderator = moderator;
        let (_, actor, action) = parse(&resolve).unwrap();
        assert_that!(actor.user_id, eq(UserId(20)));
        assert_that!(actor.moderator, eq(moderator));
        assert_that!(
            action,
            eq(&Action::Write(Command::Resolve {
                id: "1234".to_owned().into(),
                outcome: OutcomeIndex(1)
            }))
        );
    }
    let cancel = input("cancel", vec![text("id", "1234")]);
    assert_that!(parse(&cancel).unwrap().1.moderator, eq(false));
}

#[googletest::test]
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
    assert_that!(question, eq("Will it rain?"));
    assert_that!(options, eq(&vec!["Yes", "No"]));
    assert_that!(closes_at, eq(1_893_553_445));

    let bet = input(
        "bet",
        vec![
            text("id", "1234"),
            number("outcome", 1),
            number("amount", 25),
        ],
    );
    assert_that!(
        parse(&bet).unwrap().2,
        eq(&Action::Write(Command::Bet {
            id: "1234".to_owned().into(),
            outcome: OutcomeIndex(0),
            amount: Points(25)
        }))
    );
}

#[googletest::test]
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
        assert_that!(parse(&bet), err(anything()));
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
        assert_that!(parse(&create), err(anything()));
    }
}

#[googletest::test]
fn query_outputs_are_scoped_ranked_and_bounded() {
    use super::{render_query, reply, truncate};
    use crate::domain::{Account, Actor, Bet, Market, State, Status};
    use crate::store::View;

    let mut state = State::default();
    state.accounts.insert(
        UserId(20),
        Account {
            balance: Points(75),
            next_grant: 1_900_000_000,
        },
    );
    state.accounts.insert(
        UserId(9),
        Account {
            balance: Points(75),
            next_grant: 1_900_000_000,
        },
    );
    state.accounts.insert(
        UserId(99),
        Account {
            balance: Points(1),
            next_grant: 1_900_000_000,
        },
    );
    state.markets.insert(
        "one".into(),
        Market {
            creator: UserId(20),
            question: "@everyone wins?".to_owned(),
            options: vec!["Yes".to_owned(), "No".to_owned()],
            closes_at: 1_900_000_000,
            created_at: 1_800_000_000,
            status: Status::Open,
            bets: vec![Bet {
                user_id: UserId(20),
                outcome: OutcomeIndex(0),
                amount: Points(25),
            }],
            total_staked: Points(25),
        },
    );
    let view = View {
        revision: EventRevision(7),
        state,
    };
    let actor = Actor {
        user_id: UserId(20),
        moderator: false,
        bot: false,
    };
    let payload = serde_json::to_value(
        super::ui::query(
            &view,
            &Action::Leaderboard,
            actor,
            GuildId(10),
            1_850_000_000,
        )
        .edit(),
    )
    .unwrap();
    let leaderboard = payload["content"].as_str().unwrap();
    assert_that!(
        payload["allowed_mentions"]["parse"],
        eq(&serde_json::json!([]))
    );
    assert_that!(payload["allowed_mentions"]["replied_user"], eq(false));
    assert_that!(leaderboard, contains_substring("<@20> — 75 points"));
    assert_that!(
        leaderboard.find("<@9>").unwrap(),
        lt(leaderboard.find("<@20>").unwrap())
    );
    assert_that!(
        render_query(&view, &Action::Balance, actor, 1_850_000_000),
        contains_substring("75 points")
    );
    assert_that!(
        render_query(
            &view,
            &Action::Show {
                id: "one".to_owned().into()
            },
            actor,
            1_850_000_000
        ),
        contains_substring("25 points pooled")
    );
    assert_that!(
        render_query(
            &view,
            &Action::Show {
                id: "one".to_owned().into()
            },
            actor,
            1_900_000_000
        ),
        contains_substring("Closed; awaiting outcome")
    );
    assert_that!(
        render_query(&view, &Action::List, actor, 1_900_000_000),
        contains_substring("No markets")
    );
    assert_that!(
        render_query(
            &view,
            &Action::Show {
                id: "other-server".to_owned().into()
            },
            actor,
            1_850_000_000
        ),
        contains_substring("in this server")
    );

    let content = truncate(&"🪙".repeat(2_000));
    assert_that!(content.encode_utf16().count(), le(2_000));
    assert_that!(content, ends_with("…"));
    let value = serde_json::to_value(reply("@everyone <@123>")).unwrap();
    assert_that!(
        value["allowed_mentions"]["parse"],
        eq(&serde_json::json!([]))
    );
}

#[googletest::test]
fn storage_failures_do_not_expose_database_details() {
    use super::safe_error;
    use crate::store::StoreError;
    let error = StoreError::Database(sqlx::Error::Configuration("secret database URL".into()));
    assert_that!(
        safe_error(&error),
        eq("The prediction economy is temporarily unavailable. Please try again.")
    );
}

#[googletest::test]
fn list_keeps_ten_ids_when_questions_use_long_emoji_text() {
    use super::{render_query, truncate};
    use crate::domain::{Actor, Market, State, Status};
    use crate::store::View;

    let mut state = State::default();
    for index in 0..10 {
        let id = format!("{index:08x}-0000-0000-0000-000000000000");
        state.markets.insert(
            id.into(),
            Market {
                creator: UserId(20),
                question: "🪙".repeat(200),
                options: vec!["Yes".to_owned(), "No".to_owned()],
                created_at: 1_800_000_000 + index,
                closes_at: 1_900_000_000,
                status: Status::Open,
                bets: vec![],
                total_staked: Points(0),
            },
        );
    }
    let view = View {
        revision: EventRevision(10),
        state,
    };
    let actor = Actor {
        user_id: UserId(20),
        moderator: false,
        bot: false,
    };
    let message = render_query(&view, &Action::List, actor, 1_850_000_000);
    assert_that!(message.encode_utf16().count(), le(2_000));
    let reply = truncate(&message);
    for index in 0..10 {
        let id = format!("{index:08x}-0000-0000-0000-000000000000");
        assert_that!(
            reply,
            contains_substring(id.as_str()),
            "missing market ID {id}"
        );
    }
}

#[googletest::test]
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
    assert_that!(replayed.as_deref(), eq(Some("Recorded market response")));
    let edit = serde_json::to_value(super::reply(replayed.as_deref().unwrap())).unwrap();
    assert_that!(edit["content"], eq("Recorded market response"));
    assert_that!(calls.load(Ordering::SeqCst), eq(1));

    let rejected = content_after_defer(recoverable_defer_code(40_061), {
        let calls = Arc::clone(&calls);
        move || async move {
            calls.fetch_add(1, Ordering::SeqCst);
            "Should not run".to_owned()
        }
    })
    .await;
    assert_that!(rejected, eq(&None));
    assert_that!(calls.load(Ordering::SeqCst), eq(1));
}

#[googletest::test]
fn expected_application_id_must_match_authenticated_identity() {
    use super::verify_application_id;
    assert_that!(
        verify_application_id(ApplicationId(123), None),
        eq(Ok(ApplicationId(123)))
    );
    assert_that!(
        verify_application_id(ApplicationId(123), Some(ApplicationId(123))),
        eq(Ok(ApplicationId(123)))
    );
    assert_that!(
        verify_application_id(ApplicationId(123), Some(ApplicationId(456))),
        eq(Err(
            "configured application ID does not match Discord token"
        ))
    );
    assert_that!(
        verify_application_id(ApplicationId(0), None),
        err(anything())
    );
}

#[googletest::test]
fn creation_can_start_without_typing_slash_command_fields() {
    assert_that!(parse(&input("create", vec![])), ok(anything()));
    let registration = serde_json::to_value(super::market_command()).unwrap();
    let create = registration["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["name"] == "create")
        .unwrap();
    assert_that!(
        create["options"]
            .as_array()
            .unwrap()
            .iter()
            .all(|option| option["required"] != true),
        eq(true)
    );
}

#[googletest::test]
fn resolution_can_start_without_ids_or_outcome_numbers() {
    assert_that!(parse(&input("resolve", vec![])), ok(anything()));
    let registration = serde_json::to_value(super::market_command()).unwrap();
    let resolve = registration["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["name"] == "resolve")
        .unwrap();
    assert_that!(
        resolve["options"]
            .as_array()
            .unwrap()
            .iter()
            .all(|option| option["required"] != true),
        eq(true)
    );
    for options in [vec![text("id", "market")], vec![number("outcome", 1)]] {
        assert_that!(parse(&input("resolve", options)), err(anything()));
    }
}

fn ui_actor() -> crate::domain::Actor {
    crate::domain::Actor {
        user_id: UserId(20),
        moderator: false,
        bot: false,
    }
}

fn ui_view() -> crate::store::View {
    use crate::domain::{Account, Market, State, Status};
    let mut state = State::default();
    state.accounts.insert(
        UserId(20),
        Account {
            balance: Points(100),
            next_grant: 90_000,
        },
    );
    state.markets.insert(
        "78e82954-4c67-4e0d-8c80-8ab95a527ae5".into(),
        Market {
            creator: UserId(20),
            question: "Who wins?".into(),
            options: vec!["Red".into(), "Blue".into()],
            created_at: 1_000,
            closes_at: 10_000,
            status: Status::Open,
            bets: vec![],
            total_staked: Points(0),
        },
    );
    crate::store::View {
        revision: EventRevision(0),
        state,
    }
}

#[googletest::test]
fn resolve_picker_filters_status_close_time_and_current_actor_permissions() {
    use crate::domain::Status;
    let mut view = ui_view();
    let base = view.state.markets.values().next().unwrap().clone();
    view.state.markets.clear();
    for (id, creator, closes_at, status) in [
        ("mine", 20, 2000, Status::Open),
        ("others", 21, 1999, Status::Open),
        ("future", 20, 2001, Status::Open),
        (
            "resolved",
            20,
            1999,
            Status::Resolved {
                outcome: OutcomeIndex(0),
                refunded: true,
            },
        ),
        ("cancelled", 20, 1999, Status::Cancelled),
    ] {
        let mut market = base.clone();
        market.creator = UserId(creator);
        market.closes_at = closes_at;
        market.status = status;
        view.state.markets.insert(id.into(), market);
    }
    let action = parse(&input("resolve", vec![])).unwrap().2;
    for (actor, expected) in [
        (ui_actor(), vec!["mine"]),
        (
            crate::domain::Actor {
                moderator: true,
                ..ui_actor()
            },
            vec!["others", "mine"],
        ),
    ] {
        let panel = serde_json::to_value(
            super::ui::query(&view, &action, actor, 10.into(), 2000).message(),
        )
        .unwrap();
        let ids: Vec<_> = panel["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|option| option["value"].as_str().unwrap())
            .collect();
        assert_that!(ids, eq(&expected));
        assert_that!(panel["flags"], eq(64));
    }
    view.state.markets.clear();
    let empty = serde_json::to_value(
        super::ui::query(&view, &action, ui_actor(), 10.into(), 2000).message(),
    )
    .unwrap();
    assert_that!(empty["components"], eq(&serde_json::json!([])));
    assert_that!(
        empty["content"].as_str().unwrap(),
        contains_substring("No closed")
    );
}

#[googletest::test]
fn preset_picker_opens_forms_that_create_the_selected_outcomes() {
    let view = ui_view();
    let picker = serde_json::to_value(
        super::ui::query(&view, &Action::CreateForm, ui_actor(), 10.into(), 2_000).message(),
    )
    .unwrap();
    let select = &picker["components"][0]["components"][0];
    assert_that!(picker["flags"], eq(64));
    assert_that!(
        picker["allowed_mentions"]["parse"],
        eq(&serde_json::json!([]))
    );
    for (preset, expected) in [
        ("yesno", vec!["Yes", "No"]),
        ("result", vec!["Win", "Lose", "Draw"]),
        ("custom", vec!["First", "Second"]),
    ] {
        assert_that!(
            select["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option["value"] == preset),
            eq(true)
        );
        let response = super::ui::component(
            10.into(),
            ui_actor(),
            select["custom_id"].as_str().unwrap(),
            &[preset.into()],
            &view,
            2_000,
        )
        .unwrap();
        let modal = serde_json::to_value(response).unwrap();
        assert_that!(
            modal["type"],
            eq(9),
            "selection should open a modal immediately"
        );
        let mut fields = vec![text("question", "Will it happen?"), text("closes_at", "1h")];
        if preset == "custom" {
            fields.push(text("options", " First \nSecond "));
        }
        let command = super::ui::modal_command(
            10.into(),
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
        assert_that!(question, eq("Will it happen?"));
        assert_that!(options, eq(&expected));
        assert_that!(closes_at, eq(5_600));
    }
}

#[googletest::test]
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
        assert_that!(closes_at, eq(1_893_553_445));

        let command = super::ui::modal_command(
            10.into(),
            ui_actor(),
            "pm:10:20:new:yesno",
            vec![text("question", "Will it rain?"), text("closes_at", value)],
            2_000,
        )
        .unwrap();
        let Command::Create { closes_at, .. } = command else {
            panic!("expected create command");
        };
        assert_that!(closes_at, eq(1_893_553_445));
    }
}

#[googletest::test]
fn browsing_markets_shows_read_only_cards_with_betting_guidance() {
    let view = ui_view();
    let list = serde_json::to_value(
        super::ui::query(&view, &Action::List, ui_actor(), 10.into(), 2_000).message(),
    )
    .unwrap();
    let select = &list["components"][0]["components"][0];
    let market_id = select["options"][0]["value"].as_str().unwrap();
    let card = serde_json::to_value(
        super::ui::component(
            10.into(),
            ui_actor(),
            select["custom_id"].as_str().unwrap(),
            &[market_id.into()],
            &view,
            2_000,
        )
        .unwrap(),
    )
    .unwrap();
    assert_that!(card["data"]["embeds"][0]["title"], eq("Who wins?"));
    assert_that!(card["data"]["components"], eq(&serde_json::json!([])));
    assert_that!(
        card["data"]["content"].as_str().unwrap(),
        contains_substring("/market bet")
    );
    let shown = super::ui::query(
        &view,
        &Action::Show {
            id: market_id.into(),
        },
        ui_actor(),
        10.into(),
        2_000,
    );
    assert_that!(shown.components, is_empty());
    assert_that!(shown.content, contains_substring("/market bet"));
}

#[googletest::test]
fn forms_reject_other_members_servers_bots_and_malformed_values() {
    let view = ui_view();
    for (guild, actor) in [
        (11, ui_actor()),
        (
            10,
            crate::domain::Actor {
                user_id: UserId(21),
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
        assert_that!(
            super::ui::component(
                GuildId(guild),
                actor,
                "pm:10:20:create",
                &["yesno".into()],
                &view,
                2_000
            ),
            err(anything())
        );
        assert_that!(
            super::ui::modal_command(
                GuildId(guild),
                actor,
                "pm:10:20:new:yesno",
                vec![text("question", "Q"), text("closes_at", "1h")],
                2_000
            ),
            err(anything())
        );
    }
    for time in [
        "0h",
        "-1h",
        "999999999999999999999999d",
        "yesterday",
        "1970-01-01T00:00:00Z",
    ] {
        assert_that!(
            super::ui::modal_command(
                10.into(),
                ui_actor(),
                "pm:10:20:new:yesno",
                vec![text("question", "Q"), text("closes_at", time)],
                2_000
            ),
            err(anything())
        );
    }
    for amount in ["0", "-1", "2.5", "NaN", "9223372036854775808"] {
        assert_that!(
            super::ui::modal_command(
                10.into(),
                ui_actor(),
                "pm:10:20:stake:78e82954-4c67-4e0d-8c80-8ab95a527ae5:1",
                vec![text("amount", amount)],
                2_000
            ),
            err(anything())
        );
    }
    assert_that!(
        super::ui::component(
            10.into(),
            ui_actor(),
            "pm:10:20:create",
            &["unknown".into()],
            &view,
            2_000
        ),
        err(anything())
    );
    assert_that!(
        super::ui::component(10.into(), ui_actor(), "pm:10:20:create", &[], &view, 2_000),
        err(anything())
    );
    for options in ["Only one", "Yes\nyes", "Yes\n\nNo"] {
        assert_that!(
            super::ui::modal_command(
                10.into(),
                ui_actor(),
                "pm:10:20:new:custom",
                vec![
                    text("question", "Q"),
                    text("closes_at", "1h"),
                    text("options", options)
                ],
                2_000
            ),
            err(anything())
        );
    }
}

#[googletest::test]
fn closed_market_cards_and_stale_outcome_selections_cannot_open_bet_forms() {
    let view = ui_view();
    let action = Action::Show {
        id: "78e82954-4c67-4e0d-8c80-8ab95a527ae5".into(),
    };
    let card = serde_json::to_value(
        super::ui::query(&view, &action, ui_actor(), 10.into(), 10_000).message(),
    )
    .unwrap();
    assert_that!(card["components"], eq(&serde_json::json!([])));
    assert_that!(
        super::ui::component(
            10.into(),
            ui_actor(),
            "pm:10:20:bet:78e82954-4c67-4e0d-8c80-8ab95a527ae5",
            &["0".into()],
            &view,
            10_000
        ),
        err(anything())
    );
    assert_that!(
        super::ui::component(
            10.into(),
            ui_actor(),
            "pm:10:20:bet:78e82954-4c67-4e0d-8c80-8ab95a527ae5",
            &["9".into()],
            &view,
            2_000
        ),
        err(anything())
    );
}

#[googletest::test]
fn announcements_are_registered_as_an_admin_subcommand_group_without_restricting_market() {
    let registration = serde_json::to_value(super::market_command()).unwrap();
    assert_that!(
        registration["default_member_permissions"].is_null(),
        eq(true)
    );

    let announcements = registration["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["name"] == "announcements")
        .expect("announcement configuration must be discoverable");
    assert_that!(announcements["type"], eq(2));
    let commands = announcements["options"].as_array().unwrap();
    assert_that!(
        commands
            .iter()
            .map(|command| command["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        eq(&vec!["set", "status", "disable"])
    );
    let channel = &commands[0]["options"][0];
    assert_that!(channel["name"], eq("channel"));
    assert_that!(channel["type"], eq(7));
    assert_that!(channel["required"], eq(true));
    assert_that!(channel["channel_types"], eq(&serde_json::json!([0])));
}

#[googletest::test]
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
    assert_that!(
        super::from_discord(&command).unwrap(),
        eq(&Input {
            guild_id: Some(GuildId(10)),
            user_id: UserId(7),
            bot: false,
            moderator: true,
            subcommand: "announcements.set".into(),
            options: vec![channel("channel", 55)],
        })
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
        assert_that!(input.subcommand, eq(&format!("announcements.{name}")));
        assert_that!(input.options, is_empty());
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
        assert_that!(super::from_discord(&malformed), err(anything()));
    }
}

#[googletest::test]
fn help_is_registered_without_arguments_and_parses_before_enrollment() {
    let registration = serde_json::to_value(super::market_command()).unwrap();
    let help = registration["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["name"] == "help")
        .expect("help must be discoverable");
    assert_that!(
        help["options"].as_array().is_none_or(Vec::is_empty),
        eq(true)
    );
    assert_that!(parse(&input("help", vec![])), ok(anything()));
    assert_that!(
        parse(&input("help", vec![text("unexpected", "value")])),
        err(anything())
    );
}

#[googletest::test]
fn help_explains_getting_started_without_an_account() {
    let action = parse(&input("help", vec![])).unwrap().2;
    let view = crate::store::View {
        revision: EventRevision(0),
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
        assert_that!(
            content,
            contains_substring(guidance),
            "missing guidance: {guidance}"
        );
    }
    let response = serde_json::to_value(super::reply(&content)).unwrap();
    assert_that!(
        response["allowed_mentions"]["parse"],
        eq(&serde_json::json!([]))
    );
    assert_that!(response["allowed_mentions"]["replied_user"], eq(false));
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

#[googletest::test]
fn mentioning_this_bot_produces_one_help_prompt_without_pings() {
    let response = super::mention_reply(&mentioned_message(), 99.into())
        .expect("a human mentioning this bot should receive help");
    let payload = serde_json::to_value(response).unwrap();
    assert_that!(
        payload["content"].as_str().unwrap(),
        contains_substring("/market help")
    );
    assert_that!(
        payload["allowed_mentions"]["parse"],
        eq(&serde_json::json!([]))
    );
    assert_that!(payload["allowed_mentions"]["replied_user"], eq(false));
}

#[googletest::test]
fn mention_help_ignores_other_mentions_bots_and_private_messages() {
    assert_that!(
        super::mention_reply(&mentioned_message(), 98.into()),
        none()
    );
    assert_that!(super::mention_reply(&mentioned_message(), 0.into()), none());
    let mut ordinary = mentioned_message();
    ordinary.mentions.clear();
    ordinary.content = "bot, help please".to_owned();
    assert_that!(super::mention_reply(&ordinary, 99.into()), none());
    let mut from_bot = mentioned_message();
    from_bot.author.bot = true;
    assert_that!(super::mention_reply(&from_bot, 99.into()), none());
    let mut private = mentioned_message();
    private.guild_id = None;
    assert_that!(super::mention_reply(&private, 99.into()), none());
}

#[derive(Default)]
struct AuditRecorder(std::sync::Mutex<Vec<crate::audit::AuditEvent>>);
impl crate::audit::AuditListener for AuditRecorder {
    fn on_event(&self, event: &crate::audit::AuditEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

#[googletest::test]
#[tokio::test]
async fn component_timeout_keeps_private_retry_response_and_reports_correlated_failure() {
    use crate::audit::{AuditEvent, Failure, FailureCategory, Outcome, QueryKind, Stage};
    let recorder = AuditRecorder::default();
    let result = super::read_query(
        &recorder,
        10.into(),
        123,
        QueryKind::Component,
        std::future::pending(),
        Some(std::time::Duration::ZERO),
    )
    .await;
    let response = super::interaction_error(&result.err().unwrap());
    let value = serde_json::to_value(response).unwrap();
    assert_that!(
        value["data"]["content"],
        eq("Loading took too long. Please select the option again.")
    );
    assert_that!(value["data"]["flags"], eq(64));
    assert_that!(
        recorder.0.lock().unwrap().as_slice(),
        elements_are![matches_pattern!(AuditEvent::QueryCompleted {
            guild: eq(&GuildId(10)),
            interaction_id: eq(&123),
            query: eq(&QueryKind::Component),
            stage: eq(&Stage::Query),
            outcome: matches_pattern!(Outcome::Failed(matches_pattern!(Failure {
                category: eq(&FailureCategory::Timeout),
                ..
            }))),
            ..
        })]
    );
}

#[googletest::test]
#[tokio::test]
async fn failed_query_keeps_safe_response_and_reports_correlated_failure() {
    use crate::audit::{AuditEvent, Failure, FailureCategory, Outcome, QueryKind, Stage};
    let recorder = AuditRecorder::default();
    let read = async {
        Err(crate::store::StoreError::Database(
            sqlx::Error::Configuration("sentinel secret URL".into()),
        ))
    };
    let error = super::read_query(&recorder, 10.into(), 124, QueryKind::Balance, read, None)
        .await
        .err()
        .unwrap();
    assert_that!(
        serde_json::to_value(super::reply(&error)).unwrap()["content"],
        eq("The prediction economy is temporarily unavailable. Please try again.")
    );
    assert_that!(
        recorder.0.lock().unwrap().as_slice(),
        elements_are![matches_pattern!(AuditEvent::QueryCompleted {
            guild: eq(&GuildId(10)),
            interaction_id: eq(&124),
            query: eq(&QueryKind::Balance),
            stage: eq(&Stage::Query),
            outcome: matches_pattern!(Outcome::Failed(matches_pattern!(Failure {
                category: eq(&FailureCategory::Configuration),
                sqlstate: none(),
                http_status: none(),
                discord_code: none()
            }))),
            ..
        })]
    );
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
        bot_user_id: UserId(99),
        store: std::sync::Arc::new(crate::store::Store::new_with_audit(
            pool,
            42.into(),
            crate::domain::Policy {
                amount: Points(100),
                interval: 86400,
            },
            recorder,
        )),
    }
}

#[googletest::test]
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
    assert_that!(requests.len(), eq(2));
    assert_that!(requests[0].method.as_str(), eq("POST"));
    assert_that!(
        requests[0].url.path(),
        eq("/api/v10/interactions/125/test-interaction-token/callback")
    );
    assert_that!(
        requests[0].body_json::<serde_json::Value>().unwrap()["type"],
        eq(5)
    );
    assert_that!(
        requests[0].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
        eq(64)
    );
    assert_that!(requests[1].method.as_str(), eq("PATCH"));
    assert_that!(
        requests[1].url.path(),
        eq("/api/v10/webhooks/42/test-interaction-token/messages/@original")
    );
    assert_that!(
        requests[1].body_json::<serde_json::Value>().unwrap()["content"],
        eq("The prediction economy is temporarily unavailable. Please try again.")
    );
    let events = recorder.0.lock().unwrap();
    assert_that!(
        events.as_slice(),
        elements_are![
            matches_pattern!(AuditEvent::InteractionCompleted {
                interaction_id: eq(&125),
                stage: eq(&Stage::Acknowledge),
                outcome: eq(&Outcome::Succeeded),
                ..
            }),
            matches_pattern!(AuditEvent::QueryCompleted {
                guild: eq(&GuildId(10)),
                interaction_id: eq(&125),
                query: eq(&QueryKind::Balance),
                stage: eq(&Stage::Query),
                outcome: matches_pattern!(Outcome::Failed(matches_pattern!(Failure {
                    category: eq(&FailureCategory::Connection),
                    ..
                }))),
                ..
            }),
            matches_pattern!(AuditEvent::InteractionCompleted {
                guild: some(eq(&GuildId(10))),
                interaction_id: eq(&125),
                stage: eq(&Stage::Deliver),
                outcome: matches_pattern!(Outcome::Failed(matches_pattern!(Failure {
                    category: eq(&FailureCategory::Discord),
                    sqlstate: none(),
                    http_status: some(eq(&503)),
                    discord_code: some(eq(&10015))
                })))
            })
        ]
    );
}

#[googletest::test]
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
    assert_that!(requests.len(), eq(3));
    assert_that!(
        requests[0].body_json::<serde_json::Value>().unwrap()["type"],
        eq(5)
    );
    assert_that!(
        requests[0].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
        eq(64)
    );
    assert_that!(requests[1].method.as_str(), eq("PATCH"));
    assert_that!(
        requests[1].body_json::<serde_json::Value>().unwrap()["content"],
        eq("This control belongs to another member or server. Run /market list or /market create.")
    );
    assert_that!(requests[2].method.as_str(), eq("POST"));
    assert_that!(
        requests[2].body_json::<serde_json::Value>().unwrap()["type"],
        eq(4)
    );
    assert_that!(
        requests[2].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
        eq(64)
    );
    assert_that!(
        requests[2].body_json::<serde_json::Value>().unwrap()["data"]["content"],
        eq("Choose an option from the market menu.")
    );
    let events = recorder.0.lock().unwrap();
    for id in [126, 127] {
        assert_that!(
            events.as_slice(),
            contains(matches_pattern!(AuditEvent::InteractionCompleted {
                guild: some(eq(&GuildId(10))),
                interaction_id: eq(&id),
                stage: eq(&Stage::Validate),
                outcome: matches_pattern!(Outcome::Rejected(eq(&Rejection::InvalidInput)))
            }))
        );
        assert_that!(
            events.as_slice(),
            contains(matches_pattern!(AuditEvent::InteractionCompleted {
                guild: some(eq(&GuildId(10))),
                interaction_id: eq(&id),
                stage: eq(&Stage::Deliver),
                outcome: eq(&Outcome::Succeeded)
            }))
        );
    }
}

#[googletest::test]
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
    assert_that!(
        requests[0].body_json::<serde_json::Value>().unwrap()["type"],
        eq(4)
    );
    assert_that!(
        requests[0].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
        eq(64)
    );
    assert_that!(
        requests[0].body_json::<serde_json::Value>().unwrap()["data"]["content"],
        eq("The prediction economy is temporarily unavailable. Please try again.")
    );
    let events = recorder.0.lock().unwrap();
    assert_that!(
        events.as_slice(),
        elements_are![
            matches_pattern!(AuditEvent::QueryCompleted {
                guild: eq(&GuildId(10)),
                interaction_id: eq(&128),
                query: eq(&QueryKind::Component),
                outcome: matches_pattern!(Outcome::Failed(anything())),
                stage: eq(&Stage::Query),
                ..
            }),
            matches_pattern!(AuditEvent::InteractionCompleted {
                interaction_id: eq(&128),
                stage: eq(&Stage::Deliver),
                outcome: eq(&Outcome::Succeeded),
                ..
            }),
            matches_pattern!(AuditEvent::RegistrationCompleted {
                guild: eq(&GuildId(10)),
                stage: eq(&Stage::Register),
                outcome: eq(&Outcome::Succeeded)
            })
        ]
    );
}

#[googletest::test]
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
        42.into(),
        crate::domain::Policy {
            amount: Points(100),
            interval: 86400,
        },
        recorder.clone(),
    ));
    super::grant_worker(store, receiver).await;
    assert_that!(
        *recorder.events.lock().unwrap(),
        eq(&vec![AuditEvent::GrantFailed {
            guild: None,
            stage: Stage::Discover,
            outcome: Outcome::Failed(Failure {
                category: FailureCategory::Connection,
                sqlstate: None,
                http_status: None,
                discord_code: None
            }),
        }])
    );
}

#[googletest::test]
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
        42.into(),
        crate::domain::Policy {
            amount: Points(100),
            interval: 86_400,
        },
        recorder.clone(),
    ));

    let result = super::run(store, "unused".into()).await;

    assert_that!(
        result,
        err(matches_pattern!(super::DiscordError::Store(
            matches_pattern!(crate::store::StoreError::Database(matches_pattern!(
                sqlx::Error::PoolClosed
            )))
        )))
    );
    assert_that!(
        recorder.0.lock().unwrap().as_slice(),
        elements_are![matches_pattern!(AuditEvent::Lifecycle {
            kind: eq(&LifecycleKind::Startup),
            application_id: some(eq(&ApplicationId(42))),
            stage: eq(&Stage::Acquire),
            outcome: matches_pattern!(Outcome::Failed(matches_pattern!(Failure {
                category: eq(&FailureCategory::Connection),
                sqlstate: none(),
                http_status: none(),
                discord_code: none()
            })))
        })]
    );
}

#[googletest::test]
#[tokio::test]
async fn query_success_and_corrupt_history_have_distinct_operational_outcomes() {
    use crate::audit::{AuditEvent, Failure, FailureCategory, Outcome, QueryKind};
    let recorder = AuditRecorder::default();
    let view = std::sync::Arc::new(crate::store::View {
        revision: EventRevision(5),
        state: Default::default(),
    });
    let result = super::read_query(
        &recorder,
        10.into(),
        130,
        QueryKind::List,
        async { Ok(view) },
        None,
    )
    .await
    .unwrap();
    assert_that!(result.revision.0, eq(5));
    let read = async {
        Err(crate::store::StoreError::Domain(
            crate::domain::DomainError::Invalid("corrupt state"),
        ))
    };
    assert_that!(
        super::read_query(&recorder, 10.into(), 131, QueryKind::Show, read, None).await,
        err(anything())
    );
    assert_that!(
        recorder.0.lock().unwrap().as_slice(),
        elements_are![
            matches_pattern!(AuditEvent::QueryCompleted {
                interaction_id: eq(&130),
                outcome: eq(&Outcome::Succeeded),
                ..
            }),
            matches_pattern!(AuditEvent::QueryCompleted {
                interaction_id: eq(&131),
                outcome: matches_pattern!(Outcome::Failed(matches_pattern!(Failure {
                    category: eq(&FailureCategory::History),
                    ..
                }))),
                ..
            })
        ]
    );
}

#[googletest::test]
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

    assert_that!(result, err(matches_pattern!(super::DiscordError::Gateway)));
    let events = recorder.0.lock().unwrap();
    let lifecycle_events: Vec<_> = events
        .iter()
        .filter(|event| matches!(event, AuditEvent::Lifecycle { .. }))
        .collect();
    assert_that!(
        lifecycle_events,
        eq(&vec![&AuditEvent::Lifecycle {
            kind: LifecycleKind::Shutdown,
            application_id: Some(ApplicationId(42)),
            stage: Stage::Shutdown,
            outcome: Outcome::Failed(Failure {
                category: FailureCategory::Discord,
                sqlstate: None,
                http_status: Some(403),
                discord_code: Some(50001),
            }),
        }])
    );
}

#[googletest::test]
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
        assert_that!(requests.len(), eq(1));
        assert_that!(requests[0].method.as_str(), eq("POST"));
        assert_that!(requests[0].url.path(), eq("/api/v10/channels/20/messages"));
        assert_that!(
            requests[0].body_json::<serde_json::Value>().unwrap()["content"]
                .as_str()
                .unwrap(),
            contains_substring("/market help")
        );
        assert_that!(
            requests[0].body_json::<serde_json::Value>().unwrap()["allowed_mentions"]["parse"],
            eq(&serde_json::json!([]))
        );
        assert_that!(
            requests[0].body_json::<serde_json::Value>().unwrap()["allowed_mentions"]["replied_user"],
            eq(false)
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
        assert_that!(
            *recorder.0.lock().unwrap(),
            eq(&vec![AuditEvent::MentionReplyCompleted {
                guild: GuildId(10),
                channel_id: ChannelId(20),
                message_id: 123,
                outcome,
                stage: Stage::Deliver,
            }])
        );
    }
}

#[googletest::test]
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

    assert_that!(server.received_requests().await.unwrap(), is_empty());
    assert_that!(recorder.0.lock().unwrap().as_slice(), is_empty());
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

#[googletest::test]
#[tokio::test]
async fn destination_validation_uses_real_http_and_effective_bot_permissions() {
    let server = MockServer::start().await;
    mount_destination_reads(&server, channel_json(10, 0, 0), 1024 | 2048).await;

    assert_that!(
        super::announcements::validate_destination(
            &discord_http(&server),
            10.into(),
            99.into(),
            55.into()
        )
        .await,
        eq(Ok(()))
    );
    assert_that!(server.received_requests().await.unwrap().len(), eq(3));
}

#[googletest::test]
#[tokio::test]
async fn destination_validation_rejects_cross_guild_non_text_missing_permissions_and_reads() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/55"))
        .respond_with(ResponseTemplate::new(200).set_body_json(channel_json(11, 0, 0)))
        .expect(1)
        .mount(&server)
        .await;
    assert_that!(
        super::announcements::validate_destination(
            &discord_http(&server),
            10.into(),
            99.into(),
            55.into()
        )
        .await,
        eq(Err("Choose a text channel in this server."))
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/55"))
        .respond_with(ResponseTemplate::new(200).set_body_json(channel_json(10, 2, 0)))
        .expect(1)
        .mount(&server)
        .await;
    assert_that!(
        super::announcements::validate_destination(
            &discord_http(&server),
            10.into(),
            99.into(),
            55.into()
        )
        .await,
        eq(Err("Choose a text channel in this server."))
    );

    let server = MockServer::start().await;
    mount_destination_reads(&server, channel_json(10, 0, 2048), 1024 | 2048).await;
    assert_that!(
        super::announcements::validate_destination(
            &discord_http(&server),
            10.into(),
            99.into(),
            55.into()
        )
        .await,
        eq(Err(
            "I need View Channel and Send Messages in that channel."
        ))
    );

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v10/channels/55"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    assert_that!(
        super::announcements::validate_destination(
            &discord_http(&server),
            10.into(),
            99.into(),
            55.into()
        )
        .await,
        eq(Err("I could not verify that channel. Please try again."))
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

#[googletest::test]
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
    assert_that!(requests.len(), eq(2));
    assert_that!(
        requests[0].body_json::<serde_json::Value>().unwrap()["type"],
        eq(5)
    );
    assert_that!(
        requests[0].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
        eq(64)
    );
    let response = requests[1].body_json::<serde_json::Value>().unwrap();
    assert_that!(
        response["content"],
        eq("The prediction economy is temporarily unavailable. Please try again.")
    );
    assert_that!(
        response["allowed_mentions"]["parse"],
        eq(&serde_json::json!([]))
    );
    assert_that!(
        requests.iter().any(|request| {
            request.method.as_str() == "POST"
                && request.url.path() == "/api/v10/channels/55/messages"
        }),
        eq(false)
    );
}

#[googletest::test]
fn announcement_status_and_receipts_explain_delivery_state() {
    let status = crate::announcements::AnnouncementStatus {
        channel_id: Some(ChannelId(55)),
        enabled: true,
        version: ConfigurationVersion(3),
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
        assert_that!(
            rendered,
            contains_substring(detail),
            "missing status detail: {detail}"
        );
    }

    let changed =
        super::announcements::configuration_receipt("Announcements enabled for <#55>.", false);
    assert_that!(
        changed,
        contains_substring("Pending announcements will use this destination.")
    );
    assert_that!(changed, contains_substring("already in flight"));
    let disabled = super::announcements::configuration_receipt("Announcements disabled.", true);
    assert_that!(
        disabled,
        contains_substring("Pending announcements were discarded.")
    );
    assert_that!(disabled, contains_substring("already in flight"));
}

#[googletest::test]
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
        assert_that!(requests.len(), eq(2));
        assert_that!(
            requests[0].body_json::<serde_json::Value>().unwrap()["type"],
            eq(5)
        );
        assert_that!(
            requests[0].body_json::<serde_json::Value>().unwrap()["data"]["flags"],
            eq(64)
        );
        let response = requests[1].body_json::<serde_json::Value>().unwrap();
        assert_that!(
            response["content"],
            eq("The prediction economy is temporarily unavailable. Please try again.")
        );
        assert_that!(
            response["allowed_mentions"]["parse"],
            eq(&serde_json::json!([]))
        );
        assert_that!(
            recorder.0.lock().unwrap().as_slice(),
            not(contains(matches_pattern!(
                AuditEvent::QueryCompleted { .. }
            ))),
            "{action} fell through to the economy query path"
        );
    }
}

#[googletest::test]
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
    assert_that!(result, err(matches_pattern!(super::DiscordError::Gateway)));
    assert_that!(
        recorder.0.lock().unwrap().as_slice(),
        contains(matches_pattern!(AuditEvent::Lifecycle {
            kind: eq(&LifecycleKind::Shutdown),
            stage: eq(&Stage::AnnouncementWorker),
            outcome: matches_pattern!(Outcome::Failed(anything())),
            ..
        }))
    );
}

#[googletest::test]
fn market_cards_show_stake_weighted_percentages() {
    use crate::domain::{Actor, Bet, Market, State, Status};
    use crate::store::View;

    // Repeated bets count by points, not by the number of bets or bettors.
    for (stakes, expected) in [
        (vec![(0, 25), (0, 50), (1, 25)], ["75.0%", "25.0%"]),
        (vec![(0, 1), (1, 2)], ["33.3%", "66.7%"]),
        (vec![(0, 5)], ["100.0%", "0.0%"]),
        (vec![], ["N/A (no bets)", "N/A (no bets)"]),
        (vec![(0, i64::MAX - 1), (1, 1)], ["100.0%", "0.0%"]),
    ] {
        let market = Market {
            creator: UserId(20),
            question: "Will it rain?".into(),
            options: vec!["Yes".into(), "No".into()],
            closes_at: 2000,
            created_at: 1000,
            status: Status::Open,
            total_staked: Points(stakes.iter().map(|(_, amount)| amount).sum()),
            bets: stakes
                .into_iter()
                .map(|(outcome, amount)| Bet {
                    user_id: UserId(20),
                    outcome: OutcomeIndex(outcome),
                    amount: Points(amount),
                })
                .collect(),
        };
        let mut state = State::default();
        state.markets.insert("rain".into(), market);
        let view = View {
            revision: EventRevision(1),
            state,
        };
        let actor = Actor {
            user_id: UserId(20),
            moderator: false,
            bot: false,
        };
        let payload = serde_json::to_value(
            super::ui::query(
                &view,
                &Action::Show { id: "rain".into() },
                actor,
                GuildId(10),
                1001,
            )
            .edit(),
        )
        .unwrap();
        let description = payload["embeds"][0]["description"].as_str().unwrap();
        for (label, percentage) in ["Yes", "No"].into_iter().zip(expected) {
            let line = description
                .lines()
                .find(|line| line.contains(label))
                .unwrap();
            assert_that!(
                line,
                contains_substring(format!("{percentage} implied chance"))
            );
        }
    }
}

#[googletest::test]
fn bet_without_arguments_opens_a_private_widget() {
    let (guild, actor, action) = parse(&input("bet", vec![])).unwrap();
    let view = crate::store::View {
        state: crate::domain::State::default(),
        revision: EventRevision(0),
    };
    let panel =
        serde_json::to_value(super::ui::query(&view, &action, actor, guild, 1000).message())
            .unwrap();
    assert_that!(panel["flags"], eq(64));
    assert_that!(
        panel["content"].as_str().unwrap(),
        contains_substring("No markets")
    );
    assert_that!(
        parse(&input("bet", vec![text("id", "market")])),
        err(anything())
    );
}

#[googletest::test]
fn legacy_betting_controls_redirect_to_market_bet() {
    let view = ui_view();
    assert_that!(
        super::ui::component(
            10.into(),
            ui_actor(),
            "pm:10:20:bet:78e82954-4c67-4e0d-8c80-8ab95a527ae5",
            &["1".into()],
            &view,
            2_000
        ),
        err(contains_substring("/market bet"))
    );
    assert_that!(
        super::ui::modal_command(
            10.into(),
            ui_actor(),
            "pm:10:20:stake:78e82954-4c67-4e0d-8c80-8ab95a527ae5:1",
            vec![text("amount", "25")],
            2_000
        ),
        err(contains_substring("/market bet"))
    );
}

#[googletest::test]
fn unavailable_market_cards_do_not_advertise_betting() {
    use crate::domain::Status;

    let id = "78e82954-4c67-4e0d-8c80-8ab95a527ae5";
    for (status, now) in [
        (Status::Open, 10_000),
        (
            Status::Resolved {
                outcome: OutcomeIndex(0),
                refunded: false,
            },
            2_000,
        ),
        (Status::Cancelled, 2_000),
    ] {
        let mut view = ui_view();
        view.state.markets.get_mut(id).unwrap().status = status;
        let card = super::ui::query(
            &view,
            &Action::Show { id: id.into() },
            ui_actor(),
            10.into(),
            now,
        );
        assert_that!(card.content, not(contains_substring("/market bet")));
        assert_that!(card.components, is_empty());
        let selected = serde_json::to_value(
            super::ui::component(
                10.into(),
                ui_actor(),
                "pm:10:20:list",
                &[id.into()],
                &view,
                now,
            )
            .unwrap(),
        )
        .unwrap();
        assert_that!(
            selected["data"]["content"].as_str().unwrap(),
            not(contains_substring("/market bet"))
        );
    }
}
