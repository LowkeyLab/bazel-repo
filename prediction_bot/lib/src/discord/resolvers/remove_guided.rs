//! Read-only removal navigation. Only the existing `d` confirmation executes a command.
use super::{Actor, GuildId, MarketId, Status, UserId, View, remove, truncate_to, ui};
use crate::discord::{Input, InputValue};
use serenity::{
    all::{ButtonStyle, ComponentInteractionDataKind},
    builder::{CreateActionRow, CreateButton, CreateEmbed, CreateSelectMenuOption},
};
use std::fmt::Write as _;

const PAGE_SIZE: usize = 25;
const INVALID: &str = "This removal control is invalid. Run /market resolver remove again.";

pub(in crate::discord) fn options(
    input: &Input,
) -> Result<(Option<MarketId>, Option<UserId>), &'static str> {
    let mut market = None;
    let mut user = None;
    for option in &input.options {
        match (option.name.as_str(), &option.value) {
            ("market", InputValue::String(id)) if market.is_none() => {
                compact(id)?;
                market = Some(id.clone().into());
            }
            ("user", InputValue::User(id)) if user.is_none() && id.0 != 0 => user = Some(*id),
            _ => return Err("Invalid command options."),
        }
    }
    Ok((market, user))
}

pub(in crate::discord) enum Action {
    Markets {
        page: usize,
        user: Option<UserId>,
        selected: Option<MarketId>,
    },
    Market {
        id: MarketId,
        user: Option<UserId>,
    },
    Assignments {
        id: MarketId,
        page: usize,
        selected: Option<UserId>,
    },
    Confirmation {
        id: MarketId,
        user: UserId,
    },
}

pub(in crate::discord) fn is_control(id: &str) -> bool {
    matches!(
        id.split(':').nth(3),
        Some("rm" | "rp" | "ru" | "rq" | "r" | "s")
    )
}

fn compact(id: &str) -> Result<String, &'static str> {
    if id.len() != 36 || uuid::Uuid::parse_str(id).is_err() {
        return Err("Invalid market ID.");
    }
    Ok(id.replace('-', ""))
}

fn expanded(id: &str) -> Result<MarketId, &'static str> {
    if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid market ID.");
    }
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &id[..8],
        &id[8..12],
        &id[12..16],
        &id[16..20],
        &id[20..]
    )
    .into())
}

fn user(value: &str) -> Result<UserId, &'static str> {
    value
        .parse::<UserId>()
        .ok()
        .filter(|id| id.0 != 0)
        .ok_or("Choose a valid person.")
}

pub(in crate::discord) fn parse(
    guild: GuildId,
    actor: Actor,
    id: &str,
    kind: &ComponentInteractionDataKind,
) -> Result<Action, &'static str> {
    let parts = ui::scope(guild, actor, id)?;
    let optional_user = |value: &str| {
        if value == "0" {
            Ok(None)
        } else {
            user(value).map(Some)
        }
    };
    let page = |value: &str| {
        value
            .parse::<u32>()
            .map(|value| value as usize)
            .map_err(|_| INVALID)
    };
    match (parts.as_slice(), kind) {
        (["rp", index, target], ComponentInteractionDataKind::Button) => Ok(Action::Markets {
            page: page(index)?,
            user: optional_user(target)?,
            selected: None,
        }),
        (["s", id, target], ComponentInteractionDataKind::Button) => Ok(Action::Markets {
            page: 0,
            user: optional_user(target)?,
            selected: Some(expanded(id)?),
        }),
        (["rm", target], ComponentInteractionDataKind::StringSelect { values }) => {
            let [id] = values.as_slice() else {
                return Err("Choose exactly one market.");
            };
            compact(id)?;
            Ok(Action::Market {
                id: id.clone().into(),
                user: optional_user(target)?,
            })
        }
        (["rq", id, index], ComponentInteractionDataKind::Button) => Ok(Action::Assignments {
            id: expanded(id)?,
            page: page(index)?,
            selected: None,
        }),
        (["r", id, target], ComponentInteractionDataKind::Button) => Ok(Action::Assignments {
            id: expanded(id)?,
            page: 0,
            selected: Some(user(target)?),
        }),
        (["ru", id], ComponentInteractionDataKind::StringSelect { values }) => {
            let [target] = values.as_slice() else {
                return Err("Choose exactly one assignment.");
            };
            Ok(Action::Confirmation {
                id: expanded(id)?,
                user: user(target)?,
            })
        }
        _ => Err(INVALID),
    }
}

fn button(id: String, label: &str) -> CreateButton {
    CreateButton::new(id)
        .label(label)
        .style(ButtonStyle::Secondary)
}

fn manageable(market: &crate::domain::Market, actor: Actor) -> bool {
    !actor.bot
        && actor.user_id.0 != 0
        && market.status == Status::Open
        && (actor.moderator || market.creator == actor.user_id)
}

fn markets(
    view: &View,
    guild: GuildId,
    actor: Actor,
    page: usize,
    user: Option<UserId>,
    selected: Option<&MarketId>,
) -> ui::Panel {
    let mut eligible: Vec<_> = view
        .state
        .markets
        .iter()
        .filter(|(_, market)| manageable(market, actor))
        .collect();
    eligible.sort_by_key(|(id, _)| *id);
    let mut panel = ui::Panel {
        content: "No nonterminal markets are available for you to manage.".into(),
        embed: None,
        components: vec![],
    };
    if eligible.is_empty() {
        return panel;
    }
    let last = (eligible.len() - 1) / PAGE_SIZE;
    let page = selected
        .and_then(|id| eligible.iter().position(|(candidate, _)| *candidate == id))
        .map_or(page, |index| index / PAGE_SIZE)
        .min(last);
    let prefix = ui::prefix(guild, actor);
    let target = user.unwrap_or(UserId(0));
    panel.content = format!(
        "Choose a market to remove an additional resolver. Page {} of {}.",
        page + 1,
        last + 1
    );
    if let Some(user) = user {
        let _ = write!(panel.content, " Selected person: <@{user}>.");
    }
    panel.components.push(ui::menu(
        format!("{prefix}:rm:{target}"),
        "Choose a market",
        eligible
            .iter()
            .skip(page * PAGE_SIZE)
            .take(PAGE_SIZE)
            .map(|(id, market)| {
                CreateSelectMenuOption::new(truncate_to(&market.question, 100), id.to_string())
                    .description(format!("Market {id}"))
                    .default_selection(selected == Some(*id))
            })
            .collect(),
    ));
    let mut buttons = vec![];
    if page > 0 {
        buttons.push(button(
            format!("{prefix}:rp:{}:{target}", page - 1),
            "Previous markets",
        ));
    }
    if page < last {
        buttons.push(button(
            format!("{prefix}:rp:{}:{target}", page + 1),
            "Next markets",
        ));
    }
    if !buttons.is_empty() {
        panel.components.push(CreateActionRow::Buttons(buttons));
    }
    panel
}

fn assignments(
    view: &View,
    guild: GuildId,
    actor: Actor,
    id: &MarketId,
    page: usize,
    selected: Option<UserId>,
) -> Result<ui::Panel, &'static str> {
    let market = view
        .state
        .markets
        .get(id)
        .ok_or("This market no longer exists.")?;
    if !(actor.moderator || market.creator == actor.user_id) || actor.bot || actor.user_id.0 == 0 {
        return Err("You no longer have permission to manage this market's resolvers.");
    }
    if market.status != Status::Open {
        return Err("This market is completed; its resolvers cannot be changed.");
    }
    let compact = compact(&id.0)?;
    let prefix = ui::prefix(guild, actor);
    let position = selected.and_then(|user| {
        market
            .resolvers
            .iter()
            .position(|candidate| *candidate == user)
    });
    let page = position
        .map_or(page, |index| index / PAGE_SIZE)
        .min(market.resolvers.len().saturating_sub(1) / PAGE_SIZE);
    let mut panel = ui::Panel {
        content: format!("Market {id} has no explicit resolver assignments to remove."),
        embed: Some(CreateEmbed::new().title(truncate_to(&market.question, 256))),
        components: vec![],
    };
    let mut buttons = vec![];
    if !market.resolvers.is_empty() {
        let last = (market.resolvers.len() - 1) / PAGE_SIZE;
        panel.content = format!(
            "Choose an explicit resolver assignment to remove from market {id}. Departed members remain removable. Page {} of {}.",
            page + 1,
            last + 1
        );
        panel.components.push(ui::menu(
            format!("{prefix}:ru:{compact}"),
            "Choose an existing assignment",
            market
                .resolvers
                .iter()
                .skip(page * PAGE_SIZE)
                .take(PAGE_SIZE)
                .map(|user| {
                    CreateSelectMenuOption::new(format!("User {user}"), user.to_string())
                        .default_selection(selected == Some(*user))
                })
                .collect(),
        ));
        if page > 0 {
            buttons.push(button(
                format!("{prefix}:rq:{compact}:{}", page - 1),
                "Previous assignments",
            ));
        }
        if page < last {
            buttons.push(button(
                format!("{prefix}:rq:{compact}:{}", page + 1),
                "Next assignments",
            ));
        }
    }
    buttons.push(button(
        format!("{prefix}:s:{compact}:{}", selected.unwrap_or(UserId(0))),
        "Back to markets",
    ));
    panel.components.push(CreateActionRow::Buttons(buttons));
    Ok(panel)
}

pub(in crate::discord) fn start(
    view: &View,
    guild: GuildId,
    actor: Actor,
    id: Option<&MarketId>,
    user: Option<UserId>,
) -> Result<ui::Panel, &'static str> {
    match id {
        Some(id) => panel(
            view,
            guild,
            actor,
            &Action::Market {
                id: id.clone(),
                user,
            },
        ),
        None => Ok(markets(view, guild, actor, 0, user, None)),
    }
}

pub(in crate::discord) fn panel(
    view: &View,
    guild: GuildId,
    actor: Actor,
    action: &Action,
) -> Result<ui::Panel, &'static str> {
    match action {
        Action::Markets {
            page,
            user,
            selected,
        } => Ok(markets(view, guild, actor, *page, *user, selected.as_ref())),
        Action::Market { id, user: None } => assignments(view, guild, actor, id, 0, None),
        Action::Assignments { id, page, selected } => {
            assignments(view, guild, actor, id, *page, *selected)
        }
        Action::Market {
            id,
            user: Some(user),
        }
        | Action::Confirmation { id, user } => {
            let mut panel = remove::confirmation(view, guild, actor, id, *user)?;
            panel.components.push(CreateActionRow::Buttons(vec![button(
                format!("{}:r:{}:{user}", ui::prefix(guild, actor), compact(&id.0)?),
                "Back to assignments",
            )]));
            Ok(panel)
        }
    }
}

pub(in crate::discord) async fn handle_component(
    store: &crate::store::Store,
    http: &serenity::http::Http,
    component: &serenity::all::ComponentInteraction,
) {
    use crate::audit::QueryKind;
    use crate::discord::{
        deferred_response, read_query, rejected, reply, transport::SerenityTransport,
    };
    use serenity::all::Permissions;
    let actor = Actor {
        user_id: UserId(component.user.id.get()),
        bot: component.user.bot,
        moderator: component
            .member
            .as_ref()
            .and_then(|member| member.permissions)
            .is_some_and(|permissions| {
                permissions.intersects(Permissions::ADMINISTRATOR | Permissions::MANAGE_GUILD)
            }),
    };
    let guild = component
        .guild_id
        .map_or(GuildId(0), |id| GuildId(id.get()));
    let action = parse(
        guild,
        actor,
        &component.data.custom_id,
        &component.data.kind,
    );
    let transport = if action.is_ok() {
        SerenityTransport::ComponentUpdate(component, http)
    } else {
        SerenityTransport::Component(component, http)
    };
    deferred_response(
        &transport,
        store.audit().as_ref(),
        component.guild_id.map(|id| GuildId(id.get())),
        component.id.get(),
        || async {
            let result = match action {
                Ok(action) => match read_query(
                    store.audit().as_ref(),
                    guild,
                    component.id.get(),
                    QueryKind::Component,
                    store.view(guild),
                    None,
                )
                .await
                {
                    Ok(view) => panel(&view, guild, actor, &action),
                    Err(message) => return reply(&message).embeds(vec![]).components(vec![]),
                },
                Err(message) => Err(message),
            };
            match result {
                Ok(panel) => panel.edit(),
                Err(message) => {
                    rejected(
                        store.audit().as_ref(),
                        component.guild_id.map(|id| GuildId(id.get())),
                        component.id.get(),
                    );
                    reply(message).embeds(vec![]).components(vec![])
                }
            }
        },
    )
    .await;
}
