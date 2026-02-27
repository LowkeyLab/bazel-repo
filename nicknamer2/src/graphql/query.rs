use graphql_context::Context;
use graphql_model::{Name, NodeValue, PageInfo, Server, ServerConnection, ServerEdge};
use graphql_relay::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, MIN_PAGE_SIZE, RelayId, ServerCursor};
use juniper::{FieldResult, ID, graphql_object};
use name::{DiscordId, DiscordServerId};

/// Root query for the nicknamer2 GraphQL API.
pub struct QueryRoot;

#[graphql_object]
#[graphql(context = Context)]
impl QueryRoot {
    /// Fetch a Discord server by its ID
    fn server(#[graphql(description = "The Discord server ID")] id: ID) -> FieldResult<Server> {
        let server_id: &str = &id;
        let server_id_u64 = server_id
            .parse::<u64>()
            .map_err(|_| "Invalid server ID format")?;

        if server_id_u64 == 0 {
            return Err("Server ID must be greater than 0".into());
        }

        Ok(Server {
            id: DiscordServerId(server_id_u64),
        })
    }

    /// Fetch any object by its global Relay ID.
    async fn node(
        context: &Context,
        #[graphql(description = "The global Relay ID")] id: ID,
    ) -> FieldResult<Option<NodeValue>> {
        let relay_id = RelayId::decode(&id).map_err(|e| format!("Invalid ID: {}", e))?;

        match relay_id.type_name.as_str() {
            "Name" => {
                let (discord_id, discord_server) = relay_id
                    .as_name()
                    .map_err(|e| format!("Invalid Name ID: {}", e))?;

                if discord_id == 0 {
                    return Err("Discord ID must be greater than 0".into());
                }
                if discord_server == 0 {
                    return Err("Server ID must be greater than 0".into());
                }

                let result = context
                    .name_service
                    .get_name(DiscordId(discord_id), DiscordServerId(discord_server))
                    .await
                    .map_err(|e| format!("{e}"))?;

                Ok(result.map(Name::from).map(NodeValue::Name))
            }
            "Server" => {
                let discord_server = relay_id
                    .as_server()
                    .map_err(|e| format!("Invalid Server ID: {}", e))?;

                if discord_server == 0 {
                    return Err("Server ID must be greater than 0".into());
                }

                Ok(Some(NodeValue::Server(Server {
                    id: DiscordServerId(discord_server),
                })))
            }
            _ => Ok(None),
        }
    }

    /// Paginated list of all servers
    async fn servers(
        context: &Context,
        #[graphql(description = "Number of servers to return")] first: Option<i32>,
        #[graphql(description = "Cursor to paginate after")] after: Option<String>,
    ) -> FieldResult<ServerConnection> {
        // Validate and apply pagination limits
        let requested = first.unwrap_or(DEFAULT_PAGE_SIZE);
        if requested < MIN_PAGE_SIZE {
            return Err(format!(
                "Argument 'first' must be at least {}, got {}",
                MIN_PAGE_SIZE, requested
            )
            .into());
        }
        let limit = requested.min(MAX_PAGE_SIZE);

        // Decode cursor if provided
        let cursor_value = if let Some(after_cursor) = after {
            let cursor = ServerCursor::decode(&after_cursor)?;
            Some(DiscordServerId(cursor.server_id_value()))
        } else {
            None
        };

        // Track if we have a cursor for pagination info
        let has_cursor = cursor_value.is_some();

        // Request one extra item to determine if there's a next page
        let fetch_limit = (limit + 1) as i64;

        // Fetch total count and servers from the service
        let total_count = context.name_service.count_servers().await? as i32;

        let mut servers = context
            .name_service
            .list_servers(fetch_limit, cursor_value)
            .await?;

        // Determine if there's a next page
        let has_next_page = servers.len() > limit as usize;
        if has_next_page {
            servers.pop(); // Remove the extra item
        }

        // Build edges with cursors
        let edges: Vec<ServerEdge> = servers
            .into_iter()
            .map(|discord_server| {
                let cursor = ServerCursor::new(discord_server.0);
                ServerEdge {
                    cursor: cursor.encode(),
                    node: Server { id: discord_server },
                }
            })
            .collect();

        // Build page info
        let page_info = PageInfo {
            has_next_page,
            has_previous_page: has_cursor,
            start_cursor: edges.first().map(|e| e.cursor.clone()),
            end_cursor: edges.last().map(|e| e.cursor.clone()),
        };

        Ok(ServerConnection {
            edges,
            page_info,
            total_count,
        })
    }
}
