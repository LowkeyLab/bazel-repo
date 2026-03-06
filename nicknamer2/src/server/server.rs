use std::sync::Arc;

use auth_claims::AuthService;
use axum::Extension;
use axum::http::HeaderMap;
use axum::routing::{MethodFilter, get, on};
use juniper_axum::extract::JuniperRequest;
use juniper_axum::graphiql;
use juniper_axum::response::JuniperResponse;
use name_repo::Repo;
use name_service::Service;
use tower_http::cors::CorsLayer;

use graphql_context::Context;
use graphql_schema::Schema;

/// Custom GraphQL handler that creates a per-request context with auth info.
async fn graphql_handler(
    Extension(schema): Extension<Arc<Schema>>,
    Extension(name_service): Extension<Arc<Service<Repo>>>,
    Extension(jwks_validator): Extension<Arc<dyn AuthService>>,
    headers: HeaderMap,
    JuniperRequest(request): JuniperRequest,
) -> JuniperResponse {
    let auth_token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let context = Context {
        name_service,
        jwks_validator,
        auth_token,
    };

    JuniperResponse(request.execute(&*schema, &context).await)
}

/// Creates the axum Router with GraphQL and GraphiQL endpoints.
pub fn create_router(
    schema: Arc<Schema>,
    name_service: Arc<Service<Repo>>,
    jwks_validator: Arc<dyn AuthService>,
) -> axum::Router {
    axum::Router::new()
        .route(
            "/graphql",
            on(MethodFilter::GET.or(MethodFilter::POST), graphql_handler),
        )
        .route("/graphiql", get(graphiql("/graphql", None)))
        .layer(CorsLayer::permissive())
        .layer(Extension(schema))
        .layer(Extension(name_service))
        .layer(Extension(jwks_validator))
}
