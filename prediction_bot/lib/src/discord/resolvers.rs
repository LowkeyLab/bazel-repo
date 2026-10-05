//! Resolver management UI and the external membership capability.
use super::{
    deferred_response, reply, safe_error,
    transport::{InteractionTransport, SerenityTransport},
    truncate_to, ui,
};
use crate::{
    domain::{Actor, Command, MembershipEvidence, Status},
    store::{Store, View},
    types::{GuildId, MarketId, UserId},
};
use serenity::{
    all::{ButtonStyle, ComponentInteraction, ComponentInteractionDataKind, Permissions},
    builder::{CreateActionRow, CreateButton, CreateEmbed, EditInteractionResponse},
    http::{Http, HttpError},
};

mod add;
pub(super) use add::start;

/// Verify present human/bot membership, confirmed absence, or inability to verify.
/// Implementations must obtain fresh evidence for the supplied guild and user.
#[serenity::async_trait]
pub trait MembershipVerifier: Send + Sync {
    async fn verify(&self, guild: GuildId, user: UserId) -> MembershipEvidence;
}

#[serenity::async_trait]
impl MembershipVerifier for Http {
    async fn verify(&self, guild: GuildId, user: UserId) -> MembershipEvidence {
        if guild.0 == 0 || user.0 == 0 {
            return MembershipEvidence::Unavailable;
        }
        match self.get_member(guild.0.into(), user.0.into()).await {
            Ok(member) if member.user.id.get() == user.0 => MembershipEvidence::Present {
                user_id: user,
                bot: member.user.bot,
            },
            Err(serenity::Error::Http(HttpError::UnsuccessfulRequest(response)))
                if response.status_code.as_u16() == 404 && response.error.code == 10007 =>
            {
                MembershipEvidence::Absent { user_id: user }
            }
            _ => MembershipEvidence::Unavailable,
        }
    }
}

pub(super) mod remove;

pub(super) fn is_control(custom_id: &str) -> bool {
    matches!(custom_id.split(':').nth(3), Some("a" | "d")) || add::is_control(custom_id)
}

pub(super) fn confirmation(
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
    let control = format!("{}:a:{compact}:{user}", ui::prefix(guild, actor));
    Ok(ui::Panel {
        content: format!(
            "Add <@{user}> as an additional resolver for market {id}? Confirm to grant permission to settle this market."
        ),
        embed: Some(CreateEmbed::new().title(truncate_to(&market.question, 256))),
        components: vec![CreateActionRow::Buttons(vec![
            CreateButton::new(control)
                .label("Confirm add resolver")
                .style(ButtonStyle::Primary),
        ])],
    })
}

pub(super) fn parse(
    guild: GuildId,
    actor: Actor,
    custom_id: &str,
    kind: &ComponentInteractionDataKind,
) -> Result<Command, &'static str> {
    if custom_id.split(':').nth(3) == Some("d") {
        return remove::parse(guild, actor, custom_id, kind);
    }
    let parts = ui::scope(guild, actor, custom_id)?;
    let (["a", id, user], ComponentInteractionDataKind::Button) = (parts.as_slice(), kind) else {
        return Err("Invalid resolver confirmation. Run /market resolver add again.");
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
    Ok(Command::AddResolver { id, user_id })
}

async fn execute_request(
    store: &Store,
    verifier: &dyn MembershipVerifier,
    guild: GuildId,
    actor: Actor,
    command: &Command,
    interaction_id: u64,
) -> EditInteractionResponse {
    let verification = async {
        match command {
            Command::AddResolver { user_id, .. } => verifier.verify(guild, *user_id).await,
            _ => MembershipEvidence::Unavailable,
        }
    };
    let result = store
        .execute_with_membership(
            guild,
            &format!("discord:{interaction_id}"),
            actor,
            command,
            verification,
        )
        .await;
    match result {
        Ok(message) => reply(&message),
        Err(error) => reply(&safe_error(&error)),
    }
    .embeds(vec![])
    .components(vec![])
}

/// Execute a confirmed resolver change through the real store and private deferred response.
pub async fn execute_interaction(
    transport: &dyn InteractionTransport,
    store: &Store,
    verifier: &dyn MembershipVerifier,
    guild: GuildId,
    actor: Actor,
    command: &Command,
    interaction_id: u64,
) {
    deferred_response(
        transport,
        store.audit().as_ref(),
        Some(guild),
        interaction_id,
        || execute_request(store, verifier, guild, actor, command, interaction_id),
    )
    .await;
}

pub(super) async fn handle_component(store: &Store, http: &Http, component: &ComponentInteraction) {
    if add::is_control(&component.data.custom_id) {
        add::handle_component(store, http, component).await;
        return;
    }
    let actor = Actor {
        user_id: UserId(component.user.id.get()),
        bot: component.user.bot,
        moderator: component
            .member
            .as_ref()
            .and_then(|member| member.permissions)
            .is_some_and(|p| p.intersects(Permissions::ADMINISTRATOR | Permissions::MANAGE_GUILD)),
    };
    let guild = component
        .guild_id
        .map_or(GuildId(0), |id| GuildId(id.get()));
    let request = parse(
        guild,
        actor,
        &component.data.custom_id,
        &component.data.kind,
    );
    let transport = if request.is_ok() {
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
            match request {
                Ok(command) => {
                    execute_request(store, http, guild, actor, &command, component.id.get()).await
                }
                Err(message) => {
                    super::rejected(
                        store.audit().as_ref(),
                        component.guild_id.map(|id| GuildId(id.get())),
                        component.id.get(),
                    );
                    reply(message)
                }
            }
        },
    )
    .await;
}
