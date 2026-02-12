use juniper::{FieldResult, ID, graphql_object};

use graphql_context::Context;
use graphql_model::{Name, NodeValue};
use graphql_relay::RelayId;

/// Root query for the nicknamer2 GraphQL API.
pub struct QueryRoot;

#[graphql_object]
#[graphql(context = Context)]
impl QueryRoot {
    /// Fetch any object by its global Relay ID.
    async fn node(
        context: &Context,
        #[graphql(description = "The global Relay ID")] id: ID,
    ) -> FieldResult<Option<NodeValue>> {
        let relay_id = RelayId::decode(&id).map_err(|e| format!("Invalid ID: {}", e))?;

        match relay_id.type_name.as_str() {
            "Name" => {
                let (user_id, server_id) = relay_id
                    .as_name()
                    .map_err(|e| format!("Invalid Name ID: {}", e))?;

                let result = context
                    .name_service
                    .get_name(user_id, server_id)
                    .await
                    .map_err(|e| format!("{e}"))?;

                Ok(result.map(Name::from).map(NodeValue::Name))
            }
            _ => Ok(None),
        }
    }
}
