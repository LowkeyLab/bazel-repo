use super::{Action, panel, parse, picker, prefix, preview, stake_modal};
use crate::{
    discord::ui::Panel,
    domain::{Account, Actor, Command, Market, State, Status},
    store::View,
    types::{EventRevision, MarketId, OutcomeIndex, Points, UserId},
};
use googletest::{
    assert_that,
    matchers::{anything, contains_substring, eq, err, le, matches_pattern},
};
use serde_json::Value;
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
    state.accounts.insert(
        actor().user_id,
        Account {
            balance: Points(i64::MAX),
            next_grant: 3000,
        },
    );
    for n in 0..count {
        state.markets.insert(
            uuid::Uuid::from_u128(n as u128).to_string().into(),
            Market {
                creator: actor().user_id,
                question: format!("Market {n}?"),
                options: vec!["Yes".into(), "No".into()],
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
fn json(panel: Panel) -> Value {
    serde_json::to_value(panel.message()).unwrap()
}
fn click(control: &Value, values: &[&str]) -> Action {
    parse(
        u64::MAX.into(),
        actor(),
        control["custom_id"].as_str().unwrap(),
        &if values.is_empty() {
            ComponentInteractionDataKind::Button
        } else {
            ComponentInteractionDataKind::StringSelect {
                values: values.iter().map(ToString::to_string).collect(),
            }
        },
    )
    .unwrap()
}
#[googletest::test]
fn pagination_reaches_all_open_markets_and_clamps_stale_pages() {
    let mut view = view(26);
    let first = json(picker(&view, actor(), u64::MAX.into(), 1000, 0));
    assert_that!(
        first["components"][0]["components"][0]["options"]
            .as_array()
            .unwrap()
            .len(),
        eq(25)
    );
    let next = click(&first["components"][1]["components"][0], &[]);
    let second = json(panel(&view, actor(), u64::MAX.into(), 1000, &next).unwrap());
    assert_that!(
        second["components"][0]["components"][0]["options"][0]["label"],
        eq("Market 25?")
    );
    view.state
        .markets
        .remove(&MarketId::from(uuid::Uuid::from_u128(25).to_string()));
    let recovered = json(panel(&view, actor(), u64::MAX.into(), 1000, &next).unwrap());
    let clamped = json(picker(&view, actor(), u64::MAX.into(), 1000, usize::MAX));
    for page in [recovered, clamped] {
        assert_that!(
            page["content"].as_str().unwrap(),
            contains_substring("Page 1 of 1")
        );
        assert_that!(
            page["components"][0]["components"][0]["options"]
                .as_array()
                .unwrap()
                .len(),
            eq(25)
        );
        assert_that!(
            page["components"][1]["components"]
                .as_array()
                .unwrap()
                .len(),
            eq(1)
        );
        assert_that!(
            click(&page["components"][1]["components"][0], &[]),
            eq(&Action::Cancel)
        );
    }
    assert_that!(
        picker(&view, actor(), u64::MAX.into(), 2000, 0).content,
        contains_substring("No markets")
    );
}
#[googletest::test]
fn maximum_confirmation_fits_discord_and_round_trips_without_losing_stake_or_identity() {
    let mut view = view(1);
    let market = view.state.markets.pop_first().unwrap().1;
    let id: MarketId = uuid::Uuid::from_u128(u128::MAX).to_string().into();
    view.state.markets.insert(id.clone(), market);
    let modal = serde_json::to_value(
        stake_modal(&view, actor(), u64::MAX.into(), 1000, &id, OutcomeIndex(1)).unwrap(),
    )
    .unwrap();
    let confirmation = json(
        preview(
            &view,
            actor(),
            u64::MAX.into(),
            1000,
            modal["custom_id"].as_str().unwrap(),
            &i64::MAX.to_string(),
            u64::MAX,
        )
        .unwrap(),
    );
    for control in confirmation["components"][0]["components"]
        .as_array()
        .unwrap()
    {
        assert_that!(control["custom_id"].as_str().unwrap().len(), le(100));
    }
    let action = click(&confirmation["components"][0]["components"][0], &[]);
    assert_that!(
        action,
        eq(&Action::Confirm {
            command: Command::Bet {
                id: id.clone(),
                outcome: OutcomeIndex(1),
                amount: Points(i64::MAX)
            },
            submission: u64::MAX
        })
    );
    let back = click(&confirmation["components"][0]["components"][1], &[]);
    assert_that!(back, eq(&Action::Market(id)));
    let cancel = click(&confirmation["components"][0]["components"][2], &[]);
    assert_that!(cancel, eq(&Action::Cancel));
    assert_that!(
        panel(&view, actor(), u64::MAX.into(), 1000, &cancel)
            .unwrap()
            .components
            .len(),
        eq(0)
    );
}
#[googletest::test]
fn stake_preview_rejects_invalid_amounts_foreign_members_and_stale_markets() {
    let mut view = view(1);
    let id = view.state.markets.first_key_value().unwrap().0.clone();
    view.state
        .accounts
        .get_mut(&actor().user_id)
        .unwrap()
        .balance = Points(50);
    let modal = serde_json::to_value(
        stake_modal(&view, actor(), u64::MAX.into(), 1000, &id, OutcomeIndex(0)).unwrap(),
    )
    .unwrap();
    let custom_id = modal["custom_id"].as_str().unwrap();
    for amount in ["0", "-1", "1.5", "text", "51", "9223372036854775808"] {
        assert_that!(
            preview(
                &view,
                actor(),
                u64::MAX.into(),
                1000,
                custom_id,
                amount,
                123
            )
            .is_err(),
            eq(true)
        );
    }
    assert_that!(
        preview(&view, actor(), 1.into(), 1000, custom_id, "10", 123).is_err(),
        eq(true)
    );
    let other = Actor {
        user_id: UserId(1),
        ..actor()
    };
    assert_that!(
        preview(&view, other, u64::MAX.into(), 1000, custom_id, "10", 123).is_err(),
        eq(true)
    );
    assert_that!(
        preview(&view, actor(), u64::MAX.into(), 2000, custom_id, "10", 123).is_err(),
        eq(true)
    );
    view.state.markets.get_mut(&id).unwrap().status = Status::Cancelled;
    assert_that!(
        stake_modal(&view, actor(), u64::MAX.into(), 1000, &id, OutcomeIndex(0)).is_err(),
        eq(true)
    );
}
#[googletest::test]
fn controls_reject_wrong_scope_kind_and_malformed_values() {
    let view = view(1);
    let picker = json(picker(&view, actor(), u64::MAX.into(), 1000, 0));
    let control = picker["components"][0]["components"][0]["custom_id"]
        .as_str()
        .unwrap();
    let kind = ComponentInteractionDataKind::StringSelect {
        values: vec![uuid::Uuid::nil().to_string()],
    };
    assert_that!(parse(1.into(), actor(), control, &kind), err(anything()));
    assert_that!(
        parse(
            u64::MAX.into(),
            Actor {
                bot: true,
                ..actor()
            },
            control,
            &kind
        ),
        err(anything())
    );
    assert_that!(
        parse(
            u64::MAX.into(),
            actor(),
            control,
            &ComponentInteractionDataKind::Button
        ),
        err(anything())
    );
    assert_that!(
        parse(u64::MAX.into(), actor(), control, &kind).unwrap(),
        matches_pattern!(Action::Market(_))
    );
    for suffix in ["c:0:0:0:1", "c:0:10:1:1", "c:0:0:1:0", "c:!:0:1:1", "p:-1"] {
        assert_that!(
            parse(
                u64::MAX.into(),
                actor(),
                &format!("{}:{suffix}", prefix(u64::MAX.into(), actor())),
                &ComponentInteractionDataKind::Button
            ),
            err(anything())
        );
    }
}
