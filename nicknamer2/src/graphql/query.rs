use graphql_context::Context;
use graphql_model::{Name, NodeValue, Server};
use graphql_relay::RelayId;
use juniper::{FieldResult, ID, graphql_object};

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

        Ok(Server { id: server_id_u64 })
    }

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
            "Server" => {
                let server_id = relay_id
                    .as_server()
                    .map_err(|e| format!("Invalid Server ID: {}", e))?;

                if server_id == 0 {
                    return Err("Server ID must be greater than 0".into());
                }

                Ok(Some(NodeValue::Server(Server { id: server_id })))
            }
            _ => Ok(None),
        }
    }
}
