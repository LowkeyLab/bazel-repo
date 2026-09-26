use crate::entities::name;
use crate::observations::{
    BulkOperation, BulkOutcome, Fact, FailureCategory, MutationKind, MutationOrigin,
    MutationOutcome, Observation, ObservationContext, Outcome, SharedObserver,
};
use sea_orm::{ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, QueryFilter};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

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
    #[must_use]
    pub fn new(id: u32, discord_id: u64, name: String, server_id: String) -> Self {
        Self {
            id,
            discord_id,
            name,
            server_id,
        }
    }

    /// Returns the Discord ID of the name.
    #[must_use]
    pub fn discord_id(&self) -> u64 {
        self.discord_id
    }

    /// Returns the name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the server ID of the name.
    #[must_use]
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    /// Returns the ID of the name.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.id
    }
}

/// Error type for `NameService` operations.
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
    /// Represents a name not found error by Discord ID and Server ID combination.
    #[error("Name entry with Discord ID {0} and Server ID '{1}' not found")]
    NameNotFoundByDiscordServer(u64, String),
    /// Represents malformed data error during bulk operations.
    #[error("Malformed data: {0}")]
    MalformedData(String),
}

pub struct NameService<'a> {
    db: &'a sea_orm::DatabaseConnection,
    observer: Option<SharedObserver>,
    context: Option<ObservationContext>,
}

// IDs retain their unsigned bit patterns in the signed database columns.
impl From<name::Model> for Name {
    fn from(model: name::Model) -> Self {
        Name::new(
            model.id.cast_unsigned(),
            model.discord_id.cast_unsigned(),
            model.name,
            model.server_id,
        )
    }
}

impl NameService<'_> {
    #[must_use]
    pub fn new(db: &sea_orm::DatabaseConnection) -> NameService<'_> {
        NameService {
            db,
            observer: None,
            context: None,
        }
    }

    pub fn with_observer(
        db: &sea_orm::DatabaseConnection,
        observer: SharedObserver,
    ) -> NameService<'_> {
        NameService {
            db,
            observer: Some(observer),
            context: None,
        }
    }

    /// Supplies request correlation for facts produced by this service.
    #[must_use]
    pub fn with_context(mut self, context: ObservationContext) -> Self {
        self.context = Some(context);
        self
    }

    fn context(&self, parent: Option<&ObservationContext>) -> ObservationContext {
        if let Some(parent) = parent {
            ObservationContext::child_of(parent)
        } else if let Some(context) = &self.context {
            ObservationContext::child_of(context)
        } else {
            ObservationContext::new()
        }
    }

    fn record(&self, mut context: ObservationContext, fact: Fact) {
        context.occurred_at = chrono::Utc::now();
        if let Some(observer) = &self.observer {
            observer.record(&Observation { context, fact });
        }
    }

    fn mutation(
        &self,
        context: ObservationContext,
        kind: MutationKind,
        origin: MutationOrigin,
        result: &Result<Name, NameServiceError>,
    ) {
        let (outcome, category) = match result {
            Ok(_) => (MutationOutcome::Committed, None),
            Err(NameServiceError::DuplicateEntryError(..)) => {
                (MutationOutcome::Rejected, Some(FailureCategory::Duplicate))
            }
            Err(
                NameServiceError::NameNotFound(..)
                | NameServiceError::NameNotFoundByDiscordServer(..),
            ) => (
                MutationOutcome::Rejected,
                Some(FailureCategory::MissingEntry),
            ),
            Err(_) => (MutationOutcome::Failed, Some(FailureCategory::Database)),
        };
        self.record(
            context,
            Fact::NameMutationFinished {
                kind,
                origin,
                outcome,
                category,
            },
        );
    }

    fn category(error: &NameServiceError) -> FailureCategory {
        match error {
            NameServiceError::DuplicateEntryError(..) => FailureCategory::Duplicate,
            NameServiceError::NameNotFound(..)
            | NameServiceError::NameNotFoundByDiscordServer(..) => FailureCategory::MissingEntry,
            NameServiceError::MalformedData(..) => FailureCategory::MalformedInput,
            NameServiceError::Database(..) => FailureCategory::Database,
        }
    }

    pub fn export_query_failed(&self) {
        self.record(
            self.context(None),
            Fact::ExportPrepared {
                entry_count: None,
                prepared_bytes: None,
                outcome: Outcome::Failed,
                stage: crate::observations::ExportStage::Query,
                category: Some(FailureCategory::Database),
            },
        );
    }

    /// Serializes names grouped by server as YAML.
    ///
    /// # Errors
    /// Returns the YAML serialization error if encoding fails.
    pub fn prepare_export(&self, names: &[Name]) -> Result<String, serde_yaml::Error> {
        let mut yaml_map: BTreeMap<String, BTreeMap<u64, String>> = BTreeMap::new();
        for name in names {
            yaml_map
                .entry(name.server_id().to_string())
                .or_default()
                .insert(name.discord_id(), name.name().to_string());
        }
        let result = serde_yaml::to_string(&yaml_map);
        self.record(
            self.context(None),
            Fact::ExportPrepared {
                entry_count: Some(names.len() as u64),
                prepared_bytes: result.as_ref().ok().map(|yaml| yaml.len() as u64),
                outcome: if result.is_ok() {
                    Outcome::Succeeded
                } else {
                    Outcome::Failed
                },
                stage: crate::observations::ExportStage::Serialization,
                category: result
                    .as_ref()
                    .err()
                    .map(|_| FailureCategory::Serialization),
            },
        );
        result
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
    ///
    /// # Errors
    /// Returns an error if the Discord/server pair already exists or the database fails.
    #[tracing::instrument(skip_all)]
    pub async fn create_name(
        &self,
        discord_id: u64,
        name: String,
        server_id: String,
    ) -> Result<Name, NameServiceError> {
        self.create_name_in(discord_id, name, server_id, None).await
    }

    async fn create_name_in(
        &self,
        discord_id: u64,
        name: String,
        server_id: String,
        parent: Option<&ObservationContext>,
    ) -> Result<Name, NameServiceError> {
        let context = self.context(parent);
        let started = Instant::now();
        let result = async {
            if self.entry_exists(discord_id, &server_id).await? {
                return Err(NameServiceError::DuplicateEntryError(discord_id, server_id));
            }
            let active_model = name::ActiveModel {
                discord_id: ActiveValue::Set(discord_id.cast_signed()),
                name: ActiveValue::Set(name),
                server_id: ActiveValue::Set(server_id),
                ..Default::default()
            };
            let created_model = active_model.insert(self.db).await?;
            Ok(Name::from(created_model))
        }
        .await;
        self.mutation(
            context.with_duration(started.elapsed()),
            MutationKind::Create,
            if parent.is_some() {
                MutationOrigin::Bulk
            } else {
                MutationOrigin::Standalone
            },
            &result,
        );
        result
    }

    /// Creates multiple name entries in the database from a YAML mapping.
    /// Skips entries that already exist (Discord ID + Server ID combination).
    ///
    /// # Arguments
    ///
    /// * `yaml_content` - The YAML content as a string containing `discord_id: name` mappings.
    /// * `server_id` - The server ID where the names are used.
    ///
    /// # Returns
    ///
    /// A `Result` containing a tuple with `(created_count, skipped_count, errors)` if successful, or an error otherwise.
    ///
    /// # Errors
    /// Returns an error for malformed YAML. Per-entry failures are returned in the errors list.
    #[tracing::instrument(skip_all)]
    pub async fn bulk_create_names(
        &self,
        yaml_content: &str,
        server_id: String,
    ) -> Result<(usize, usize, Vec<String>), NameServiceError> {
        let started = Instant::now();
        let context = self.context(None);
        let yaml_map: HashMap<u64, String> = match serde_yaml::from_str(yaml_content) {
            Ok(map) => map,
            Err(error) => {
                self.record(
                    context.with_duration(started.elapsed()),
                    Fact::BulkOperationFinished {
                        operation: BulkOperation::Import,
                        attempted: 0,
                        succeeded: 0,
                        skipped: 0,
                        failed: 0,
                        input_count: None,
                        outcome: BulkOutcome::Rejected,
                        categories: vec![(FailureCategory::MalformedInput, 1)],
                    },
                );
                return Err(NameServiceError::MalformedData(format!(
                    "Invalid YAML format: {error}"
                )));
            }
        };
        let input_count = yaml_map.len() as u64;
        let mut created_count = 0;
        let mut skipped_count = 0;
        let mut errors = Vec::new();
        let mut categories = Vec::new();
        for (discord_id, name) in yaml_map {
            match self
                .create_name_in(discord_id, name, server_id.clone(), Some(&context))
                .await
            {
                Ok(_) => created_count += 1,
                Err(NameServiceError::DuplicateEntryError(..)) => {
                    skipped_count += 1;
                    add_category(&mut categories, FailureCategory::Duplicate);
                }
                Err(error) => {
                    add_category(&mut categories, Self::category(&error));
                    errors.push(format!(
                        "Failed to create entry for Discord ID {discord_id}: {error}"
                    ));
                }
            }
        }
        let failed = errors.len() as u64;
        self.record(
            context.with_duration(started.elapsed()),
            Fact::BulkOperationFinished {
                operation: BulkOperation::Import,
                attempted: input_count,
                succeeded: created_count as u64,
                skipped: skipped_count as u64,
                failed,
                input_count: Some(input_count),
                outcome: bulk_outcome(created_count as u64, failed),
                categories,
            },
        );
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
    ///
    /// # Errors
    /// Returns an error if the entry does not exist or the database fails.
    #[tracing::instrument(skip_all)]
    pub async fn edit_name_by_id(
        &self,
        id: u32,
        new_name: String,
        new_server_id: String,
    ) -> Result<Name, NameServiceError> {
        let context = self.context(None);
        let started = Instant::now();
        let result = async {
            let name_to_update = name::Entity::find_by_id(id.cast_signed())
                .one(self.db)
                .await?
                .ok_or(NameServiceError::NameNotFound(id))?;

            let mut active_model: name::ActiveModel = name_to_update.into();
            active_model.name = ActiveValue::Set(new_name.clone());
            active_model.server_id = ActiveValue::Set(new_server_id.clone());
            let updated_model = active_model.update(self.db).await?;

            Ok(Name::from(updated_model))
        }
        .await;
        self.mutation(
            context.with_duration(started.elapsed()),
            MutationKind::Update,
            MutationOrigin::Standalone,
            &result,
        );
        result
    }

    /// Retrieves all name entries from the database.
    ///
    /// # Returns
    ///
    /// A `Result` containing a vector of `Name` if successful, or an error otherwise.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    #[tracing::instrument(skip_all)]
    pub async fn get_all_names(&self) -> Result<Vec<Name>, NameServiceError> {
        let started = Instant::now();
        let context = self.context(None);
        let result: Result<_, NameServiceError> = async {
            let names: Vec<Name> = name::Entity::find()
                .all(self.db)
                .await?
                .into_iter()
                .map(Name::from)
                .collect();
            Ok(names)
        }
        .await;
        let (outcome, category, result_count) = match &result {
            Ok(names) => (Outcome::Succeeded, None, Some(names.len() as u64)),
            Err(error) => (Outcome::Failed, Some(Self::category(error)), None),
        };
        self.record(
            context.with_duration(started.elapsed()),
            Fact::NamesReadFinished {
                filter_present: false,
                result_count,
                outcome,
                category,
            },
        );
        result
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
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    #[tracing::instrument(skip_all)]
    pub async fn get_names_by_server(
        &self,
        server_id: &str,
    ) -> Result<Vec<Name>, NameServiceError> {
        let started = Instant::now();
        let context = self.context(None);
        let result: Result<_, NameServiceError> = async {
            let names: Vec<Name> = name::Entity::find()
                .filter(name::Column::ServerId.eq(server_id))
                .all(self.db)
                .await?
                .into_iter()
                .map(Name::from)
                .collect();
            Ok(names)
        }
        .await;
        let (outcome, category, result_count) = match &result {
            Ok(names) => (Outcome::Succeeded, None, Some(names.len() as u64)),
            Err(error) => (Outcome::Failed, Some(Self::category(error)), None),
        };
        self.record(
            context.with_duration(started.elapsed()),
            Fact::NamesReadFinished {
                filter_present: true,
                result_count,
                outcome,
                category,
            },
        );
        result
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
    ///
    /// # Errors
    /// Returns an error if the entry does not exist or the database fails.
    #[tracing::instrument(skip_all)]
    pub async fn delete_name_by_id(&self, id: u32) -> Result<Name, NameServiceError> {
        self.delete_name_in(id, None).await.map(|(name, _)| name)
    }

    async fn delete_name_in(
        &self,
        id: u32,
        parent: Option<&ObservationContext>,
    ) -> Result<(Name, bool), NameServiceError> {
        let context = self.context(parent);
        let started = Instant::now();
        let result = async {
            let name_to_delete = name::Entity::find_by_id(id.cast_signed())
                .one(self.db)
                .await?
                .ok_or(NameServiceError::NameNotFound(id))?;

            let name_copy = Name::from(name_to_delete.clone());
            let deleted = name::Entity::delete_by_id(id.cast_signed())
                .exec(self.db)
                .await?;
            Ok((name_copy, deleted.rows_affected > 0))
        }
        .await;
        let origin = if parent.is_some() {
            MutationOrigin::Bulk
        } else {
            MutationOrigin::Standalone
        };
        let (outcome, category) = match &result {
            Ok((_, true)) => (MutationOutcome::Committed, None),
            Ok((_, false)) => (
                MutationOutcome::Rejected,
                Some(FailureCategory::NoRowsAffected),
            ),
            Err(NameServiceError::NameNotFound(..)) => (
                MutationOutcome::Rejected,
                Some(FailureCategory::MissingEntry),
            ),
            Err(_) => (MutationOutcome::Failed, Some(FailureCategory::Database)),
        };
        self.record(
            context.with_duration(started.elapsed()),
            Fact::NameMutationFinished {
                kind: MutationKind::Delete,
                origin,
                outcome,
                category,
            },
        );
        result
    }

    /// Deletes multiple name entries by their IDs.
    ///
    /// # Arguments
    ///
    /// * `ids` - A slice of IDs of the name entries to delete.
    ///
    /// # Returns
    ///
    /// A `Result` containing a tuple with `(deleted_count, failed_deletes)` if successful, or an error otherwise.
    ///
    /// # Errors
    /// Currently returns per-entry failures in the result rather than returning an error.
    #[tracing::instrument(skip_all)]
    pub async fn bulk_delete_names(
        &self,
        ids: &[u32],
    ) -> Result<(usize, Vec<String>), NameServiceError> {
        let started = Instant::now();
        let context = self.context(None);
        let mut deleted_count = 0;
        let mut observed_succeeded = 0;
        let mut observed_failed = 0;
        let mut failed_deletes = Vec::new();
        let mut categories = Vec::new();
        for &id in ids {
            match self.delete_name_in(id, Some(&context)).await {
                Ok((_, true)) => {
                    deleted_count += 1;
                    observed_succeeded += 1;
                }
                Ok((_, false)) => {
                    deleted_count += 1;
                    observed_failed += 1;
                    add_category(&mut categories, FailureCategory::NoRowsAffected);
                }
                Err(NameServiceError::NameNotFound(_)) => {
                    failed_deletes.push(format!("Name with ID {id} not found"));
                    observed_failed += 1;
                    add_category(&mut categories, FailureCategory::MissingEntry);
                }
                Err(error) => {
                    failed_deletes.push(format!("Failed to delete name with ID {id}: {error}"));
                    observed_failed += 1;
                    add_category(&mut categories, Self::category(&error));
                }
            }
        }
        self.record(
            context.with_duration(started.elapsed()),
            Fact::BulkOperationFinished {
                operation: BulkOperation::Delete,
                attempted: ids.len() as u64,
                succeeded: observed_succeeded,
                skipped: 0,
                failed: observed_failed,
                input_count: Some(ids.len() as u64),
                outcome: bulk_outcome(observed_succeeded, observed_failed),
                categories,
            },
        );
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
    #[tracing::instrument(skip_all)]
    async fn entry_exists(
        &self,
        discord_id: u64,
        server_id: &str,
    ) -> Result<bool, NameServiceError> {
        let existing_name = name::Entity::find()
            .filter(name::Column::DiscordId.eq(discord_id.cast_signed()))
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
    ///
    /// # Errors
    /// Returns an error if the entry does not exist or the database fails.
    #[tracing::instrument(skip_all)]
    pub async fn get_name_by_id(&self, id: u32) -> Result<Name, NameServiceError> {
        let started = Instant::now();
        let context = self.context(None);
        let result: Result<_, NameServiceError> = async {
            let name_model = name::Entity::find_by_id(id.cast_signed())
                .one(self.db)
                .await?
                .ok_or(NameServiceError::NameNotFound(id))?;
            Ok(Name::from(name_model))
        }
        .await;
        let (outcome, category, result_count) = match &result {
            Ok(_) => (Outcome::Succeeded, None, Some(1)),
            Err(error) => (Outcome::Failed, Some(Self::category(error)), None),
        };
        self.record(
            context.with_duration(started.elapsed()),
            Fact::NamesReadFinished {
                filter_present: false,
                result_count,
                outcome,
                category,
            },
        );
        result
    }

    /// Updates a name entry by Discord ID and Server ID combination.
    ///
    /// # Arguments
    ///
    /// * `discord_id` - The Discord ID of the user.
    /// * `server_id` - The Server ID where the name is used.
    /// * `new_name` - The new name to set.
    ///
    /// # Returns
    ///
    /// A `Result` containing the updated `Name` if successful, or an error otherwise.
    ///
    /// # Errors
    /// Returns an error if the Discord/server pair does not exist or the database fails.
    #[tracing::instrument(skip_all)]
    pub async fn update_name_by_discord_server(
        &self,
        discord_id: u64,
        server_id: &str,
        new_name: String,
    ) -> Result<Name, NameServiceError> {
        let context = self.context(None);
        let started = Instant::now();
        let result = async {
            let name_to_update = name::Entity::find()
                .filter(name::Column::DiscordId.eq(discord_id.cast_signed()))
                .filter(name::Column::ServerId.eq(server_id))
                .one(self.db)
                .await?
                .ok_or(NameServiceError::NameNotFoundByDiscordServer(
                    discord_id,
                    server_id.to_string(),
                ))?;

            let mut active_model: name::ActiveModel = name_to_update.into();
            active_model.name = ActiveValue::Set(new_name.clone());
            let updated_model = active_model.update(self.db).await?;

            Ok(Name::from(updated_model))
        }
        .await;
        self.mutation(
            context.with_duration(started.elapsed()),
            MutationKind::Update,
            MutationOrigin::Standalone,
            &result,
        );
        result
    }
}

fn bulk_outcome(succeeded: u64, failed: u64) -> BulkOutcome {
    if failed == 0 {
        BulkOutcome::Succeeded
    } else if succeeded > 0 {
        BulkOutcome::Partial
    } else {
        BulkOutcome::Failed
    }
}

fn add_category(categories: &mut Vec<(FailureCategory, u64)>, category: FailureCategory) {
    if let Some((_, count)) = categories
        .iter_mut()
        .find(|(existing, _)| *existing == category)
    {
        *count += 1;
    } else {
        categories.push((category, 1));
    }
}
