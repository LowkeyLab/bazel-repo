use std::sync::Arc;

use auth::JwksValidator;
use name_repo::Repo;
use name_service::Service;

/// GraphQL context providing access to the name service and authentication.
pub struct Context {
    pub name_service: Arc<Service<Repo>>,
    pub jwks_validator: Arc<JwksValidator>,
    pub auth_token: Option<String>,
}

impl juniper::Context for Context {}
