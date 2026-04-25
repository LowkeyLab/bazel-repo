use graphql_context::{Context, require_auth};
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

/// A single entry in a batch name creation request.
#[derive(GraphQLInputObject)]
#[graphql(description = "A Discord user ID and nickname pair")]
pub struct NameEntry {
    /// The Discord user ID.
    pub discord_id: String,
    /// The nickname to assign.
    pub name: String,
}

/// Input for the createNames batch mutation.
#[derive(GraphQLInputObject)]
#[graphql(description = "Input for creating multiple names for Discord users in a server")]
pub struct CreateNamesInput {
    /// An opaque identifier for the client performing the mutation.
    pub client_mutation_id: Option<String>,
    /// The Discord server ID.
    pub discord_server_id: String,
    /// The list of user/name entries to create or update.
    pub names: Vec<NameEntry>,
}

/// Payload returned by the createNames batch mutation.
pub struct CreateNamesPayload {
    pub client_mutation_id: Option<String>,
    pub names: Vec<Name>,
}

#[graphql_object]
#[graphql(context = Context)]
impl CreateNamesPayload {
    /// The client mutation ID that was passed in.
    fn client_mutation_id(&self) -> Option<&str> {
        self.client_mutation_id.as_deref()
    }

    /// The created or updated names.
    fn names(&self) -> &[Name] {
        &self.names
    }
}

/// Input for the createServer mutation.
#[derive(GraphQLInputObject)]
#[graphql(description = "Input for creating a Discord server")]
pub struct CreateServerInput {
    /// An opaque identifier for the client performing the mutation.
    pub client_mutation_id: Option<String>,
    /// The Discord server ID.
    pub discord_server_id: String,
    /// The display name for this server.
    pub display_name: String,
}

/// Payload returned by the createServer mutation.
pub struct CreateServerPayload {
    pub client_mutation_id: Option<String>,
    pub server: graphql_model::Server,
}

#[graphql_object]
#[graphql(context = Context)]
impl CreateServerPayload {
    /// The client mutation ID that was passed in.
    fn client_mutation_id(&self) -> Option<&str> {
        self.client_mutation_id.as_deref()
    }

    /// The newly created server.
    fn server(&self) -> &graphql_model::Server {
        &self.server
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
        require_auth(context).await?;

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

        let id = context
            .name_service
            .create_name(
                DiscordId(discord_id),
                DiscordServerId(discord_server_id),
                input.name,
            )
            .await?;

        let created = context
            .name_service
            .get_name(id.discord_id, id.discord_server)
            .await?
            .ok_or("Failed to retrieve created name")?;

        Ok(CreateNamePayload {
            client_mutation_id: input.client_mutation_id,
            name: Name::from(created),
        })
    }

    /// Create or update names for multiple Discord users in a server (batch upsert).
    async fn create_names(
        context: &Context,
        input: CreateNamesInput,
    ) -> FieldResult<CreateNamesPayload> {
        require_auth(context).await?;

        let discord_server_id: u64 = input
            .discord_server_id
            .parse()
            .map_err(|_| "Invalid server ID format")?;

        if discord_server_id == 0 {
            return Err("Server ID must be greater than 0".into());
        }

        let mut entries = Vec::with_capacity(input.names.len());
        for entry in &input.names {
            let discord_id: u64 = entry
                .discord_id
                .parse()
                .map_err(|_| format!("Invalid discordId: {}", entry.discord_id))?;
            if discord_id == 0 {
                return Err("Discord ID must be greater than 0".into());
            }
            entries.push((DiscordId(discord_id), entry.name.clone()));
        }

        let server = DiscordServerId(discord_server_id);
        let created_names = context.name_service.create_names(server, entries).await?;

        let names = created_names.into_iter().map(Name::from).collect();

        Ok(CreateNamesPayload {
            client_mutation_id: input.client_mutation_id,
            names,
        })
    }

    /// Create a new Discord server.
    async fn create_server(
        context: &Context,
        input: CreateServerInput,
    ) -> FieldResult<CreateServerPayload> {
        require_auth(context).await?;

        let discord_server_id: u64 = input
            .discord_server_id
            .parse()
            .map_err(|_| "Invalid server ID format")?;

        if discord_server_id == 0 {
            return Err("Server ID must be greater than 0".into());
        }

        let id = context
            .server_service
            .create_server(DiscordServerId(discord_server_id), input.display_name)
            .await?;

        let created = context
            .server_service
            .get_server(id)
            .await?
            .ok_or("Failed to retrieve created server")?;

        Ok(CreateServerPayload {
            client_mutation_id: input.client_mutation_id,
            server: graphql_model::Server::from(created),
        })
    }
}
