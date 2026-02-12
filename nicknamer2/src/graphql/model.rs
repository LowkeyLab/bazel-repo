use chrono::{DateTime, Utc};
use juniper::{GraphQLInterface, ID, graphql_object};
use uuid::Uuid;

/// The Relay Node interface - all types with global IDs implement this
#[derive(GraphQLInterface)]
#[graphql(for = Name)]
pub struct Node {
    /// The globally unique ID for this object
    pub id: ID,
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

#[graphql_object]
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
