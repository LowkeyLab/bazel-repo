use super::{Panel, panel, parse, picker, ui};
use crate::{
    domain::State,
    types::{EventRevision, OutcomeIndex, Points},
};
use crate::{
    domain::{Actor, Market, Status},
    store::View,
    types::{GuildId, UserId},
};
use googletest::{
    assert_that,
    matchers::{contains_substring, eq, le, not},
};
use serde_json::{Value, json};
use serenity::all::ComponentInteractionDataKind;

fn reader() -> Actor {
    Actor {
        user_id: UserId(u64::MAX),
        bot: false,
        moderator: false,
    }
}

fn view() -> View {
    let mut state = State::default();
    for n in 0..26 {
        state.markets.insert(
            format!("00000000-0000-4000-8000-{n:012}").into(),
            Market {
                creator: 7.into(),
                question: format!("Market {n}?"),
                options: vec!["Yes".into(), "No".into()],
                created_at: 1,
                closes_at: if n == 1 { 100 } else { 300 },
                status: match n {
                    2 => Status::Resolved {
                        outcome: OutcomeIndex(0),
                        refunded: true,
                    },
                    3 => Status::Cancelled,
                    _ => Status::Open,
                },
                bets: vec![],
                total_staked: Points(0),
                resolvers: if n == 2 {
                    (1..=26).map(UserId).collect()
                } else {
                    Default::default()
                },
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
fn navigate(view: &View, control: &Value, values: &[&str]) -> Value {
    let kind = if control["type"] == 2 {
        ComponentInteractionDataKind::Button
    } else {
        ComponentInteractionDataKind::StringSelect {
            values: values.iter().map(ToString::to_string).collect(),
        }
    };
    let id = control["custom_id"].as_str().unwrap();
    assert_that!(id.len(), le(100));
    let action = parse(GuildId(u64::MAX), reader(), id, &kind).unwrap();
    json_panel(panel(view, GuildId(u64::MAX), reader(), 200, action).unwrap())
}
fn control<'a>(panel: &'a Value, label: &str) -> &'a Value {
    panel["components"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row["components"].as_array().unwrap())
        .find(|c| c["label"] == label)
        .unwrap()
}

#[googletest::test]
fn browsing_reaches_all_markets_and_assignments_including_completed_and_empty_sets() {
    let view = view();
    let first = json_panel(picker(&view, GuildId(u64::MAX), reader(), 200, 0));
    assert_that!(first["flags"], eq(64));
    assert_that!(first["allowed_mentions"]["parse"], eq(&json!([])));
    let menu = &first["components"][0]["components"][0];
    assert_that!(menu["options"].as_array().unwrap().len(), eq(25));
    for status in ["Open", "Awaiting settlement", "Resolved", "Cancelled"] {
        assert_that!(menu.to_string(), contains_substring(status));
    }
    let last = navigate(&view, control(&first, "Next markets"), &[]);
    assert_that!(
        last["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
    let id = "00000000-0000-4000-8000-000000000025";
    let empty = navigate(&view, &last["components"][0]["components"][0], &[id]);
    assert_that!(
        empty["embeds"][0]["description"].as_str().unwrap(),
        contains_substring("0 total; page 1 of 1):\nNone")
    );
    let back = navigate(&view, control(&empty, "Back to markets"), &[]);
    assert_that!(
        back["content"].as_str().unwrap(),
        contains_substring("Page 2 of 2")
    );
    let first_again = navigate(&view, control(&back, "Previous markets"), &[]);
    let populated = navigate(
        &view,
        &first_again["components"][0]["components"][0],
        &["00000000-0000-4000-8000-000000000002"],
    );
    let text = populated["embeds"][0]["description"].as_str().unwrap();
    assert_that!(text, contains_substring("Status: Resolved"));
    assert_that!(text, contains_substring("<@25>"));
    assert_that!(text, not(contains_substring("<@26>")));
    let final_assignments = navigate(&view, control(&populated, "Next assignments"), &[]);
    let text = final_assignments["embeds"][0]["description"]
        .as_str()
        .unwrap();
    assert_that!(text, contains_substring("26 total; page 2 of 2"));
    assert_that!(text, contains_substring("<@26>"));
    assert_that!(text, not(contains_substring("<@25>")));
    let previous = navigate(
        &view,
        control(&final_assignments, "Previous assignments"),
        &[],
    );
    assert_that!(
        previous["embeds"][0]["description"].as_str().unwrap(),
        contains_substring("<@1>")
    );
}

#[googletest::test]
fn foreign_and_malformed_listing_controls_cannot_navigate() {
    let view = view();
    let first = json_panel(picker(&view, GuildId(u64::MAX), reader(), 200, 0));
    let id = first["components"][0]["components"][0]["custom_id"]
        .as_str()
        .unwrap();
    let selection = ComponentInteractionDataKind::StringSelect {
        values: vec!["00000000-0000-4000-8000-000000000002".into()],
    };
    for (guild, actor) in [
        (GuildId(1), reader()),
        (
            GuildId(u64::MAX),
            Actor {
                user_id: 1.into(),
                ..reader()
            },
        ),
        (
            GuildId(u64::MAX),
            Actor {
                bot: true,
                ..reader()
            },
        ),
    ] {
        assert_that!(parse(guild, actor, id, &selection).is_err(), eq(true));
    }
    assert_that!(
        parse(
            GuildId(u64::MAX),
            reader(),
            id,
            &ComponentInteractionDataKind::Button
        )
        .is_err(),
        eq(true)
    );
    let bad = format!("{}:lp:4294967296", ui::prefix(GuildId(u64::MAX), reader()));
    assert_that!(
        parse(
            GuildId(u64::MAX),
            reader(),
            &bad,
            &ComponentInteractionDataKind::Button
        )
        .is_err(),
        eq(true)
    );
}
