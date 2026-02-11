use juniper::{EmptyMutation, EmptySubscription, RootNode};

use graphql_context::Context;
use graphql_query::QueryRoot;

/// The GraphQL schema type for the nicknamer2 API.
pub type Schema = RootNode<QueryRoot, EmptyMutation<Context>, EmptySubscription<Context>>;

/// Creates a new GraphQL schema instance.
pub fn create_schema() -> Schema {
    Schema::new(QueryRoot, EmptyMutation::new(), EmptySubscription::new())
}
