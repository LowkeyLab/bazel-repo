//! Read-only addition navigation. Final confirmation keeps the receipt-first write path.
use super::{confirmation, deferred_response, reply, truncate_to, ui};
use crate::{
    audit::QueryKind,
    discord::{read_query, rejected, transport::SerenityTransport},
    domain::{Actor, Market, Status},
    store::{Store, View},
    types::{GuildId, MarketId, UserId},
};
use serenity::{
    all::{ButtonStyle, ComponentInteraction, ComponentInteractionDataKind, Permissions},
    builder::{
        CreateActionRow, CreateButton, CreateEmbed, CreateSelectMenu, CreateSelectMenuKind,
        CreateSelectMenuOption,
    },
    http::Http,
};

const INVALID: &str = "This resolver control is invalid. Run /market resolver add again.";

pub(super) fn is_control(id: &str) -> bool {
    matches!(id.split(':').nth(3), Some("m" | "u" | "p" | "b" | "v"))
}

fn manageable(market: &Market, actor: Actor) -> bool {
    !actor.bot
        && actor.user_id.0 != 0
        && (actor.moderator || market.creator == actor.user_id)
        && market.status == Status::Open
}

fn market<'a>(view: &'a View, actor: Actor, id: &MarketId) -> Result<&'a Market, &'static str> {
    let market = view
        .state
        .markets
        .get(id)
        .ok_or("This market no longer exists.")?;
    if actor.bot || actor.user_id.0 == 0 || !(actor.moderator || actor.user_id == market.creator) {
        return Err("You no longer have permission to manage this market's resolvers.");
    }
    if market.status != Status::Open {
        return Err("This market is completed; its resolvers cannot be changed.");
    }
    Ok(market)
}

fn compact(id: &MarketId) -> Result<String, &'static str> {
    let compact = id.0.replace('-', "");
    uuid::Uuid::parse_str(&id.0).map_err(|_| "Invalid market ID.")?;
    if compact.len() != 32 {
        return Err("Invalid market ID.");
    }
    Ok(compact)
}
fn expand(id: &str) -> Result<MarketId, &'static str> {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
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
fn user(value: &str) -> Result<Option<UserId>, &'static str> {
    match value {
        "0" => Ok(None),
        value => value
            .parse::<UserId>()
            .ok()
            .filter(|id| id.0 != 0)
            .map(Some)
            .ok_or("Choose a valid person."),
    }
}

fn button(id: String, label: &str) -> CreateButton {
    CreateButton::new(id)
        .label(label)
        .style(ButtonStyle::Secondary)
}

fn people(
    view: &View,
    guild: GuildId,
    actor: Actor,
    id: &MarketId,
    selected: Option<UserId>,
) -> Result<ui::Panel, &'static str> {
    let market = market(view, actor, id)?;
    let compact = compact(id)?;
    let prefix = ui::prefix(guild, actor);
    Ok(ui::Panel {
        content: "Choose a person to add as an additional resolver.".into(),
        embed: Some(CreateEmbed::new().title(truncate_to(&market.question, 256))),
        components: vec![
            CreateActionRow::SelectMenu(
                CreateSelectMenu::new(
                    format!("{prefix}:u:{compact}"),
                    CreateSelectMenuKind::User {
                        default_users: selected.map(|user| vec![user.0.into()]),
                    },
                )
                .placeholder("Choose a person")
                .min_values(1)
                .max_values(1),
            ),
            CreateActionRow::Buttons(vec![button(
                format!("{prefix}:v:{compact}:{}", selected.unwrap_or(UserId(0))),
                "Back to markets",
            )]),
        ],
    })
}

fn picker(
    view: &View,
    guild: GuildId,
    actor: Actor,
    page: usize,
    selected: Option<UserId>,
    current: Option<&MarketId>,
) -> ui::Panel {
    let markets: Vec<_> = view
        .state
        .markets
        .iter()
        .filter(|(_, market)| manageable(market, actor))
        .collect();
    if markets.is_empty() {
        return ui::Panel {
            content: "No markets are available for you to manage.".into(),
            embed: None,
            components: vec![],
        };
    }
    let last = (markets.len() - 1) / 25;
    let page = current
        .and_then(|current| markets.iter().position(|(id, _)| *id == current))
        .map_or(page, |index| index / 25)
        .min(last);
    let options = markets
        .iter()
        .skip(page * 25)
        .take(25)
        .map(|(id, market)| {
            CreateSelectMenuOption::new(truncate_to(&market.question, 100), id.to_string())
                .default_selection(current == Some(*id))
        })
        .collect();
    let prefix = ui::prefix(guild, actor);
    let selected = selected.unwrap_or(UserId(0));
    let mut components = vec![ui::menu(
        format!("{prefix}:m:{selected}"),
        "Choose a market",
        options,
    )];
    let mut buttons = vec![];
    if page > 0 {
        buttons.push(button(
            format!("{prefix}:p:{}:{selected}", page - 1),
            "Previous",
        ));
    }
    if page < last {
        buttons.push(button(
            format!("{prefix}:p:{}:{selected}", page + 1),
            "Next",
        ));
    }
    if !buttons.is_empty() {
        components.push(CreateActionRow::Buttons(buttons));
    }
    ui::Panel {
        content: format!(
            "Choose a market to add a resolver. Page {} of {}.",
            page + 1,
            last + 1
        ),
        embed: None,
        components,
    }
}

pub(in crate::discord) fn start(
    view: &View,
    guild: GuildId,
    actor: Actor,
    id: Option<&MarketId>,
    selected: Option<UserId>,
) -> Result<ui::Panel, &'static str> {
    match (id, selected) {
        (Some(id), Some(selected)) => {
            let mut panel = confirmation(view, guild, actor, id, selected)?;
            if let Some(CreateActionRow::Buttons(buttons)) = panel.components.first_mut() {
                buttons.push(button(
                    format!("{}:b:{}:{selected}", ui::prefix(guild, actor), compact(id)?),
                    "Back",
                ));
            }
            Ok(panel)
        }
        (Some(id), None) => people(view, guild, actor, id, None),
        (None, _) => Ok(picker(view, guild, actor, 0, selected, None)),
    }
}

enum Action {
    Market(MarketId, Option<UserId>),
    People(MarketId, Option<UserId>),
    BackMarkets(MarketId, Option<UserId>),
    Page(usize, Option<UserId>),
}
fn selection(
    guild: GuildId,
    actor: Actor,
    custom_id: &str,
    kind: &ComponentInteractionDataKind,
) -> Result<Action, &'static str> {
    let parts = ui::scope(guild, actor, custom_id)?;
    match (parts.as_slice(), kind) {
        (["m", selected], ComponentInteractionDataKind::StringSelect { values }) => {
            let [id] = values.as_slice() else {
                return Err("Choose exactly one market.");
            };
            Ok(Action::Market(id.clone().into(), user(selected)?))
        }
        (["u", id], ComponentInteractionDataKind::UserSelect { values }) => {
            let [selected] = values.as_slice() else {
                return Err("Choose exactly one person.");
            };
            if selected.get() == 0 {
                return Err("Choose a valid person.");
            }
            Ok(Action::Market(expand(id)?, Some(UserId(selected.get()))))
        }
        (["b", id, selected], ComponentInteractionDataKind::Button) => {
            Ok(Action::People(expand(id)?, user(selected)?))
        }
        (["v", id, selected], ComponentInteractionDataKind::Button) => {
            Ok(Action::BackMarkets(expand(id)?, user(selected)?))
        }
        (["p", page, selected], ComponentInteractionDataKind::Button) => Ok(Action::Page(
            page.parse().map_err(|_| INVALID)?,
            user(selected)?,
        )),
        _ => Err(INVALID),
    }
}

fn navigate(
    view: &View,
    guild: GuildId,
    actor: Actor,
    action: Action,
) -> Result<ui::Panel, &'static str> {
    match action {
        Action::Market(id, selected) => start(view, guild, actor, Some(&id), selected),
        Action::People(id, selected) => people(view, guild, actor, &id, selected),
        Action::BackMarkets(id, selected) => {
            market(view, actor, &id)?;
            Ok(picker(view, guild, actor, 0, selected, Some(&id)))
        }
        Action::Page(page, selected) => Ok(picker(view, guild, actor, page, selected, None)),
    }
}

pub(super) async fn handle_component(store: &Store, http: &Http, component: &ComponentInteraction) {
    let actor = Actor {
        user_id: UserId(component.user.id.get()),
        bot: component.user.bot,
        moderator: component
            .member
            .as_ref()
            .and_then(|m| m.permissions)
            .is_some_and(|p| p.intersects(Permissions::ADMINISTRATOR | Permissions::MANAGE_GUILD)),
    };
    let guild = component
        .guild_id
        .map_or(GuildId(0), |id| GuildId(id.get()));
    let selection = selection(
        guild,
        actor,
        &component.data.custom_id,
        &component.data.kind,
    );
    let transport = if selection.is_ok() {
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
            let result = match selection {
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
                    Ok(view) => navigate(&view, guild, actor, action),
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
