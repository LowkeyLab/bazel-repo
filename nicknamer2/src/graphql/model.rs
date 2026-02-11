use chrono::{DateTime, Utc};
use juniper::GraphQLObject;
use uuid::Uuid;

/// A name associated with a user in a Discord server, exposed via GraphQL.
#[derive(GraphQLObject)]
#[graphql(description = "A name associated with a user in a Discord server")]
pub struct Name {
    pub user_id: Uuid,
    /// Discord server ID represented as a string (GraphQL has no u64 scalar).
    pub server_id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<name::Name> for Name {
    fn from(n: name::Name) -> Self {
        Self {
            user_id: n.user_id,
            server_id: n.server_id.to_string(),
            name: n.name,
            created_at: n.created_at,
            updated_at: n.updated_at,
        }
    }
}
