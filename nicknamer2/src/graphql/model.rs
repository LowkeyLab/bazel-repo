use chrono::{DateTime, Utc};
use graphql_context::Context;
use graphql_relay::{Cursor, RelayId};
use juniper::{FieldResult, GraphQLInterface, ID, graphql_object};
use uuid::Uuid;

/// Pagination constants for the names connection
const DEFAULT_PAGE_SIZE: i32 = 10;
const MAX_PAGE_SIZE: i32 = 100;
const MIN_PAGE_SIZE: i32 = 1;

/// The Relay Node interface - all types with global IDs implement this
#[derive(GraphQLInterface)]
#[graphql(for = [Name, Server], context = Context)]
pub struct Node {
    /// The globally unique ID for this object
    pub id: ID,
}

/// A Discord server
pub struct Server {
    pub id: u64,
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
        RelayId::encode_server(self.id)
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
            Some(cursor.user_id_value())
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
                let cursor = Cursor::new(name.user_id);
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

/// A name associated with a user in a Discord server, exposed via GraphQL.
/// Internal storage for GraphQL Name type
pub struct Name {
    pub user_id: Uuid,
    pub server_id: u64,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[graphql_object(context = Context)]
#[graphql(impl = NodeValue, description = "A name associated with a user in a Discord server")]
impl Name {
    /// The globally unique Relay ID for this Name
    fn id(&self) -> ID {
        graphql_relay::RelayId::encode_name(self.user_id, self.server_id)
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

impl From<name::Name> for Name {
    fn from(n: name::Name) -> Self {
        Self {
            user_id: n.user_id,
            server_id: n.server_id,
            name: n.name,
            created_at: n.created_at,
            updated_at: n.updated_at,
        }
    }
}
