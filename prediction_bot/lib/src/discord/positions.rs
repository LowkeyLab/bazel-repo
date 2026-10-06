//! Private, read-only inspection of recorded market settlement positions.
use std::collections::BTreeMap;
use std::fmt::Write as _;

use serenity::{
    all::{ButtonStyle, ComponentInteraction, ComponentInteractionDataKind},
    builder::{CreateActionRow, CreateButton, CreateEmbed},
    http::Http,
};

use super::{
    truncate_to,
    ui::{self, Panel},
};
use crate::{
    domain::{Actor, Market, Status},
    store::{Store, View},
    types::{GuildId, MarketId, UserId},
};

const INVALID: &str = "This positions control is invalid. Run /market positions again.";
const UNENROLLED: &str = "You are not enrolled. Use /market join first.";

pub(super) fn is_control(id: &str) -> bool {
    id.split(':').nth(3) == Some("ps")
}
fn message(content: &str) -> Panel {
    Panel {
        content: content.into(),
        embed: None,
        components: vec![],
    }
}
pub(super) fn enrolled(view: &View, actor: Actor) -> Result<(), &'static str> {
    view.state
        .accounts
        .contains_key(&actor.user_id)
        .then_some(())
        .ok_or(UNENROLLED)
}
pub(super) fn start(view: &View, guild: GuildId, actor: Actor, id: Option<&MarketId>) -> Panel {
    let result = enrolled(view, actor).and_then(|()| {
        id.ok_or("Supply a resolved market ID with /market positions id.")
            .and_then(|id| market(view, guild, actor, id, 0))
    });
    result.unwrap_or_else(message)
}

struct Position {
    user: UserId,
    net: i64,
    block: String,
}
fn positions(market: &Market) -> Vec<Position> {
    let mut stakes: BTreeMap<UserId, Vec<i64>> = BTreeMap::new();
    for bet in &market.bets {
        stakes
            .entry(bet.user_id)
            .or_insert_with(|| vec![0; market.options.len()])[bet.outcome.0] += bet.amount.0;
    }
    let payouts: BTreeMap<_, _> = market
        .payouts
        .iter()
        .map(|a| (a.user_id, a.amount.0))
        .collect();
    let mut result: Vec<_> = stakes
        .into_iter()
        .map(|(user, stakes)| {
            let total: i64 = stakes.iter().sum();
            let payout = payouts.get(&user).copied().unwrap_or(0);
            let net = payout - total;
            let mut block = format!("<@{user}>\n");
            for (index, amount) in stakes
                .into_iter()
                .enumerate()
                .filter(|(_, amount)| *amount > 0)
            {
                let _ = writeln!(
                    block,
                    "{}. {}: {amount} points",
                    index + 1,
                    truncate_to(&market.options[index], 48)
                );
            }
            let _ = write!(
                block,
                "Total stake: {total} · Payout: {payout} · Net: {net:+}\n\n"
            );
            Position { user, net, block }
        })
        .collect();
    result.sort_by(|a, b| b.net.cmp(&a.net).then_with(|| a.user.cmp(&b.user)));
    result
}

pub(super) fn market(
    view: &View,
    guild: GuildId,
    actor: Actor,
    id: &MarketId,
    requested_page: usize,
) -> Result<Panel, &'static str> {
    enrolled(view, actor)?;
    let market = view
        .state
        .markets
        .get(id)
        .ok_or("This market does not exist in this server.")?;
    let Status::Resolved { outcome, refunded } = market.status else {
        return Err("Positions are available only for resolved markets.");
    };
    let header = format!(
        "ID: {id}\nWinning outcome: {}. {}\n{}\n\n",
        outcome.0 + 1,
        truncate_to(&market.options[outcome.0], 80),
        if refunded {
            "Refunded: no bets backed the winning outcome; stakes were returned."
        } else {
            "Settlement: recorded payouts."
        }
    );
    // Reserve space for the page indicator. Complete blocks fit even with ten backed outcomes.
    let budget = 4096 - header.encode_utf16().count() - 100;
    let mut pages = vec![String::new()];
    let mut units = 0;
    for position in positions(market) {
        let size = position.block.encode_utf16().count();
        if units + size > budget {
            pages.push(String::new());
            units = 0;
        }
        pages
            .last_mut()
            .expect("at least one page")
            .push_str(&position.block);
        units += size;
    }
    let page = requested_page.min(pages.len() - 1);
    let description = format!(
        "{header}Bettors · Page {} of {}\n\n{}",
        page + 1,
        pages.len(),
        if market.bets.is_empty() {
            "No bets were placed in this market."
        } else {
            &pages[page]
        }
    );
    let prefix = ui::prefix(guild, actor);
    let mut buttons = vec![];
    for (label, target) in [
        ("Previous", page.checked_sub(1)),
        ("Next", (page + 1 < pages.len()).then_some(page + 1)),
    ] {
        if let Some(target) = target {
            buttons.push(
                CreateButton::new(format!("{prefix}:ps:{id}:{target}"))
                    .label(label)
                    .style(ButtonStyle::Secondary),
            );
        }
    }
    Ok(Panel {
        content: "Recorded positions and settlement results.".into(),
        embed: Some(
            CreateEmbed::new()
                .title(truncate_to(&market.question, 256))
                .description(description),
        ),
        components: if buttons.is_empty() {
            vec![]
        } else {
            vec![CreateActionRow::Buttons(buttons)]
        },
    })
}

pub(super) async fn handle_component(store: &Store, http: &Http, component: &ComponentInteraction) {
    let guild = component
        .guild_id
        .map_or(GuildId(0), |id| GuildId(id.get()));
    let actor = Actor {
        user_id: UserId(component.user.id.get()),
        bot: component.user.bot,
        moderator: false,
    };
    let action = ui::scope(guild, actor, &component.data.custom_id).and_then(|parts| {
        match (parts.as_slice(), &component.data.kind) {
            (["ps", id, page], ComponentInteractionDataKind::Button) => page
                .parse::<u32>()
                .map(|page| (MarketId::from(*id), page as usize))
                .map_err(|_| INVALID),
            _ => Err(INVALID),
        }
    });
    super::resolvers::read_navigation(store, http, component, guild, action, |view, (id, page)| {
        market(view, guild, actor, &id, page)
    })
    .await;
}
