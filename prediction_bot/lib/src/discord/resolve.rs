//! Private resolution navigation. Only a confirmed action becomes a store command.
use crate::{
    domain::{Actor, Command, Market, Status},
    store::View,
    types::{GuildId, MarketId, OutcomeIndex},
};
use serenity::{
    all::{ButtonStyle, ComponentInteractionDataKind},
    builder::{CreateActionRow, CreateButton, CreateEmbed, CreateSelectMenuOption},
};

use super::{
    render_market, truncate_to,
    ui::{self, Panel},
};

const PAGE_SIZE: usize = 25;
const INVALID: &str = "This resolution control is invalid. Run /market resolve again.";

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Action {
    Page(usize),
    Market {
        id: MarketId,
        page: usize,
    },
    Outcome {
        id: MarketId,
        outcome: OutcomeIndex,
        page: usize,
    },
    Confirm(Command),
}

pub(super) fn is_control(custom_id: &str) -> bool {
    custom_id.split(':').nth(3) == Some("resolve")
}

pub(super) fn parse(
    guild: GuildId,
    actor: Actor,
    custom_id: &str,
    kind: &ComponentInteractionDataKind,
) -> Result<Action, &'static str> {
    let parts = ui::scope(guild, actor, custom_id)?;
    let page = |value: &str| value.parse::<usize>().map_err(|_| INVALID);
    let outcome = |value: &str| {
        value
            .parse::<usize>()
            .ok()
            .filter(|value| *value < 10)
            .map(OutcomeIndex)
            .ok_or("Choose a valid outcome.")
    };
    match (parts.as_slice(), kind) {
        (["resolve", "p", value], ComponentInteractionDataKind::Button) => {
            Ok(Action::Page(page(value)?))
        }
        (["resolve", "m", value], ComponentInteractionDataKind::StringSelect { values }) => {
            let [id] = values.as_slice() else {
                return Err("Choose exactly one market.");
            };
            Ok(Action::Market {
                id: id.clone().into(),
                page: page(value)?,
            })
        }
        (["resolve", "b", id, value], ComponentInteractionDataKind::Button) => Ok(Action::Market {
            id: (*id).into(),
            page: page(value)?,
        }),
        (["resolve", "o", id, value], ComponentInteractionDataKind::StringSelect { values }) => {
            let [selected] = values.as_slice() else {
                return Err("Choose exactly one outcome.");
            };
            Ok(Action::Outcome {
                id: (*id).into(),
                outcome: outcome(selected)?,
                page: page(value)?,
            })
        }
        (["resolve", "c", id, value], ComponentInteractionDataKind::Button) => {
            // The store checks eligibility under its guild lock. Do not pre-read here:
            // redelivery must recover the receipt even after the market has settled.
            Ok(Action::Confirm(Command::Resolve {
                id: (*id).into(),
                outcome: outcome(value)?,
            }))
        }
        _ => Err(INVALID),
    }
}

fn eligible(market: &Market, actor: Actor, now: i64) -> bool {
    !actor.bot
        && actor.user_id.0 != 0
        && (actor.moderator || market.creator == actor.user_id)
        && market.status == Status::Open
        && now >= market.closes_at
}

fn button(id: String, label: &str) -> CreateButton {
    CreateButton::new(id)
        .label(label)
        .style(ButtonStyle::Secondary)
}

pub(super) fn picker(view: &View, actor: Actor, guild: GuildId, now: i64, page: usize) -> Panel {
    let mut markets: Vec<_> = view
        .state
        .markets
        .iter()
        .filter(|(_, market)| eligible(market, actor, now))
        .collect();
    markets.sort_by(|(ida, a), (idb, b)| a.closes_at.cmp(&b.closes_at).then_with(|| ida.cmp(idb)));
    let mut panel = Panel {
        content: "No closed, unresolved markets are available for you to resolve.".into(),
        embed: None,
        components: vec![],
    };
    if markets.is_empty() {
        return panel;
    }
    let last_page = (markets.len() - 1) / PAGE_SIZE;
    let page = page.min(last_page);
    let prefix = ui::prefix(guild, actor);
    let options = markets
        .iter()
        .skip(page * PAGE_SIZE)
        .take(PAGE_SIZE)
        .map(|(id, market)| {
            CreateSelectMenuOption::new(truncate_to(&market.question, 100), id.to_string())
                .description(format!("{} points pooled · ID: {id}", market.total_staked))
        })
        .collect();
    panel.content = format!(
        "Choose a closed market to resolve. Page {} of {}.",
        page + 1,
        last_page + 1
    );
    panel.components.push(ui::menu(
        format!("{prefix}:resolve:m:{page}"),
        "Choose a market to resolve",
        options,
    ));
    let mut buttons = vec![];
    if page > 0 {
        buttons.push(button(
            format!("{prefix}:resolve:p:{}", page - 1),
            "Previous",
        ));
    }
    if page < last_page {
        buttons.push(button(format!("{prefix}:resolve:p:{}", page + 1), "Next"));
    }
    if !buttons.is_empty() {
        panel.components.push(CreateActionRow::Buttons(buttons));
    }
    panel
}

pub(super) fn panel(
    view: &View,
    actor: Actor,
    guild: GuildId,
    now: i64,
    action: &Action,
) -> Result<Panel, &'static str> {
    let (id, page, outcome) = match action {
        Action::Page(page) => return Ok(picker(view, actor, guild, now, *page)),
        Action::Market { id, page } => (id, *page, None),
        Action::Outcome { id, page, outcome } => (id, *page, Some(*outcome)),
        Action::Confirm(_) => return Err(INVALID),
    };
    let market = view
        .state
        .markets
        .get(id)
        .ok_or("This market no longer exists. Run /market resolve again.")?;
    if !eligible(market, actor, now) {
        return Err(
            "This market is no longer eligible for you to resolve. Run /market resolve again.",
        );
    }
    let eligible_count = view
        .state
        .markets
        .values()
        .filter(|market| eligible(market, actor, now))
        .count();
    let page = page.min(eligible_count.saturating_sub(1) / PAGE_SIZE);
    let prefix = ui::prefix(guild, actor);
    let mut panel = Panel {
        content: "Choose the winning outcome.".into(),
        embed: Some(
            CreateEmbed::new()
                .title(truncate_to(&market.question, 256))
                .description(render_market(id, market, now)),
        ),
        components: vec![],
    };
    if let Some(outcome) = outcome {
        let label = market
            .options
            .get(outcome.0)
            .ok_or("Choose a valid outcome.")?;
        panel.content = format!(
            "Confirm resolution with winning outcome: {}. This settles the market and cannot be undone.",
            truncate_to(label, 160)
        );
        panel.components.push(CreateActionRow::Buttons(vec![
            button(
                format!("{prefix}:resolve:c:{id}:{outcome}"),
                "Confirm resolution",
            )
            .style(ButtonStyle::Danger),
            button(format!("{prefix}:resolve:b:{id}:{page}"), "Back"),
        ]));
    } else {
        panel.components.push(ui::menu(
            format!("{prefix}:resolve:o:{id}:{page}"),
            "Choose the winning outcome",
            market
                .options
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    CreateSelectMenuOption::new(truncate_to(label, 100), i.to_string())
                })
                .collect(),
        ));
        panel.components.push(CreateActionRow::Buttons(vec![button(
            format!("{prefix}:resolve:p:{page}"),
            "Back to markets",
        )]));
    }
    Ok(panel)
}
