use graphql_context::{Context, require_auth};
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
    async fn server(
        context: &Context,
        #[graphql(description = "The Discord server ID")] id: ID,
    ) -> FieldResult<Server> {
        require_auth(context).await?;

        let server_id: &str = &id;
        let server_id_u64 = server_id
            .parse::<u64>()
            .map_err(|_| "Invalid server ID format")?;

        if server_id_u64 == 0 {
            return Err("Server ID must be greater than 0".into());
        }

        let server = context
            .server_service
            .get_server(DiscordServerId(server_id_u64))
            .await?
            .ok_or("Server not found")?;

        Ok(Server::from(server))
    }

    /// Fetch any object by its global Relay ID.
    async fn node(
        context: &Context,
        #[graphql(description = "The global Relay ID")] id: ID,
    ) -> FieldResult<Option<NodeValue>> {
        require_auth(context).await?;

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

                let server = context
                    .server_service
                    .get_server(DiscordServerId(discord_server))
                    .await
                    .map_err(|e| format!("{e}"))?;

                Ok(server.map(|s| NodeValue::Server(Server::from(s))))
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
        require_auth(context).await?;

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

        let total_count = context.server_service.count_servers().await? as i32;

        let mut servers = context
            .server_service
            .list_servers(fetch_limit, cursor_value)
            .await?;

        let has_next_page = servers.len() > limit as usize;
        if has_next_page {
            servers.pop();
        }

        let edges: Vec<ServerEdge> = servers
            .into_iter()
            .map(|server| {
                let cursor = ServerCursor::new(server.id.0);
                ServerEdge {
                    cursor: cursor.encode(),
                    node: Server::from(server),
                }
            })
            .collect();

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
