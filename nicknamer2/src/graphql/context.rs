use name_repo::Repo;
use name_service::Service;
use std::sync::Arc;

/// GraphQL context providing access to the name service.
pub struct Context {
    pub name_service: Arc<Service<Repo>>,
}

impl juniper::Context for Context {}
