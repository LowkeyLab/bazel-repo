use std::sync::Arc;

use auth_claims::{AuthError, AuthService};
use discord_server_repo::Repo as ServerRepo;
use discord_server_service::Service as ServerService;
use juniper::FieldResult;
use name_repo::Repo;
use name_service::Service;

/// GraphQL context providing access to services and authentication.
pub struct Context {
    pub name_service: Arc<Service<Repo>>,
    pub server_service: Arc<ServerService<ServerRepo>>,
    pub jwks_validator: Arc<dyn AuthService>,
    pub auth_token: Option<String>,
}

impl juniper::Context for Context {}

/// Validates the auth token from the context. Returns Ok(()) if valid.
pub async fn require_auth(context: &Context) -> FieldResult<()> {
    let header_value = context
        .auth_token
        .as_deref()
        .ok_or(AuthError::MissingToken)?;

    context
        .jwks_validator
        .validate_auth_header(header_value)
        .await
        .map_err(juniper::FieldError::from)?;

    Ok(())
}
