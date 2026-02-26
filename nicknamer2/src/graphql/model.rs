use chrono::{DateTime, Utc};
use graphql_context::Context;
use graphql_relay::{Cursor, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, MIN_PAGE_SIZE, RelayId};
use juniper::{FieldResult, GraphQLInterface, ID, graphql_object};
use name::{DiscordId, DiscordServerId, Name as NameEntity, NameId};

/// The Relay Node interface - all types with global IDs implement this
#[derive(GraphQLInterface)]
#[graphql(for = [Name, Server], context = Context)]
pub struct Node {
    /// The globally unique ID for this object
    pub id: ID,
}

/// A Discord server
pub struct Server {
    pub id: DiscordServerId,
}

/// An edge in the names connection, containing a name and its cursor
pub struct NameEdge {
    pub cursor: String,
    pub node: Name,
}

#[graphql_object(context = Context)]
impl NameEdge {
    /// The cursor for this edge, used for pagination
    fn cursor(&self) -> &str {
        &self.cursor
    }

    /// The Name node
    fn node(&self) -> &Name {
        &self.node
    }
}

/// PageInfo for Relay pagination
pub struct PageInfo {
    pub has_next_page: bool,
    pub has_previous_page: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
}

#[graphql_object(context = Context)]
impl PageInfo {
    /// Whether there are more items when paginating forwards
    fn has_next_page(&self) -> bool {
        self.has_next_page
    }

    /// Whether there are more items when paginating backwards
    fn has_previous_page(&self) -> bool {
        self.has_previous_page
    }

    /// The cursor of the first item in the list
    fn start_cursor(&self) -> Option<&str> {
        self.start_cursor.as_deref()
    }

    /// The cursor of the last item in the list
    fn end_cursor(&self) -> Option<&str> {
        self.end_cursor.as_deref()
    }
}

/// A connection to a list of names in a server
pub struct NameConnection {
    pub edges: Vec<NameEdge>,
    pub page_info: PageInfo,
}

#[graphql_object(context = Context)]
impl NameConnection {
    /// The list of edges
    fn edges(&self) -> &[NameEdge] {
        &self.edges
    }

    /// Information about pagination
    fn page_info(&self) -> &PageInfo {
        &self.page_info
    }
}

#[graphql_object(context = Context)]
#[graphql(impl = NodeValue, description = "A Discord server")]
impl Server {
    /// The server ID (global Relay ID)
    fn id(&self) -> ID {
        RelayId::encode_server(self.id.0)
    }

    /// The Discord server ID
    fn server_id(&self) -> String {
        self.id.0.to_string()
    }

    /// Paginated list of names in this server
    async fn names(
        &self,
        context: &Context,
        first: Option<i32>,
        after: Option<String>,
    ) -> FieldResult<NameConnection> {
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
            let cursor = Cursor::decode(&after_cursor)?;
            Some(cursor.discord_id_value())
        } else {
            None
        };

        // Track if we have a cursor for pagination info
        let has_cursor = cursor_value.is_some();

        // Request one extra item to determine if there's a next page
        let fetch_limit = (limit + 1) as i64;

        // Fetch names from the service
        let mut names = context
            .name_service
            .list_names(self.id, fetch_limit, cursor_value)
            .await?;

        // Determine if there's a next page
        let has_next_page = names.len() > limit as usize;
        if has_next_page {
            names.pop(); // Remove the extra item
        }

        // Build edges with cursors
        let edges: Vec<NameEdge> = names
            .into_iter()
            .map(|name| {
                let cursor = Cursor::new(name.id.discord_id);
                NameEdge {
                    cursor: cursor.encode(),
                    node: Name::from(name),
                }
            })
            .collect();

        // Build page info
        let page_info = PageInfo {
            has_next_page,
            has_previous_page: has_cursor, // If we used a cursor, there are previous pages
            start_cursor: edges.first().map(|e| e.cursor.clone()),
            end_cursor: edges.last().map(|e| e.cursor.clone()),
        };

        Ok(NameConnection { edges, page_info })
    }
}

/// An edge in the servers connection, containing a server and its cursor
pub struct ServerEdge {
    pub cursor: String,
    pub node: Server,
}

#[graphql_object(context = Context)]
impl ServerEdge {
    /// The cursor for this edge, used for pagination
    fn cursor(&self) -> &str {
        &self.cursor
    }

    /// The Server node
    fn node(&self) -> &Server {
        &self.node
    }
}

/// A connection to a list of servers
pub struct ServerConnection {
    pub edges: Vec<ServerEdge>,
    pub page_info: PageInfo,
}

#[graphql_object(context = Context)]
impl ServerConnection {
    /// The list of edges
    fn edges(&self) -> &[ServerEdge] {
        &self.edges
    }

    /// Information about pagination
    fn page_info(&self) -> &PageInfo {
        &self.page_info
    }
}

/// A name associated with a user in a Discord server, exposed via GraphQL.
/// Internal storage for GraphQL Name type
pub struct Name {
    pub id: NameId,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[graphql_object(context = Context)]
#[graphql(impl = NodeValue, description = "A name associated with a user in a Discord server")]
impl Name {
    /// The globally unique Relay ID for this Name
    fn id(&self) -> ID {
        graphql_relay::RelayId::encode_name(self.id.discord_id, self.id.discord_server.0)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

impl From<NameEntity> for Name {
    fn from(n: NameEntity) -> Self {
        Self {
            id: n.id,
            name: n.name,
            created_at: n.created_at,
            updated_at: n.updated_at,
        }
    }
}
