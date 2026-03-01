use graphql_context::Context;
use graphql_model::Name;
use juniper::{FieldResult, GraphQLInputObject, graphql_object};
use name::{DiscordId, DiscordServerId};

/// Input for the createName mutation.
#[derive(GraphQLInputObject)]
#[graphql(description = "Input for creating a name for a Discord user in a server")]
pub struct CreateNameInput {
    /// An opaque identifier for the client performing the mutation.
    pub client_mutation_id: Option<String>,
    /// The Discord user ID.
    pub discord_id: String,
    /// The Discord server ID.
    pub discord_server_id: String,
    /// The nickname.
    pub name: String,
}

/// Payload returned by the createName mutation.
pub struct CreateNamePayload {
    pub client_mutation_id: Option<String>,
    pub name: Name,
}

#[graphql_object]
#[graphql(context = Context)]
impl CreateNamePayload {
    /// The client mutation ID that was passed in.
    fn client_mutation_id(&self) -> Option<&str> {
        self.client_mutation_id.as_deref()
    }

    /// The newly created name.
    fn name(&self) -> &Name {
        &self.name
    }
}

/// Root mutation for the nicknamer2 GraphQL API.
pub struct MutationRoot;

#[graphql_object]
#[graphql(context = Context)]
impl MutationRoot {
    /// Create a new name for a Discord user in a server.
    async fn create_name(
        context: &Context,
        input: CreateNameInput,
    ) -> FieldResult<CreateNamePayload> {
        let discord_id: u64 = input
            .discord_id
            .parse()
            .map_err(|_| "Invalid discord ID format")?;
        let discord_server_id: u64 = input
            .discord_server_id
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
                input.name,
            )
            .await?;

        Ok(CreateNamePayload {
            client_mutation_id: input.client_mutation_id,
            name: Name::from(created),
        })
    }
}
