use juniper::{EmptySubscription, RootNode};

use graphql_context::Context;
use graphql_mutation::MutationRoot;
use graphql_query::QueryRoot;

/// The GraphQL schema type for the nicknamer2 API.
pub type Schema = RootNode<QueryRoot, MutationRoot, EmptySubscription<Context>>;

pub fn create_schema() -> Schema {
    Schema::new(QueryRoot, MutationRoot, EmptySubscription::new())
}
