use std::sync::Arc;

use axum::Extension;
use axum::routing::{MethodFilter, get, on};
use juniper_axum::extract::JuniperRequest;
use juniper_axum::graphiql;
use juniper_axum::response::JuniperResponse;

use graphql_context::Context;
use graphql_schema::Schema;

/// Custom GraphQL handler that injects our application context.
async fn graphql_handler(
    Extension(schema): Extension<Arc<Schema>>,
    Extension(context): Extension<Arc<Context>>,
    JuniperRequest(request): JuniperRequest,
) -> JuniperResponse {
    JuniperResponse(request.execute(&*schema, &*context).await)
}

/// Creates the axum Router with GraphQL and GraphiQL endpoints.
pub fn create_router(schema: Arc<Schema>, context: Arc<Context>) -> axum::Router {
    axum::Router::new()
        .route(
            "/graphql",
            on(MethodFilter::GET.or(MethodFilter::POST), graphql_handler),
        )
        .route("/graphiql", get(graphiql("/graphql", None)))
        .layer(Extension(schema))
        .layer(Extension(context))
}
