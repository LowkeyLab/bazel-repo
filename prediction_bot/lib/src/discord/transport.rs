//! The outgoing Discord interaction boundary used by all handlers.
use serenity::{
    all::{CommandInteraction, ComponentInteraction, ModalInteraction},
    builder::{CreateInteractionResponse, EditInteractionResponse},
    http::Http,
};

#[serenity::async_trait]
pub trait InteractionTransport: Send + Sync {
    async fn acknowledge(&self) -> serenity::Result<()>;
    async fn edit(&self, response: EditInteractionResponse) -> serenity::Result<()>;
    async fn respond(&self, response: CreateInteractionResponse) -> serenity::Result<()>;
}

pub(super) enum SerenityTransport<'a> {
    Command(&'a CommandInteraction, &'a Http),
    Modal(&'a ModalInteraction, &'a Http),
    Component(&'a ComponentInteraction, &'a Http),
    ComponentUpdate(&'a ComponentInteraction, &'a Http),
}

#[serenity::async_trait]
impl InteractionTransport for SerenityTransport<'_> {
    async fn acknowledge(&self) -> serenity::Result<()> {
        match self {
            Self::Command(interaction, http) => interaction.defer_ephemeral(http).await,
            Self::Modal(interaction, http) => interaction.defer_ephemeral(http).await,
            Self::Component(interaction, http) => interaction.defer_ephemeral(http).await,
            Self::ComponentUpdate(interaction, http) => interaction.defer(http).await,
        }
    }
    async fn edit(&self, response: EditInteractionResponse) -> serenity::Result<()> {
        match self {
            Self::Command(interaction, http) => interaction.edit_response(http, response).await,
            Self::Modal(interaction, http) => interaction.edit_response(http, response).await,
            Self::Component(interaction, http) | Self::ComponentUpdate(interaction, http) => {
                interaction.edit_response(http, response).await
            }
        }
        .map(|_| ())
    }
    async fn respond(&self, response: CreateInteractionResponse) -> serenity::Result<()> {
        match self {
            Self::Command(interaction, http) => interaction.create_response(http, response).await,
            Self::Modal(interaction, http) => interaction.create_response(http, response).await,
            Self::Component(interaction, http) | Self::ComponentUpdate(interaction, http) => {
                interaction.create_response(http, response).await
            }
        }
    }
}
