use crate::name::web::NameState;
use crate::name::{Name, NameService};
use crate::web::api::v1::ServerErrorResponse;
use axum::{
    Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Json},
    routing::{get, put},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use utoipa::ToSchema;

/// JSON representation of a Name for API responses.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NameJson {
    /// Unique identifier for the name
    id: u32,
    /// Discord user ID associated with the name
    discord_id: u64,
    /// The actual name/nickname
    name: String,
    /// Server ID associated with the name
    server_id: String,
}

impl From<Name> for NameJson {
    fn from(name: Name) -> Self {
        Self {
            id: name.id(),
            discord_id: name.discord_id(),
            name: name.name().to_string(),
            server_id: name.server_id().to_string(),
        }
    }
}

/// API response for listing all names.
#[derive(Debug, Serialize, ToSchema)]
pub struct NamesResponse {
    /// List of names
    names: Vec<NameJson>,
    /// Total number of names
    count: usize,
}

/// Query parameters for filtering names by server.
#[derive(Debug, Deserialize, ToSchema)]
pub struct NamesQuery {
    /// Optional server ID to filter names by
    #[serde(default)]
    server_id: Option<String>,
}

/// Request body for creating a new name.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateNameRequest {
    /// Discord user ID associated with the name
    pub discord_id: u64,
    /// The actual name/nickname
    pub name: String,
    /// Server ID where the name is used
    pub server_id: String,
}

/// Request body for updating an existing name.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateNameRequest {
    /// The new name/nickname
    pub name: String,
}

/// Handler for GET /api/v1/names - Returns all names in JSON format.
#[tracing::instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/v1/names",
    params(
        ("server_id" = Option<String>, Query, description = "Optional server ID to filter names by")
    ),
    responses(
        (status = 200, description = "Successfully retrieved names", body = NamesResponse),
        (status = 500, description = "Internal server error", body = ServerErrorResponse)
    ),
    tag = "Names"
)]
pub async fn get_names_handler(
    State(state): State<Arc<NameState>>,
    Query(query): Query<NamesQuery>,
) -> Result<Json<NamesResponse>, (StatusCode, Json<ServerErrorResponse>)> {
    let service = NameService::new(&state.db);

    let names_result = match query.server_id {
        Some(server_id) => service.get_names_by_server(&server_id).await,
        None => service.get_all_names().await,
    };

    match names_result {
        Ok(names) => {
            let json_names: Vec<NameJson> = names.into_iter().map(NameJson::from).collect();
            let count = json_names.len();

            Ok(Json(NamesResponse {
                names: json_names,
                count,
            }))
        }
        Err(err) => {
            tracing::error!("Failed to get names: {}", err);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ServerErrorResponse::new(
                    "Failed to retrieve names".to_string(),
                )),
            ))
        }
    }
}

/// Handler for POST /api/v1/names - Creates a new name.
#[tracing::instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/v1/names",
    request_body = CreateNameRequest,
    responses(
        (status = 201, description = "Successfully created name", body = NameJson),
        (status = 400, description = "Bad request", body = ServerErrorResponse),
        (status = 409, description = "Name already exists for this Discord ID and server", body = ServerErrorResponse),
        (status = 500, description = "Internal server error", body = ServerErrorResponse)
    ),
    tag = "Names"
)]
pub async fn create_name_handler(
    State(state): State<Arc<NameState>>,
    Json(request): Json<CreateNameRequest>,
) -> Result<(StatusCode, Json<NameJson>), (StatusCode, Json<ServerErrorResponse>)> {
    let service = NameService::new(&state.db);

    match service
        .create_name(request.discord_id, request.name, request.server_id)
        .await
    {
        Ok(name) => Ok((StatusCode::CREATED, Json(NameJson::from(name)))),
        Err(crate::name::NameServiceError::DuplicateEntryError(discord_id, server_id)) => {
            tracing::warn!(
                "Duplicate name entry for Discord ID {} in server {}",
                discord_id,
                server_id
            );
            Err((
                StatusCode::CONFLICT,
                Json(ServerErrorResponse::new(format!(
                    "Name already exists for Discord ID {} in server '{}'",
                    discord_id, server_id
                ))),
            ))
        }
        Err(err) => {
            tracing::error!("Failed to create name: {}", err);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ServerErrorResponse::new(
                    "Failed to create name".to_string(),
                )),
            ))
        }
    }
}

/// Handler for PUT /api/v1/names/{discord_id}/servers/{server_id} - Updates an existing name.
#[tracing::instrument(skip(state))]
#[utoipa::path(
    put,
    path = "/api/v1/names/{discord_id}/servers/{server_id}",
    params(
        ("discord_id" = u64, Path, description = "Discord user ID associated with the name"),
        ("server_id" = String, Path, description = "Server ID where the name is used")
    ),
    request_body = UpdateNameRequest,
    responses(
        (status = 200, description = "Successfully updated name", body = NameJson),
        (status = 400, description = "Bad request", body = ServerErrorResponse),
        (status = 404, description = "Name not found for this Discord ID and server", body = ServerErrorResponse),
        (status = 500, description = "Internal server error", body = ServerErrorResponse)
    ),
    tag = "Names"
)]
pub async fn update_name_by_discord_server_handler(
    State(state): State<Arc<NameState>>,
    Path((discord_id, server_id)): Path<(u64, String)>,
    Json(request): Json<UpdateNameRequest>,
) -> Result<Json<NameJson>, (StatusCode, Json<ServerErrorResponse>)> {
    let service = NameService::new(&state.db);

    match service
        .update_name_by_discord_server(discord_id, &server_id, request.name)
        .await
    {
        Ok(updated_name) => Ok(Json(NameJson::from(updated_name))),
        Err(crate::name::NameServiceError::NameNotFoundByDiscordServer(discord_id, server_id)) => {
            tracing::warn!(
                "Name not found for Discord ID {} in server {}",
                discord_id,
                server_id
            );
            Err((
                StatusCode::NOT_FOUND,
                Json(ServerErrorResponse::new(format!(
                    "Name not found for Discord ID {} in server '{}'",
                    discord_id, server_id
                ))),
            ))
        }
        Err(err) => {
            tracing::error!("Failed to update name: {}", err);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ServerErrorResponse::new(
                    "Failed to update name".to_string(),
                )),
            ))
        }
    }
}

/// Handler for GET /api/v1/names/export - Exports all names as a YAML file.
#[tracing::instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/v1/names/export",
    params(
        ("server_id" = Option<String>, Query, description = "Optional server ID to filter names by")
    ),
    responses(
        (status = 200, description = "YAML file with discord_id: name mappings", content_type = "application/x-yaml"),
        (status = 500, description = "Internal server error", body = ServerErrorResponse)
    ),
    tag = "Names"
)]
pub async fn export_names_handler(
    State(state): State<Arc<NameState>>,
    Query(query): Query<NamesQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ServerErrorResponse>)> {
    let service = NameService::new(&state.db);

    let names = match query.server_id {
        Some(server_id) => service.get_names_by_server(&server_id).await,
        None => service.get_all_names().await,
    }
    .map_err(|err| {
        tracing::error!("Failed to get names for export: {}", err);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ServerErrorResponse::new(
                "Failed to retrieve names for export".to_string(),
            )),
        )
    })?;

    // NOTE: If the same discord_id exists across multiple servers and no server_id
    // filter is applied, only the last entry per discord_id is kept. This matches
    // the bulk-import format which also uses flat discord_id: name mappings.
    let yaml_map: BTreeMap<u64, String> = names
        .into_iter()
        .map(|n| (n.discord_id(), n.name().to_string()))
        .collect();

    let yaml = serde_yaml::to_string(&yaml_map).map_err(|err| {
        tracing::error!("Failed to serialize names to YAML: {}", err);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ServerErrorResponse::new(
                "Failed to serialize names to YAML".to_string(),
            )),
        )
    })?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/x-yaml"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"names.yaml\"",
            ),
        ],
        yaml,
    ))
}

/// Creates and returns the names API router.
pub fn create_api_router(state: Arc<NameState>) -> Router {
    Router::new()
        .route("/names", get(get_names_handler).post(create_name_handler))
        .route("/names/export", get(export_names_handler))
        .route(
            "/names/{discord_id}/servers/{server_id}",
            put(update_name_by_discord_server_handler),
        )
        .with_state(state)
}
