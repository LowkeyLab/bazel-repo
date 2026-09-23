//! Private betting navigation. A stake submission identifies one confirmed bet.
use super::{
    render_market, truncate_to,
    ui::{self, Panel},
};
use crate::{
    domain::{Actor, Command, Market, Status},
    store::View,
    types::{GuildId, MarketId, OutcomeIndex, Points},
};
use serenity::{
    all::{ButtonStyle, ComponentInteractionDataKind, InputTextStyle},
    builder::{
        CreateActionRow, CreateButton, CreateEmbed, CreateInputText, CreateModal,
        CreateSelectMenuOption,
    },
};

mod handler;
#[cfg(test)]
mod tests;

const INVALID: &str = "This betting control is invalid. Run /market bet again.";
const PAGE_SIZE: usize = 25;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Action {
    Page(usize),
    Market(MarketId),
    Stake { id: MarketId, outcome: OutcomeIndex },
    Confirm { command: Command, submission: u64 },
    Cancel,
}

// Compact numbers keep scoped controls below Discord's 100-character limit,
// even with maximum snowflakes, UUID, stake, and submission ID.
fn compact(mut value: u128) -> String {
    let mut digits = Vec::new();
    loop {
        digits.push(b"0123456789abcdefghijklmnopqrstuvwxyz"[(value % 36) as usize] as char);
        value /= 36;
        if value == 0 {
            break;
        }
    }
    digits.into_iter().rev().collect()
}
fn number(value: &str) -> Result<u128, &'static str> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
    {
        return Err(INVALID);
    }
    u128::from_str_radix(value, 36).map_err(|_| INVALID)
}
fn prefix(guild: GuildId, actor: Actor) -> String {
    format!(
        "pm:w:{}:{}",
        compact(guild.0.into()),
        compact(actor.user_id.0.into())
    )
}
pub(super) fn is_control(id: &str) -> bool {
    id.starts_with("pm:w:")
}
fn scope(guild: GuildId, actor: Actor, id: &str) -> Result<Vec<&str>, &'static str> {
    if guild.0 == 0 || actor.user_id.0 == 0 || actor.bot || id.len() > 100 {
        return Err(INVALID);
    }
    id.strip_prefix(&format!("{}:", prefix(guild, actor)))
        .map(|suffix| suffix.split(':').collect())
        .ok_or(INVALID)
}
fn market_key(id: &MarketId) -> Result<String, &'static str> {
    // MarketId keys are case-sensitive. Keep all hexadecimal characters exactly
    // as stored; removing the four hyphens still fits the control length limit.
    uuid::Uuid::parse_str(&id.0)
        .map(|_| id.0.replace('-', ""))
        .map_err(|_| INVALID)
}
fn market_id(value: &str) -> Result<MarketId, &'static str> {
    if value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Ok(format!(
            "{}-{}-{}-{}-{}",
            &value[..8],
            &value[8..12],
            &value[12..16],
            &value[16..20],
            &value[20..]
        )
        .into());
    }
    // Existing widgets encoded lowercase UUIDs in base 36 (at most 25 digits).
    // Keep accepting them so controls already displayed by Discord still work.
    if value.len() > 25 {
        return Err(INVALID);
    }
    Ok(uuid::Uuid::from_u128(number(value)?).to_string().into())
}
fn outcome(value: &str) -> Result<OutcomeIndex, &'static str> {
    value
        .parse::<usize>()
        .ok()
        .filter(|n| *n < 10)
        .map(OutcomeIndex)
        .ok_or("Choose a valid outcome.")
}
pub(super) fn parse(
    guild: GuildId,
    actor: Actor,
    id: &str,
    kind: &ComponentInteractionDataKind,
) -> Result<Action, &'static str> {
    let parts = scope(guild, actor, id)?;
    match (parts.as_slice(), kind) {
        (["p", page], ComponentInteractionDataKind::Button) => Ok(Action::Page(
            usize::try_from(number(page)?).map_err(|_| INVALID)?,
        )),
        (["m"], ComponentInteractionDataKind::StringSelect { values }) => {
            let [id] = values.as_slice() else {
                return Err(INVALID);
            };
            uuid::Uuid::parse_str(id).map_err(|_| INVALID)?;
            Ok(Action::Market(id.clone().into()))
        }
        (["b", id], ComponentInteractionDataKind::Button) => Ok(Action::Market(market_id(id)?)),
        (["o", id], ComponentInteractionDataKind::StringSelect { values }) => {
            let [selected] = values.as_slice() else {
                return Err(INVALID);
            };
            Ok(Action::Stake {
                id: market_id(id)?,
                outcome: outcome(selected)?,
            })
        }
        (["c", id, selected, amount, submission], ComponentInteractionDataKind::Button) => {
            let amount = i64::try_from(number(amount)?)
                .ok()
                .filter(|n| *n > 0)
                .ok_or(INVALID)?;
            let submission = u64::try_from(number(submission)?)
                .ok()
                .filter(|n| *n > 0)
                .ok_or(INVALID)?;
            Ok(Action::Confirm {
                command: Command::Bet {
                    id: market_id(id)?,
                    outcome: outcome(selected)?,
                    amount: Points(amount),
                },
                submission,
            })
        }
        (["x"], ComponentInteractionDataKind::Button) => Ok(Action::Cancel),
        _ => Err(INVALID),
    }
}
fn button(id: String, label: &str) -> CreateButton {
    CreateButton::new(id)
        .label(label)
        .style(ButtonStyle::Secondary)
}
fn empty(content: &str) -> Panel {
    Panel {
        content: content.into(),
        embed: None,
        components: vec![],
    }
}
fn eligible(market: &Market, now: i64) -> bool {
    market.status == Status::Open && now < market.closes_at
}
fn open_market<'a>(view: &'a View, id: &MarketId, now: i64) -> Result<&'a Market, &'static str> {
    view.state
        .markets
        .get(id)
        .filter(|m| eligible(m, now))
        .ok_or("This market is closed or unavailable. Run /market bet again.")
}
pub(super) fn picker(view: &View, actor: Actor, guild: GuildId, now: i64, page: usize) -> Panel {
    let mut markets: Vec<_> = view
        .state
        .markets
        .iter()
        .filter(|(_, m)| eligible(m, now))
        .collect();
    markets.sort_by(|(ia, a), (ib, b)| a.closes_at.cmp(&b.closes_at).then_with(|| ia.cmp(ib)));
    if markets.is_empty() {
        return empty("No markets are open for betting.");
    }
    let last = (markets.len() - 1) / PAGE_SIZE;
    let page = page.min(last);
    let prefix = prefix(guild, actor);
    let mut panel = empty(&format!(
        "Choose an open market. Page {} of {}.",
        page + 1,
        last + 1
    ));
    panel.components.push(ui::menu(
        format!("{prefix}:m"),
        "Choose a market to bet on",
        markets
            .iter()
            .skip(page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .map(|(id, m)| {
                CreateSelectMenuOption::new(truncate_to(&m.question, 100), id.to_string())
            })
            .collect(),
    ));
    let mut buttons = vec![];
    if page > 0 {
        buttons.push(button(
            format!("{prefix}:p:{}", compact((page - 1) as u128)),
            "Previous",
        ));
    }
    if page < last {
        buttons.push(button(
            format!("{prefix}:p:{}", compact((page + 1) as u128)),
            "Next",
        ));
    }
    buttons.push(button(format!("{prefix}:x"), "Cancel"));
    panel.components.push(CreateActionRow::Buttons(buttons));
    panel
}
pub(super) fn panel(
    view: &View,
    actor: Actor,
    guild: GuildId,
    now: i64,
    action: &Action,
) -> Result<Panel, &'static str> {
    let id = match action {
        Action::Page(page) => return Ok(picker(view, actor, guild, now, *page)),
        Action::Cancel => return Ok(empty("Bet cancelled. No points were staked.")),
        Action::Market(id) => id,
        _ => return Err(INVALID),
    };
    let market = open_market(view, id, now)?;
    let prefix = prefix(guild, actor);
    let key = market_key(id)?;
    Ok(Panel {
        content: "Choose an outcome.".into(),
        embed: Some(
            CreateEmbed::new()
                .title(truncate_to(&market.question, 256))
                .description(render_market(id, market, now)),
        ),
        components: vec![
            ui::menu(
                format!("{prefix}:o:{key}"),
                "Choose an outcome",
                market
                    .options
                    .iter()
                    .enumerate()
                    .map(|(i, label)| {
                        CreateSelectMenuOption::new(truncate_to(label, 100), i.to_string())
                    })
                    .collect(),
            ),
            CreateActionRow::Buttons(vec![
                button(format!("{prefix}:p:0"), "Back to markets"),
                button(format!("{prefix}:x"), "Cancel"),
            ]),
        ],
    })
}
pub(super) fn stake_modal(
    view: &View,
    actor: Actor,
    guild: GuildId,
    now: i64,
    id: &MarketId,
    outcome: OutcomeIndex,
) -> Result<CreateModal, &'static str> {
    let market = open_market(view, id, now)?;
    let label = market
        .options
        .get(outcome.0)
        .ok_or("Choose a valid outcome.")?;
    let account = view
        .state
        .accounts
        .get(&actor.user_id)
        .ok_or("You are not enrolled. Use /market join first.")?;
    Ok(CreateModal::new(
        format!("{}:s:{}:{outcome}", prefix(guild, actor), market_key(id)?),
        "Enter your stake",
    )
    .components(vec![CreateActionRow::InputText(
        CreateInputText::new(
            InputTextStyle::Short,
            truncate_to(&format!("Points on {label}"), 45),
            "amount",
        )
        .placeholder(format!("Available: {} points", account.balance))
        .min_length(1)
        .max_length(19)
        .required(true),
    )]))
}
pub(super) fn preview(
    view: &View,
    actor: Actor,
    guild: GuildId,
    now: i64,
    custom_id: &str,
    amount: &str,
    submission: u64,
) -> Result<Panel, &'static str> {
    let parts = scope(guild, actor, custom_id)?;
    let ["s", id, selected] = parts.as_slice() else {
        return Err(INVALID);
    };
    if submission == 0 {
        return Err(INVALID);
    }
    let id = market_id(id)?;
    let outcome = outcome(selected)?;
    let amount = amount
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or("Stake must be a positive whole number.")?;
    let market = open_market(view, &id, now)?;
    let label = market
        .options
        .get(outcome.0)
        .ok_or("Choose a valid outcome.")?;
    let account = view
        .state
        .accounts
        .get(&actor.user_id)
        .ok_or("You are not enrolled. Use /market join first.")?;
    if amount > account.balance.0 {
        return Err("You do not have enough points for this stake.");
    }
    let prefix = prefix(guild, actor);
    let key = market_key(&id)?;
    Ok(Panel {
        content: format!(
            "Confirm your bet: {amount} points on {}.",
            truncate_to(label, 160)
        ),
        embed: Some(
            CreateEmbed::new()
                .title(truncate_to(&market.question, 256))
                .description(render_market(&id, market, now)),
        ),
        components: vec![CreateActionRow::Buttons(vec![
            button(
                format!(
                    "{prefix}:c:{key}:{outcome}:{}:{}",
                    compact(amount.unsigned_abs().into()),
                    compact(submission.into())
                ),
                "Confirm bet",
            )
            .style(ButtonStyle::Primary),
            button(format!("{prefix}:b:{key}"), "Back"),
            button(format!("{prefix}:x"), "Cancel"),
        ])],
    })
}
