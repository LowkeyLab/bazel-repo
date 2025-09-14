pub(crate) mod v1 {
    use std::sync::Arc;

    use crate::{
        auth::{self, AuthState},
        name::web::NameState,
    };

    use axum::{
        Router,
        middleware::{from_fn, from_fn_with_state},
    };

    use tower::ServiceBuilder;
    use utoipa::{OpenApi, ToSchema};
    use utoipa_swagger_ui::SwaggerUi;

    /// Unified error response for all API endpoints
    #[derive(serde::Serialize, Debug, ToSchema)]
    pub struct ServerErrorResponse {
        pub error: String,
        pub message: String,
    }

    impl ServerErrorResponse {
        /// Create a new error response with just an error code and default message
        pub fn new(error: String) -> Self {
            Self {
                error: error.clone(),
                message: format!("An error occurred: {}", error),
            }
        }

        /// Create a new error response with both error code and custom message
        pub fn new_with_message(error: String, message: String) -> Self {
            Self { error, message }
        }
    }

    /// OpenAPI documentation for the v1 API
    #[derive(OpenApi)]
    #[openapi(
        paths(
            crate::auth::api::v1::json_login_handler,
            crate::name::api::v1::get_names_handler,
        ),
        components(
            schemas(
                crate::auth::api::v1::JsonLoginRequest,
                crate::auth::api::v1::LoginResponse,
                ServerErrorResponse,
                crate::name::api::v1::NameJson,
                crate::name::api::v1::NamesResponse,
            )
        ),
        tags(
            (name = "Authentication", description = "Authentication endpoints"),
            (name = "Names", description = "Name management endpoints")
        ),
        info(
            title = "Nicknamer API",
            version = "1.0.0",
            description = "API for managing Discord nicknames and authentication"
        )
    )]
    pub struct ApiDoc;

    const API_DOCS_PATH: &str = "/api-docs/openapi.json";
    const SWAGGER_UI_PATH: &str = "/swagger-ui";

    /// Creates the API routes for JSON API endpoints.
    pub fn create_api_router(
        auth_state: Arc<AuthState>,
        name_state: Arc<NameState>,
    ) -> axum::Router {
        let login_router = auth::api::v1::create_api_router(auth_state.clone());
        let names_router = crate::name::api::v1::create_api_router(name_state.clone());
        let protected_routes = names_router
            .layer(ServiceBuilder::new().layer(from_fn(auth::api::v1::require_auth_middleware)));
        let public_routes = login_router;
        let api_routes = public_routes.merge(protected_routes);

        Router::new()
            .merge(SwaggerUi::new(SWAGGER_UI_PATH).url(API_DOCS_PATH, ApiDoc::openapi()))
            .nest("/api/v1", api_routes)
            .layer(ServiceBuilder::new().layer(from_fn_with_state(
                auth_state,
                auth::api::v1::auth_user_middleware,
            )))
    }
}
