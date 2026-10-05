//! Read-only, private browsing of recorded resolver assignments.
use super::super::{
    truncate_to,
    ui::{self, Panel},
};
use crate::{
    domain::{Actor, Market, Status},
    store::{Store, View},
    types::{GuildId, MarketId, UserId},
};
use serenity::{
    all::{ButtonStyle, ComponentInteraction, ComponentInteractionDataKind},
    builder::{CreateActionRow, CreateButton, CreateEmbed, CreateSelectMenuOption},
    http::Http,
};

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;

const PAGE_SIZE: usize = 25;
const INVALID: &str = "This listing control is invalid. Run /market resolver list again.";

pub(in crate::discord) fn is_control(id: &str) -> bool {
    matches!(id.split(':').nth(3), Some("lp" | "ls" | "lm"))
}

enum Action {
    Page(usize),
    Market { id: MarketId, page: usize },
}

fn parse(
    guild: GuildId,
    actor: Actor,
    id: &str,
    kind: &ComponentInteractionDataKind,
) -> Result<Action, &'static str> {
    let parts = ui::scope(guild, actor, id)?;
    // Bound page fields as well as the complete custom ID for Discord's 100-character limit.
    let page = |value: &str| {
        value
            .parse::<u32>()
            .map(|n| n as usize)
            .map_err(|_| INVALID)
    };
    match (parts.as_slice(), kind) {
        (["lp", p], ComponentInteractionDataKind::Button) => Ok(Action::Page(page(p)?)),
        (["ls"], ComponentInteractionDataKind::StringSelect { values }) => {
            let [id] = values.as_slice() else {
                return Err("Choose exactly one market.");
            };
            Ok(Action::Market {
                id: id.clone().into(),
                page: 0,
            })
        }
        (["lm", id, p], ComponentInteractionDataKind::Button) => Ok(Action::Market {
            id: (*id).into(),
            page: page(p)?,
        }),
        _ => Err(INVALID),
    }
}

fn button(id: String, label: &str) -> CreateButton {
    CreateButton::new(id)
        .label(label)
        .style(ButtonStyle::Secondary)
}

fn status(market: &Market, now: i64) -> &'static str {
    match market.status {
        Status::Open if now < market.closes_at => "Open",
        Status::Open => "Awaiting settlement",
        Status::Resolved { .. } => "Resolved",
        Status::Cancelled => "Cancelled",
    }
}

pub(in crate::discord) fn picker(
    view: &View,
    guild: GuildId,
    actor: Actor,
    now: i64,
    page: usize,
) -> Panel {
    let mut panel = Panel {
        content: "No markets exist in this server yet.".into(),
        embed: None,
        components: vec![],
    };
    if view.state.markets.is_empty() {
        return panel;
    }
    let last = (view.state.markets.len() - 1) / PAGE_SIZE;
    let page = page.min(last);
    let prefix = ui::prefix(guild, actor);
    panel.content = format!(
        "Choose a market to inspect its explicit resolver assignments, including completed markets. Page {} of {}.",
        page + 1,
        last + 1
    );
    panel.components.push(ui::menu(
        format!("{prefix}:ls"),
        "Choose a market",
        view.state
            .markets
            .iter()
            .skip(page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .map(|(id, market)| {
                CreateSelectMenuOption::new(truncate_to(&market.question, 100), id.to_string())
                    .description(format!(
                        "{} · {} explicit assignments",
                        status(market, now),
                        market.resolvers.len()
                    ))
            })
            .collect(),
    ));
    let mut buttons = vec![];
    if page > 0 {
        buttons.push(button(
            format!("{prefix}:lp:{}", page - 1),
            "Previous markets",
        ));
    }
    if page < last {
        buttons.push(button(format!("{prefix}:lp:{}", page + 1), "Next markets"));
    }
    if !buttons.is_empty() {
        panel.components.push(CreateActionRow::Buttons(buttons));
    }
    panel
}

fn panel(
    view: &View,
    guild: GuildId,
    actor: Actor,
    now: i64,
    action: Action,
) -> Result<Panel, &'static str> {
    let Action::Market { id, page } = action else {
        let Action::Page(page) = action else {
            unreachable!()
        };
        return Ok(picker(view, guild, actor, now, page));
    };
    let market = view
        .state
        .markets
        .get(&id)
        .ok_or("This market does not exist in this server. Run /market resolver list again.")?;
    let last = market.resolvers.len().saturating_sub(1) / PAGE_SIZE;
    let page = page.min(last);
    let names = market
        .resolvers
        .iter()
        .skip(page * PAGE_SIZE)
        .take(PAGE_SIZE)
        .map(|user| format!("<@{user}>"))
        .collect::<Vec<_>>()
        .join("\n");
    let description = format!(
        "ID: {id}\nStatus: {}\n\nExplicit resolver assignments ({} total; page {} of {}):\n{}\n\nIndependent authority: creator <@{}> and current moderators (Administrator or Manage Guild).\nAssignments require current membership and enrollment for use; this list does not verify eligibility. Removing an assignment cannot revoke independent authority.",
        status(market, now),
        market.resolvers.len(),
        page + 1,
        last + 1,
        if names.is_empty() { "None" } else { &names },
        market.creator
    );
    let prefix = ui::prefix(guild, actor);
    let mut buttons = vec![];
    if page > 0 {
        buttons.push(button(
            format!("{prefix}:lm:{id}:{}", page - 1),
            "Previous assignments",
        ));
    }
    if page < last {
        buttons.push(button(
            format!("{prefix}:lm:{id}:{}", page + 1),
            "Next assignments",
        ));
    }
    let market_page = view
        .state
        .markets
        .keys()
        .position(|key| key == &id)
        .unwrap_or(0)
        / PAGE_SIZE;
    buttons.push(button(
        format!("{prefix}:lp:{market_page}"),
        "Back to markets",
    ));
    Ok(Panel {
        content:
            "Recorded assignments are retained when a person leaves and when a market completes."
                .into(),
        embed: Some(
            CreateEmbed::new()
                .title(truncate_to(&market.question, 256))
                .description(description),
        ),
        components: vec![CreateActionRow::Buttons(buttons)],
    })
}

pub(in crate::discord) async fn handle_component(
    store: &Store,
    http: &Http,
    component: &ComponentInteraction,
) {
    let guild = component
        .guild_id
        .map_or(GuildId(0), |id| GuildId(id.get()));
    let actor = Actor {
        user_id: UserId(component.user.id.get()),
        bot: component.user.bot,
        moderator: false,
    };
    let action = parse(
        guild,
        actor,
        &component.data.custom_id,
        &component.data.kind,
    );
    super::read_navigation(store, http, component, guild, action, |view, action| {
        panel(view, guild, actor, chrono::Utc::now().timestamp(), action)
    })
    .await;
}
