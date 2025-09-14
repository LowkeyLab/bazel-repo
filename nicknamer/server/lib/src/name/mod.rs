use crate::entities::*;
use sea_orm::*;
use std::collections::HashMap;

pub mod api;
pub mod web;

#[derive(Debug, PartialEq, Clone, Eq, Hash)]
pub struct Name {
    id: u32,
    discord_id: u64,
    name: String,
    server_id: String,
}

impl Name {
    pub fn new(id: u32, discord_id: u64, name: String, server_id: String) -> Self {
        Self {
            id,
            discord_id,
            name,
            server_id,
        }
    }

    /// Returns the Discord ID of the name.
    pub fn discord_id(&self) -> u64 {
        self.discord_id
    }

    /// Returns the name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the server ID of the name.
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    /// Returns the ID of the name.
    pub fn id(&self) -> u32 {
        self.id
    }
}

/// Error type for NameService operations.
#[derive(Debug, thiserror::Error)]
pub enum NameServiceError {
    /// Represents a duplicate entry error (Discord ID + Server ID combination already exists).
    #[error("Entry with Discord ID {0} and Server ID '{1}' already exists")]
    DuplicateEntryError(u64, String),
    /// Represents a database error.
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    /// Represents a name not found error.
    #[error("Name entry with ID {0} not found")]
    NameNotFound(u32),
    /// Represents malformed data error during bulk operations.
    #[error("Malformed data: {0}")]
    MalformedData(String),
}

pub struct NameService<'a> {
    db: &'a sea_orm::DatabaseConnection,
}

impl From<name::Model> for Name {
    fn from(model: name::Model) -> Self {
        Name::new(
            model.id as u32,
            model.discord_id as u64,
            model.name,
            model.server_id,
        )
    }
}

impl NameService<'_> {
    pub fn new(db: &sea_orm::DatabaseConnection) -> NameService {
        NameService { db }
    }

    /// Creates a new name entry in the database.
    /// # Arguments
    ///
    /// * `discord_id` - The Discord ID of the user.
    /// * `name` - The name of the user.
    /// * `server_id` - The server ID where the name is used.
    ///
    /// # Returns
    ///
    /// A `Result` containing the created `Name` if successful, or an error otherwise.
    #[tracing::instrument(skip(self))]
    pub async fn create_name(
        &self,
        discord_id: u64,
        name: String,
        server_id: String,
    ) -> Result<Name, NameServiceError> {
        // Check if Discord ID + Server ID combination already exists
        if self.entry_exists(discord_id, &server_id).await? {
            return Err(NameServiceError::DuplicateEntryError(discord_id, server_id));
        }

        let active_model = name::ActiveModel {
            discord_id: ActiveValue::Set(discord_id as i64),
            name: ActiveValue::Set(name.clone()),
            server_id: ActiveValue::Set(server_id.clone()),
            ..Default::default()
        };
        let created_model = active_model.insert(self.db).await?;
        Ok(Name::from(created_model))
    }

    /// Creates multiple name entries in the database from a YAML mapping.
    /// Skips entries that already exist (Discord ID + Server ID combination).
    ///
    /// # Arguments
    ///
    /// * `yaml_content` - The YAML content as a string containing discord_id: name mappings.
    /// * `server_id` - The server ID where the names are used.
    ///
    /// # Returns
    ///
    /// A `Result` containing a tuple with (created_count, skipped_count, errors) if successful, or an error otherwise.
    #[tracing::instrument(skip(self, yaml_content))]
    pub async fn bulk_create_names(
        &self,
        yaml_content: &str,
        server_id: String,
    ) -> Result<(usize, usize, Vec<String>), NameServiceError> {
        // Parse YAML content
        let yaml_map: HashMap<u64, String> = serde_yaml::from_str(yaml_content)
            .map_err(|e| NameServiceError::MalformedData(format!("Invalid YAML format: {}", e)))?;

        let mut created_count = 0;
        let mut skipped_count = 0;
        let mut errors = Vec::new();

        for (discord_id, name) in yaml_map {
            match self
                .create_name(discord_id, name.clone(), server_id.clone())
                .await
            {
                Ok(_) => created_count += 1,
                Err(NameServiceError::DuplicateEntryError(_, _)) => {
                    skipped_count += 1;
                    tracing::info!(
                        "Skipped existing entry for Discord ID {} in server {}",
                        discord_id,
                        server_id
                    );
                }
                Err(e) => {
                    errors.push(format!(
                        "Failed to create entry for Discord ID {}: {}",
                        discord_id, e
                    ));
                    tracing::error!(
                        "Failed to create entry for Discord ID {}: {}",
                        discord_id,
                        e
                    );
                }
            }
        }

        Ok((created_count, skipped_count, errors))
    }

    /// Edits a name entry by their ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the name entry to edit.
    /// * `new_name` - The new name for the entry.
    /// * `new_server_id` - The new server ID for the entry.
    ///
    /// # Returns
    ///
    /// A `Result` containing the updated `Name` if successful, or an error otherwise.
    #[tracing::instrument(skip(self))]
    pub async fn edit_name_by_id(
        &self,
        id: u32,
        new_name: String,
        new_server_id: String,
    ) -> Result<Name, NameServiceError> {
        let name_to_update = name::Entity::find_by_id(id as i32)
            .one(self.db)
            .await?
            .ok_or(NameServiceError::NameNotFound(id))?;

        let mut active_model: name::ActiveModel = name_to_update.into();
        active_model.name = ActiveValue::Set(new_name.clone());
        active_model.server_id = ActiveValue::Set(new_server_id.clone());
        let updated_model = active_model.update(self.db).await?;

        Ok(Name::from(updated_model))
    }

    /// Retrieves all name entries from the database.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Name` if successful, or an error otherwise.
    #[tracing::instrument(skip(self))]
    pub async fn get_all_names(&self) -> Result<Vec<Name>, NameServiceError> {
        let names = name::Entity::find()
            .all(self.db)
            .await?
            .into_iter()
            .map(Name::from)
            .collect();
        Ok(names)
    }

    /// Retrieves name entries from the database filtered by server ID.
    ///
    /// # Arguments
    ///
    /// * `server_id` - The server ID to filter by.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Name` if successful, or an error otherwise.
    #[tracing::instrument(skip(self))]
    pub async fn get_names_by_server(
        &self,
        server_id: &str,
    ) -> Result<Vec<Name>, NameServiceError> {
        let names = name::Entity::find()
            .filter(name::Column::ServerId.eq(server_id))
            .all(self.db)
            .await?
            .into_iter()
            .map(Name::from)
            .collect();
        Ok(names)
    }

    /// Deletes a name entry by their ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the name entry to delete.
    ///
    /// # Returns
    ///
    /// A `Result` containing the deleted `Name` if successful, or an error otherwise.
    #[tracing::instrument(skip(self))]
    pub async fn delete_name_by_id(&self, id: u32) -> Result<Name, NameServiceError> {
        let name_to_delete = name::Entity::find_by_id(id as i32)
            .one(self.db)
            .await?
            .ok_or(NameServiceError::NameNotFound(id))?;

        let name_copy = Name::from(name_to_delete.clone());
        name::Entity::delete_by_id(id as i32).exec(self.db).await?;
        Ok(name_copy)
    }

    /// Deletes multiple name entries by their IDs.
    ///
    /// # Arguments
    ///
    /// * `ids` - A slice of IDs of the name entries to delete.
    ///
    /// # Returns
    ///
    /// A `Result` containing a tuple with (deleted_count, failed_deletes) if successful, or an error otherwise.
    #[tracing::instrument(skip(self))]
    pub async fn bulk_delete_names(
        &self,
        ids: &[u32],
    ) -> Result<(usize, Vec<String>), NameServiceError> {
        let mut deleted_count = 0;
        let mut failed_deletes = Vec::new();

        for &id in ids {
            match self.delete_name_by_id(id).await {
                Ok(_) => deleted_count += 1,
                Err(NameServiceError::NameNotFound(_)) => {
                    failed_deletes.push(format!("Name with ID {} not found", id));
                    tracing::warn!("Failed to delete name with ID {}: not found", id);
                }
                Err(e) => {
                    failed_deletes.push(format!("Failed to delete name with ID {}: {}", id, e));
                    tracing::error!("Failed to delete name with ID {}: {}", id, e);
                }
            }
        }

        Ok((deleted_count, failed_deletes))
    }

    /// Checks if a name entry with the given Discord ID and Server ID combination already exists.
    ///
    /// # Arguments
    ///
    /// * `discord_id` - The Discord ID to check for.
    /// * `server_id` - The Server ID to check for.
    ///
    /// # Returns
    ///
    /// A `Result` containing `true` if the combination exists, `false` otherwise, or an error.
    #[tracing::instrument(skip(self))]
    async fn entry_exists(
        &self,
        discord_id: u64,
        server_id: &str,
    ) -> Result<bool, NameServiceError> {
        let existing_name = name::Entity::find()
            .filter(name::Column::DiscordId.eq(discord_id as i64))
            .filter(name::Column::ServerId.eq(server_id))
            .one(self.db)
            .await?;
        Ok(existing_name.is_some())
    }

    /// Retrieves a name entry by its ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the name entry to retrieve.
    ///
    /// # Returns
    ///
    /// A `Result` containing the `Name` if successful, or an error otherwise.
    #[tracing::instrument(skip(self))]
    pub async fn get_name_by_id(&self, id: u32) -> Result<Name, NameServiceError> {
        let name_model = name::Entity::find_by_id(id as i32)
            .one(self.db)
            .await?
            .ok_or(NameServiceError::NameNotFound(id))?;
        Ok(Name::from(name_model))
    }
}
