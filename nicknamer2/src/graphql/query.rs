use juniper::{FieldResult, graphql_object};
use uuid::Uuid;

use graphql_context::Context;
use graphql_model::Name;

/// Root query for the nicknamer2 GraphQL API.
pub struct QueryRoot;

#[graphql_object]
#[graphql(context = Context)]
impl QueryRoot {
    /// Retrieve the name for a user in a specific Discord server.
    async fn name(
        context: &Context,
        #[graphql(description = "The user's unique identifier.")] user_id: Uuid,
        #[graphql(description = "The Discord server ID as a string.")] server_id: String,
    ) -> FieldResult<Option<Name>> {
        let server_id: u64 = server_id
            .parse()
            .map_err(|_| "server_id must be a valid u64")?;

        let result = context
            .name_service
            .get_name(user_id, server_id)
            .await
            .map_err(|e| format!("{e}"))?;

        Ok(result.map(Name::from))
    }
}
