use askama::Template;
use axum::{
    Form, Router,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::Html,
    routing::get,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::name::{Name, NameService, NameServiceError};

#[derive(Debug, Deserialize)]
pub struct CreateNameForm {
    discord_id: u64,
    name: String,
    server_id: String,
}

#[derive(Debug, Deserialize)]
pub struct EditNameForm {
    name: String,
    server_id: String,
}

#[derive(Debug, Deserialize)]
pub struct BulkAddForm {
    server_id: String,
    yaml_content: String,
}

/// Helper function to get all names, sort them using the provided function, and render them as a names table.
/// This reduces code duplication across handlers that need to display sorted names.
#[tracing::instrument(skip(name_service, sort_fn))]
async fn render_names_table<F>(
    name_service: &NameService<'_>,
    sort_fn: F,
) -> Result<String, NameError>
where
    F: FnOnce(&mut Vec<Name>),
{
    let mut names = name_service.get_all_names().await?;
    sort_fn(&mut names);
    let table_template = NamesTableTemplate::new(names);
    table_template.render().map_err(NameError::from)
}

/// Custom error type for name handler operations.
#[derive(Debug, thiserror::Error)]
enum NameError {
    /// Represents an error during template rendering.
    #[error("Template rendering failed")]
    Template(#[from] askama::Error),
    /// Represents a database error.
    #[error("Database error")]
    Database(#[from] anyhow::Error),
    /// Represents a name service error.
    #[error("Name service error")]
    Service(#[from] NameServiceError),
    /// Represents a duplicate entry error (Discord ID + Server ID combination already exists).
    #[error("A name entry already exists for this Discord ID and Server ID combination")]
    DuplicateEntry,
    /// Represents an I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl axum::response::IntoResponse for NameError {
    fn into_response(self) -> axum::response::Response {
        let (status_code, user_facing_error_message) = match self {
            NameError::DuplicateEntry => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "A name entry already exists for this Discord ID and Server ID combination. Please use a different combination.",
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An unexpected error occurred while processing your request. Please try again later.",
            ),
        };

        let error_template = ErrorMessageTemplate::new(user_facing_error_message.to_string());
        let Ok(rendered) = error_template.render() else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };

        let mut response = (status_code, Html(rendered)).into_response();
        // Add HTMX headers to retarget the error message to the error div
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("hx-reswap"),
            HeaderValue::from_static("innerHTML"),
        );
        response.headers_mut().extend(headers);
        response
    }
}

#[derive(Template)]
#[template(path = "names.html")]
struct NamesTemplate {}

impl NamesTemplate {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Template)]
#[template(path = "names/add_name_form.html")]
struct AddNameFormTemplate;

#[derive(Template)]
#[template(path = "names/names_table.html")]
struct NamesTableTemplate {
    names: Vec<Name>,
}

impl NamesTableTemplate {
    pub fn new(names: Vec<Name>) -> Self {
        Self { names }
    }
}

#[derive(Template)]
#[template(path = "names/error_message.html")]
struct ErrorMessageTemplate {
    message: String,
}

impl ErrorMessageTemplate {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

#[derive(Template)]
#[template(path = "names/edit_name_form.html")]
struct EditNameFormTemplate {
    name: Name,
}

impl EditNameFormTemplate {
    pub fn new(name: Name) -> Self {
        Self { name }
    }
}

#[derive(Template)]
#[template(path = "names/name_row.html")]
struct NameRowTemplate {
    name: Name,
}

impl NameRowTemplate {
    pub fn new(name: Name) -> Self {
        Self { name }
    }
}

#[derive(Template)]
#[template(path = "names/bulk_add_form.html")]
struct BulkAddFormTemplate;

#[derive(Template)]
#[template(path = "names/bulk_add_success.html")]
struct BulkAddSuccessTemplate {
    created_count: usize,
    skipped_count: usize,
    errors: Vec<String>,
    server_id: String,
    yaml_content: String,
}

impl BulkAddSuccessTemplate {
    pub fn new(
        created_count: usize,
        skipped_count: usize,
        errors: Vec<String>,
        server_id: String,
        yaml_content: String,
    ) -> Self {
        Self {
            created_count,
            skipped_count,
            errors,
            server_id,
            yaml_content,
        }
    }
}

#[derive(Template)]
#[template(path = "names/bulk_delete.html")]
struct BulkDeleteTemplate;

#[derive(Template)]
#[template(path = "names/bulk_delete_table.html")]
struct BulkDeleteTableTemplate {
    names: Vec<Name>,
}

impl BulkDeleteTableTemplate {
    pub fn new(names: Vec<Name>) -> Self {
        Self { names }
    }
}

#[derive(Clone, Debug)]
pub struct NameState {
    pub db: Arc<sea_orm::DatabaseConnection>,
}

/// Handler for the /names endpoint that displays all names in a table.
#[tracing::instrument]
async fn names_handler() -> Result<Html<String>, NameError> {
    let template = NamesTemplate::new();
    template.render().map(Html).map_err(NameError::from)
}

/// Handler for creating a new name via POST request.
#[tracing::instrument(skip(state))]
async fn create_name_handler(
    State(state): State<Arc<NameState>>,
    Form(form): Form<CreateNameForm>,
) -> Result<Html<String>, NameError> {
    let name_service = NameService::new(&state.db);

    match name_service
        .create_name(form.discord_id, form.name, form.server_id)
        .await
    {
        Ok(_) => {
            // Get updated names for the table and render
            let table_html = render_names_table(&name_service, |names| {
                names.sort_by_key(|name| name.id());
            })
            .await?;
            Ok(Html(table_html))
        }
        Err(NameServiceError::DuplicateEntryError(_, _)) => Err(NameError::DuplicateEntry),
        Err(err) => Err(NameError::Service(err)),
    }
}

/// Handler for serving the add name form.
#[tracing::instrument]
async fn add_name_form_handler() -> Result<Html<String>, NameError> {
    let template = AddNameFormTemplate;
    template.render().map(Html).map_err(NameError::from)
}

/// Handler for deleting a name via POST request.
#[tracing::instrument(skip(state))]
async fn delete_name_handler(
    State(state): State<Arc<NameState>>,
    axum::extract::Path(id): axum::extract::Path<u32>,
) -> Result<Html<String>, NameError> {
    let name_service = NameService::new(&state.db);

    match name_service.delete_name_by_id(id).await {
        Ok(_) => {
            // Get updated names for the table and render
            let table_html = render_names_table(&name_service, |names| {
                names.sort_by_key(|name| name.id());
            })
            .await?;
            Ok(Html(table_html))
        }
        Err(err) => Err(NameError::Service(err)),
    }
}

/// Handler for bulk deleting names via DELETE request.
#[tracing::instrument(skip(state))]
async fn bulk_delete_names_handler(
    State(state): State<Arc<NameState>>,
    RawQuery(query): RawQuery,
) -> Result<Html<String>, NameError> {
    let name_service = NameService::new(&state.db);

    // Parse query parameters manually to handle multiple values with the same key
    let selected_ids: Vec<u32> = if let Some(query_str) = query {
        query_str
            .split('&')
            .filter_map(|pair| {
                if pair.starts_with("selected_ids=") {
                    pair.strip_prefix("selected_ids=")
                        .and_then(|id_str| id_str.parse().ok())
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    if selected_ids.is_empty() {
        // No names selected for deletion, just return the current table
        let table_html = render_names_table(&name_service, |names| {
            names.sort_by_key(|name| name.id());
        })
        .await?;
        return Ok(Html(table_html));
    }

    match name_service.bulk_delete_names(&selected_ids).await {
        Ok(_) => {
            // Get updated names for the table and render
            let table_html = render_names_table(&name_service, |names| {
                names.sort_by_key(|name| name.id());
            })
            .await?;
            Ok(Html(table_html))
        }
        Err(err) => Err(NameError::Service(err)),
    }
}

/// Handler for serving the edit name form.
#[tracing::instrument(skip(state))]
async fn edit_name_handler(
    State(state): State<Arc<NameState>>,
    axum::extract::Path(id): axum::extract::Path<u32>,
) -> Result<Html<String>, NameError> {
    let name_service = NameService::new(&state.db);

    match name_service.get_name_by_id(id).await {
        Ok(name) => {
            let template = EditNameFormTemplate::new(name);
            template.render().map(Html).map_err(NameError::from)
        }
        Err(err) => Err(NameError::Service(err)),
    }
}

/// Handler for updating a name via PUT request.
#[tracing::instrument(skip(state))]
async fn update_name_handler(
    State(state): State<Arc<NameState>>,
    axum::extract::Path(id): axum::extract::Path<u32>,
    Form(form): Form<EditNameForm>,
) -> Result<Html<String>, NameError> {
    let name_service = NameService::new(&state.db);

    match name_service
        .edit_name_by_id(id, form.name, form.server_id)
        .await
    {
        Ok(_) => {
            // Get the updated name to render just this row
            let updated_name = name_service.get_name_by_id(id).await?;

            // Render only the updated name row
            let row_template = NameRowTemplate::new(updated_name);
            let row_html = row_template.render().map_err(NameError::from)?;

            Ok(Html(row_html))
        }
        Err(err) => Err(NameError::Service(err)),
    }
}

/// Handler for GET /names/table that returns just the names table fragment.
#[tracing::instrument(skip(state))]
async fn names_table_handler(
    State(state): State<Arc<NameState>>,
) -> Result<Html<String>, NameError> {
    let name_service = NameService::new(&state.db);
    let table_html = render_names_table(&name_service, |names| {
        names.sort_by_key(|name| name.id());
    })
    .await?;
    Ok(Html(table_html))
}

/// Handler for GET /names/{id} that returns a single name row.
#[tracing::instrument(skip(state))]
async fn get_name_row_handler(
    State(state): State<Arc<NameState>>,
    axum::extract::Path(id): axum::extract::Path<u32>,
) -> Result<Html<String>, NameError> {
    let name_service = NameService::new(&state.db);

    match name_service.get_name_by_id(id).await {
        Ok(name) => {
            let template = NameRowTemplate::new(name);
            template.render().map(Html).map_err(NameError::from)
        }
        Err(err) => Err(NameError::Service(err)),
    }
}

/// Handler for GET /names/bulk-add that displays the bulk add form.
#[tracing::instrument]
async fn bulk_add_form_handler() -> Result<Html<String>, NameError> {
    let template = BulkAddFormTemplate;
    template.render().map(Html).map_err(NameError::from)
}

/// Handler for POST /names/bulk-add that processes the YAML content.
#[tracing::instrument(skip(state, form))]
async fn bulk_add_handler(
    State(state): State<Arc<NameState>>,
    Form(form): Form<BulkAddForm>,
) -> Result<Html<String>, NameError> {
    let name_service = NameService::new(&state.db);

    // Process the bulk upload using the pasted YAML content
    match name_service
        .bulk_create_names(&form.yaml_content, form.server_id.clone())
        .await
    {
        Ok((created_count, skipped_count, errors)) => {
            let template = BulkAddSuccessTemplate::new(
                created_count,
                skipped_count,
                errors,
                form.server_id,
                form.yaml_content,
            );
            template.render().map(Html).map_err(NameError::from)
        }
        Err(err) => Err(NameError::Service(err)),
    }
}

/// Handler for GET /names/delete that displays the bulk delete interface.
#[tracing::instrument]
async fn bulk_delete_page_handler() -> Result<Html<String>, NameError> {
    let template = BulkDeleteTemplate;
    template.render().map(Html).map_err(NameError::from)
}

/// Handler for GET /names/delete/table that returns the bulk delete table fragment.
#[tracing::instrument(skip(state))]
async fn bulk_delete_table_handler(
    State(state): State<Arc<NameState>>,
) -> Result<Html<String>, NameError> {
    let name_service = NameService::new(&state.db);
    let mut names = name_service.get_all_names().await?;
    names.sort_by_key(|name| name.id());
    let template = BulkDeleteTableTemplate::new(names);
    template.render().map(Html).map_err(NameError::from)
}

/// Handler for DELETE /names/delete that processes bulk deletion and returns the updated bulk delete table.
#[tracing::instrument(skip(state))]
async fn bulk_delete_names_delete_handler(
    State(state): State<Arc<NameState>>,
    RawQuery(query): RawQuery,
) -> Result<Html<String>, NameError> {
    let name_service = NameService::new(&state.db);

    // Parse query parameters manually to handle multiple values with the same key
    let selected_ids: Vec<u32> = if let Some(query_str) = query {
        query_str
            .split('&')
            .filter_map(|pair| {
                if pair.starts_with("selected_ids=") {
                    pair.strip_prefix("selected_ids=")
                        .and_then(|id_str| id_str.parse().ok())
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // Perform the deletion if any IDs are selected
    if !selected_ids.is_empty() {
        let _ = name_service.bulk_delete_names(&selected_ids).await;
    }

    // Return the updated bulk delete table (same as GET /names/delete/table)
    let mut names = name_service.get_all_names().await?;
    names.sort_by_key(|name| name.id());
    let template = BulkDeleteTableTemplate::new(names);
    template.render().map(Html).map_err(NameError::from)
}

/// Creates and returns the name router with all name-related routes.
pub fn create_name_router(state: Arc<NameState>) -> Router {
    Router::new()
        .route(
            "/names",
            get(names_handler)
                .post(create_name_handler)
                .delete(bulk_delete_names_handler),
        )
        .route("/names/add", get(add_name_form_handler))
        .route(
            "/names/bulk-add",
            get(bulk_add_form_handler).post(bulk_add_handler),
        )
        .route(
            "/names/delete",
            get(bulk_delete_page_handler).delete(bulk_delete_names_delete_handler),
        )
        .route("/names/delete/table", get(bulk_delete_table_handler))
        .route(
            "/names/{id}",
            get(get_name_row_handler)
                .delete(delete_name_handler)
                .put(update_name_handler),
        )
        .route("/names/{id}/edit", get(edit_name_handler))
        .route("/names/table", get(names_table_handler))
        .with_state(state.clone())
}
