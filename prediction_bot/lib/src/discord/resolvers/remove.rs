//! Supplied-argument removal confirmation, independent of target verification.
use super::{Actor, Command, GuildId, MarketId, Status, UserId, View, truncate_to, ui};
use serenity::{
    all::{ButtonStyle, ComponentInteractionDataKind},
    builder::{CreateActionRow, CreateButton, CreateEmbed},
};

pub(in crate::discord) fn confirmation(
    view: &View,
    guild: GuildId,
    actor: Actor,
    id: &MarketId,
    user: UserId,
) -> Result<ui::Panel, &'static str> {
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
    if user.0 == 0 {
        return Err("Choose a valid person.");
    }
    uuid::Uuid::parse_str(&id.0).map_err(|_| "Invalid market ID.")?;
    let compact = id.0.replace('-', "");
    let control = format!("{}:d:{compact}:{user}", ui::prefix(guild, actor));
    Ok(ui::Panel {
        content: format!(
            "Remove <@{user}> as an additional resolver for market {id}? Confirm removal of this explicit assignment. Separate authority as the market creator or a current moderator remains unchanged."
        ),
        embed: Some(CreateEmbed::new().title(truncate_to(&market.question, 256))),
        components: vec![CreateActionRow::Buttons(vec![
            CreateButton::new(control)
                .label("Confirm remove resolver")
                .style(ButtonStyle::Danger),
        ])],
    })
}

pub(in crate::discord) fn parse(
    guild: GuildId,
    actor: Actor,
    custom_id: &str,
    kind: &ComponentInteractionDataKind,
) -> Result<Command, &'static str> {
    let parts = ui::scope(guild, actor, custom_id)?;
    let (["d", id, user], ComponentInteractionDataKind::Button) = (parts.as_slice(), kind) else {
        return Err("Invalid resolver confirmation. Run /market resolver remove again.");
    };
    if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid market ID.");
    }
    // Preserve the exact stored UUID spelling, including historical uppercase IDs.
    let id = format!(
        "{}-{}-{}-{}-{}",
        &id[..8],
        &id[8..12],
        &id[12..16],
        &id[16..20],
        &id[20..]
    )
    .into();
    let user_id = user
        .parse::<UserId>()
        .ok()
        .filter(|id| id.0 != 0)
        .ok_or("Choose a valid person.")?;
    Ok(Command::RemoveResolver { id, user_id })
}
