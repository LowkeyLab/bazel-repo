use graphql_context::Context;
use graphql_model::Name;
use juniper::{FieldResult, ID, graphql_object};
use name::{DiscordId, DiscordServerId};

/// Root mutation for the nicknamer2 GraphQL API.
pub struct MutationRoot;

#[graphql_object]
#[graphql(context = Context)]
impl MutationRoot {
    /// Create a new name for a Discord user in a server.
    async fn create_name(
        context: &Context,
        #[graphql(description = "The Discord user ID")] discord_id: ID,
        #[graphql(description = "The Discord server ID")] discord_server_id: ID,
        #[graphql(description = "The nickname")] name: String,
    ) -> FieldResult<Name> {
        let discord_id: u64 = discord_id
            .parse()
            .map_err(|_| "Invalid discord ID format")?;
        let discord_server_id: u64 = discord_server_id
            .parse()
            .map_err(|_| "Invalid server ID format")?;

        if discord_id == 0 {
            return Err("Discord ID must be greater than 0".into());
        }
        if discord_server_id == 0 {
            return Err("Server ID must be greater than 0".into());
        }

        let created = context
            .name_service
            .create_name(
                DiscordId(discord_id),
                DiscordServerId(discord_server_id),
                name,
            )
            .await?;

        Ok(Name::from(created))
    }
}
