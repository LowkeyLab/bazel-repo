use super::super::ui::{self, Panel};
use super::{Action, panel, parse, picker};
use crate::{
    domain::{Actor, Command, Market, State, Status},
    store::View,
    types::{EventRevision, GuildId, OutcomeIndex, Points, UserId},
};
use googletest::{
    assert_that,
    matchers::{anything, contains_substring, eq, err, le, matches_pattern},
};
use serde_json::{Value, json};
use serenity::all::ComponentInteractionDataKind;

fn actor() -> Actor {
    Actor {
        user_id: UserId(u64::MAX),
        moderator: false,
        bot: false,
    }
}

fn view(count: usize) -> View {
    let mut state = State::default();
    for n in 0..count {
        state.markets.insert(
            format!("00000000-0000-4000-8000-{n:012}").into(),
            Market {
                creator: actor().user_id,
                question: format!("Market {n}?"),
                options: vec!["Red".into(), "Blue".into()],
                closes_at: 2000,
                created_at: 1000,
                status: Status::Open,
                bets: vec![],
                total_staked: Points(0),
            },
        );
    }
    View {
        state,
        revision: EventRevision(0),
    }
}

fn json_panel(panel: Panel) -> Value {
    serde_json::to_value(panel.message()).unwrap()
}

fn click(control: &Value, values: &[&str]) -> Action {
    let kind = if control["type"] == 2 {
        ComponentInteractionDataKind::Button
    } else {
        ComponentInteractionDataKind::StringSelect {
            values: values.iter().map(ToString::to_string).collect(),
        }
    };
    parse(
        u64::MAX.into(),
        actor(),
        control["custom_id"].as_str().unwrap(),
        &kind,
    )
    .unwrap()
}

#[googletest::test]
fn pagination_reaches_every_market_and_recovers_after_the_last_page_shrinks() {
    let mut view = view(26);
    let first = json_panel(picker(&view, actor(), u64::MAX.into(), 2000, 0));
    assert_that!(
        first["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(25)
    );
    let next = click(&first["components"][1]["components"][0], &[]);
    let second = json_panel(panel(&view, actor(), u64::MAX.into(), 2000, &next).unwrap());
    assert_that!(
        second["components"][0]["components"][0]["options"][0]["label"],
        eq("Market 25?")
    );
    assert_that!(
        second["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
    let previous = click(&second["components"][1]["components"][0], &[]);
    assert_that!(
        json_panel(panel(&view, actor(), u64::MAX.into(), 2000, &previous).unwrap()),
        eq(&first)
    );
    view.state
        .markets
        .remove("00000000-0000-4000-8000-000000000025");
    let refreshed = json_panel(panel(&view, actor(), u64::MAX.into(), 2000, &next).unwrap());
    assert_that!(
        refreshed["content"].as_str().unwrap(),
        contains_substring("Page 1 of 1")
    );
    assert_that!(refreshed["components"].as_array().unwrap().len(), eq(1));
}

#[googletest::test]
fn selecting_and_going_back_never_produces_a_settlement_command() {
    let view = view(1);
    let picker = json_panel(picker(&view, actor(), u64::MAX.into(), 2000, 0));
    let menu = &picker["components"][0]["components"][0];
    let id = menu["options"][0]["value"].as_str().unwrap();
    let market = click(menu, &[id]);
    assert_that!(market, matches_pattern!(Action::Market { .. }));
    let outcomes = json_panel(panel(&view, actor(), u64::MAX.into(), 2000, &market).unwrap());
    let selected = click(&outcomes["components"][0]["components"][0], &["1"]);
    assert_that!(selected, matches_pattern!(Action::Outcome { .. }));
    let confirm = json_panel(panel(&view, actor(), u64::MAX.into(), 2000, &selected).unwrap());
    assert_that!(
        confirm["content"].as_str().unwrap(),
        contains_substring("Blue")
    );
    let back = click(&confirm["components"][0]["components"][1], &[]);
    assert_that!(
        json_panel(panel(&view, actor(), u64::MAX.into(), 2000, &back).unwrap()),
        eq(&outcomes)
    );
    let confirm_action = click(&confirm["components"][0]["components"][0], &[]);
    assert_that!(
        confirm_action,
        eq(&Action::Confirm(Command::Resolve {
            id: id.into(),
            outcome: OutcomeIndex(1)
        }))
    );
    let back_to_markets = click(&outcomes["components"][1]["components"][0], &[]);
    assert_that!(
        json_panel(panel(&view, actor(), u64::MAX.into(), 2000, &back_to_markets).unwrap()),
        eq(&picker)
    );
}

#[googletest::test]
fn stale_selections_recheck_status_time_and_permission_before_confirmation() {
    let mut view = view(1);
    let id = view.state.markets.keys().next().unwrap().clone();
    let selection = Action::Outcome {
        id: id.clone(),
        page: 0,
        outcome: OutcomeIndex(1),
    };
    for status in [
        Status::Cancelled,
        Status::Resolved {
            outcome: OutcomeIndex(0),
            refunded: false,
        },
    ] {
        view.state.markets.get_mut(&id).unwrap().status = status;
        assert_that!(
            panel(&view, actor(), u64::MAX.into(), 2000, &selection).err(),
            eq(Some(
                "This market is no longer eligible for you to resolve. Run /market resolve again."
            ))
        );
    }
    view.state.markets.get_mut(&id).unwrap().status = Status::Open;
    assert_that!(
        panel(&view, actor(), u64::MAX.into(), 1999, &selection).is_err(),
        eq(true)
    );
    view.state.markets.get_mut(&id).unwrap().creator = UserId(7);
    assert_that!(
        panel(
            &view,
            Actor {
                moderator: true,
                ..actor()
            },
            u64::MAX.into(),
            2000,
            &selection
        )
        .is_ok(),
        eq(true)
    );
    assert_that!(
        panel(&view, actor(), u64::MAX.into(), 2000, &selection).is_err(),
        eq(true)
    );
    view.state.markets.clear();
    assert_that!(
        panel(&view, actor(), u64::MAX.into(), 2000, &selection).is_err(),
        eq(true)
    );
}

#[googletest::test]
fn resolution_controls_reject_foreign_scopes_and_malformed_interactions() {
    let control = format!(
        "{}:resolve:c:00000000-0000-4000-8000-000000000000:1",
        ui::prefix(u64::MAX.into(), actor())
    );
    for (guild, actor) in [
        (GuildId(0), actor()),
        (GuildId(10), actor()),
        (
            GuildId(u64::MAX),
            Actor {
                user_id: UserId(7),
                ..actor()
            },
        ),
        (
            GuildId(u64::MAX),
            Actor {
                bot: true,
                ..actor()
            },
        ),
    ] {
        assert_that!(
            parse(
                guild,
                actor,
                &control,
                &ComponentInteractionDataKind::Button
            ),
            err(anything())
        );
    }
    assert_that!(
        parse(
            u64::MAX.into(),
            actor(),
            &control,
            &ComponentInteractionDataKind::StringSelect {
                values: vec!["1".into()]
            }
        ),
        err(anything())
    );
    for (suffix, values) in [
        ("m:0", vec![]),
        ("m:0", vec!["a", "b"]),
        ("o:market:0", vec!["-1"]),
        ("o:market:0", vec!["10"]),
        ("m:invalid", vec!["market"]),
    ] {
        assert_that!(
            parse(
                u64::MAX.into(),
                actor(),
                &format!("{}:resolve:{suffix}", ui::prefix(u64::MAX.into(), actor())),
                &ComponentInteractionDataKind::StringSelect {
                    values: values.into_iter().map(str::to_owned).collect()
                }
            ),
            err(anything())
        );
    }
}

#[googletest::test]
fn obsolete_page_numbers_cannot_produce_overlong_discord_controls() {
    let view = view(1);
    let id = view.state.markets.keys().next().unwrap().clone();
    let selection = Action::Market {
        id,
        page: usize::MAX,
    };
    let response = json_panel(panel(&view, actor(), u64::MAX.into(), 2000, &selection).unwrap());
    for row in response["components"].as_array().unwrap() {
        for control in row["components"].as_array().unwrap() {
            assert_that!(control["custom_id"].as_str().unwrap().len(), le(100));
        }
    }
    assert_that!(response["allowed_mentions"]["parse"], eq(&json!([])));
}
