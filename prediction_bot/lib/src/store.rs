//! Commands append individual events atomically; queries expose immutable replayed views.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use sqlx::{
    PgConnection, PgPool, Row, SqlSafeStr,
    migrate::{Migration, MigrationType, Migrator},
    postgres::PgPoolOptions,
    types::Json,
};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::audit::{
    AuditEvent, CommandKind, Outcome, SharedAudit, Stage, canonical_command_key, logging_listener,
    store_outcome,
};
use crate::domain::{self, Actor, Command, DomainError, Event, GrantReason, Policy, State};
use crate::events::{CloudEvent, Context, EventError};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Metadata(#[from] EventError),
    #[error("event history is invalid: {0}")]
    History(&'static str),
    #[error("invalid configuration: {0}")]
    Configuration(&'static str),
}

#[derive(Clone, Debug)]
pub struct View {
    pub revision: i64,
    pub state: State,
}

pub struct Store {
    pub pool: PgPool,
    application: u64,
    defaults: Policy,
    views: Mutex<HashMap<u64, Arc<View>>>,
    audit: SharedAudit,
}

struct TransactionSuccess {
    response: String,
    stage: Stage,
}

struct TransactionFailure {
    stage: Stage,
    error: StoreError,
}

fn at_stage<T, E: Into<StoreError>>(
    stage: Stage,
    result: Result<T, E>,
) -> Result<T, TransactionFailure> {
    result.map_err(|error| TransactionFailure {
        stage,
        error: error.into(),
    })
}

impl Store {
    #[must_use]
    pub fn new(pool: PgPool, application: u64, defaults: Policy) -> Self {
        Self::new_with_audit(pool, application, defaults, logging_listener())
    }

    #[must_use]
    pub fn new_with_audit(
        pool: PgPool,
        application: u64,
        defaults: Policy,
        audit: SharedAudit,
    ) -> Self {
        Self {
            pool,
            application,
            defaults,
            views: Mutex::new(HashMap::new()),
            audit,
        }
    }

    /// Connect with a restricted runtime role and reconstruct committed guild histories.
    ///
    /// # Errors
    /// Returns an error for invalid configuration, schema or permissions, database failures,
    /// or invalid stored history.
    pub async fn connect(
        url: &str,
        application: u64,
        defaults: Policy,
    ) -> Result<Self, StoreError> {
        Self::connect_with_audit(url, application, defaults, logging_listener()).await
    }

    /// Connect with an injected operational audit listener.
    ///
    /// # Errors
    /// Returns the same configuration, schema, permission, database, and history errors as
    /// [`Self::connect`].
    pub async fn connect_with_audit(
        url: &str,
        application: u64,
        defaults: Policy,
        audit: SharedAudit,
    ) -> Result<Self, StoreError> {
        if application == 0 || defaults.amount <= 0 || defaults.interval <= 0 {
            return Err(StoreError::Configuration(
                "application ID and grant settings must be positive",
            ));
        }
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await?;
        let version: Option<i32> = sqlx::query_scalar("SELECT version FROM prediction_schema")
            .fetch_optional(&pool)
            .await?;
        if version != Some(1) {
            return Err(StoreError::Configuration(
                "run the supported schema migration first",
            ));
        }
        let mutable: bool = sqlx::query_scalar("SELECT has_table_privilege('prediction_events', 'UPDATE,DELETE,TRUNCATE') OR has_table_privilege('prediction_commands', 'UPDATE,DELETE,TRUNCATE')").fetch_one(&pool).await?;
        if mutable {
            return Err(StoreError::Configuration(
                "use a restricted runtime database role",
            ));
        }
        let store = Self::new_with_audit(pool, application, defaults, audit);
        for guild in store.guilds().await? {
            store.view(guild).await?;
        }
        Ok(store)
    }

    pub(crate) fn audit(&self) -> &SharedAudit {
        &self.audit
    }

    /// Keep this connection alive for the gateway lifetime. Two-key locks occupy
    /// a separate namespace from the one-key guild transaction locks.
    ///
    /// # Errors
    /// Returns an error if acquiring a connection fails or another gateway holds the lock.
    pub async fn gateway_guard(
        &self,
    ) -> Result<sqlx::pool::PoolConnection<sqlx::Postgres>, StoreError> {
        let mut connection = self.pool.acquire().await?;
        // Preserve all 64 identity bits in the two signed PostgreSQL lock keys.
        let bytes = self.application.to_be_bytes();
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
            .bind(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .bind(i32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]))
            .fetch_one(&mut *connection)
            .await?;
        if !acquired {
            return Err(StoreError::Configuration(
                "another gateway is already running",
            ));
        }
        connection.close_on_drop();
        Ok(connection)
    }

    /// Atomically append a command's events, or return its existing receipt on redelivery.
    ///
    /// # Errors
    /// Returns an error for invalid commands or history, metadata errors, or database failures.
    pub async fn execute(
        &self,
        guild: u64,
        key: &str,
        actor: Actor,
        command: &Command,
    ) -> Result<String, StoreError> {
        self.execute_inner(guild, key, actor, command, None).await
    }

    /// Explicit application time boundary for deterministic tests and simulations.
    ///
    /// # Errors
    /// Returns the same command, history, metadata, and database errors as [`Self::execute`].
    pub async fn execute_at(
        &self,
        guild: u64,
        key: &str,
        actor: Actor,
        command: &Command,
        now: i64,
    ) -> Result<String, StoreError> {
        self.execute_inner(guild, key, actor, command, Some(now))
            .await
    }

    async fn execute_inner(
        &self,
        guild: u64,
        key: &str,
        actor: Actor,
        command: &Command,
        now: Option<i64>,
    ) -> Result<String, StoreError> {
        let started = Instant::now();
        let audit_key = canonical_command_key(key);
        let command_kind = CommandKind::from(command);
        if guild == 0
            || key.is_empty()
            || key.len() > 200
            || key.chars().any(char::is_control)
            || !(key.starts_with("discord:") || key.starts_with("grant:"))
            || key.ends_with(':')
            || (actor.user_id == 0) != key.starts_with("grant:")
        {
            let error = StoreError::Configuration("invalid guild or command key");
            self.audit.on_event(&AuditEvent::CommandCompleted {
                guild,
                key: audit_key,
                command: command_kind,
                outcome: store_outcome(Stage::Validate, &error),
                stage: Stage::Validate,
                elapsed: started.elapsed(),
            });
            return Err(error);
        }

        let mut attempt = 0;
        let result = loop {
            let result = self.transact(guild, key, actor, command, now).await;
            if let Err(TransactionFailure {
                error: StoreError::Database(sqlx::Error::Database(error)),
                ..
            }) = &result
                && matches!(error.code().as_deref(), Some("40001" | "40P01" | "23505"))
                && attempt < 2
            {
                attempt += 1;
                continue;
            }
            break result;
        };
        let (stage, outcome) = match &result {
            Ok(success) => (success.stage, Outcome::Succeeded),
            Err(failure) => (failure.stage, store_outcome(failure.stage, &failure.error)),
        };
        self.audit.on_event(&AuditEvent::CommandCompleted {
            guild,
            key: audit_key,
            command: command_kind,
            outcome,
            stage,
            elapsed: started.elapsed(),
        });
        result
            .map(|success| success.response)
            .map_err(|failure| failure.error)
    }

    async fn transact(
        &self,
        guild: u64,
        key: &str,
        actor: Actor,
        command: &Command,
        time: Option<i64>,
    ) -> Result<TransactionSuccess, TransactionFailure> {
        let mut tx = at_stage(Stage::Acquire, self.pool.begin().await)?;
        at_stage(
            Stage::Acquire,
            sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
                .execute(&mut *tx)
                .await,
        )?;
        at_stage(
            Stage::Acquire,
            sqlx::query("SELECT pg_advisory_xact_lock($1)")
                .bind(i64::from_ne_bytes(guild.to_ne_bytes()))
                .execute(&mut *tx)
                .await,
        )?;
        let receipt = at_stage(
            Stage::Replay,
            sqlx::query("SELECT actor_id, response FROM prediction_commands WHERE guild_id=$1 AND command_key=$2")
                .bind(guild.to_string()).bind(key).fetch_optional(&mut *tx).await,
        )?;
        if let Some(receipt) = receipt {
            if at_stage(Stage::Replay, receipt.try_get::<String, _>("actor_id"))?
                != actor.user_id.to_string()
            {
                return Err(TransactionFailure {
                    stage: Stage::Replay,
                    error: StoreError::History("command actor mismatch"),
                });
            }
            let response: String = at_stage(Stage::Replay, receipt.try_get("response"))?;
            at_stage(Stage::Commit, tx.commit().await)?;
            at_stage(Stage::Refresh, self.view(guild).await)?;
            return Ok(TransactionSuccess {
                response,
                stage: Stage::Refresh,
            });
        }
        let mut view = at_stage(Stage::Replay, self.load(&mut tx, guild).await)?;
        // Capture production time only after acquiring the guild lock.
        let accepted_at = time.unwrap_or_else(|| chrono::Utc::now().timestamp());
        let decision = at_stage(
            Stage::Decide,
            domain::decide(&view.state, actor, command, accepted_at, self.defaults),
        )?;
        // A clock rollback can make a discovered grant premature. Keep its key
        // retryable until a grant actually advances the schedule.
        if matches!(command, Command::Grant { .. }) && decision.events.is_empty() {
            at_stage(Stage::Commit, tx.commit().await)?;
            self.publish(guild, view).await;
            return Ok(TransactionSuccess {
                response: decision.response,
                stage: Stage::Commit,
            });
        }
        for event in &decision.events {
            at_stage(Stage::Append, validate_event_actor(event, actor.user_id))?;
            at_stage(Stage::Append, validate_event_time(event, accepted_at))?;
            view.revision = view.revision.checked_add(1).ok_or(TransactionFailure {
                stage: Stage::Append,
                error: StoreError::History("revision overflow"),
            })?;
            let ctx = Context {
                application: self.application,
                guild,
                revision: view.revision,
                command: key,
                accepted_at,
            };
            let data = serde_json::to_value(event).map_err(|_| TransactionFailure {
                stage: Stage::Append,
                error: StoreError::History("event cannot be serialized"),
            })?;
            let cloud = at_stage(
                Stage::Append,
                CloudEvent::new(&ctx, event.name(), event.subject(), data),
            )?;
            at_stage(Stage::Append, domain::apply(&mut view.state, event))?;
            at_stage(
                Stage::Append,
                sqlx::query("INSERT INTO prediction_events(guild_id, revision, command_key, accepted_at, event) VALUES ($1,$2,$3,$4,$5)")
                .bind(guild.to_string()).bind(view.revision).bind(key).bind(accepted_at).bind(Json(cloud))
                .execute(&mut *tx).await,
            )?;
        }
        at_stage(
            Stage::Append,
            sqlx::query("INSERT INTO prediction_commands(guild_id,command_key,actor_id,accepted_at,response,last_revision) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(guild.to_string()).bind(key).bind(actor.user_id.to_string()).bind(accepted_at)
            .bind(&decision.response).bind(view.revision).execute(&mut *tx).await,
        )?;
        at_stage(Stage::Commit, tx.commit().await)?;
        self.publish(guild, view).await;
        Ok(TransactionSuccess {
            response: decision.response,
            stage: Stage::Commit,
        })
    }

    async fn load(&self, connection: &mut PgConnection, guild: u64) -> Result<View, StoreError> {
        // One SELECT snapshot sees either all or none of a command's event rows.
        let rows = sqlx::query("SELECT e.revision, e.command_key, e.accepted_at, e.event, c.actor_id, c.accepted_at AS receipt_time, c.last_revision FROM prediction_events e LEFT JOIN prediction_commands c ON c.guild_id=e.guild_id AND c.command_key=e.command_key WHERE e.guild_id=$1 ORDER BY e.revision")
            .bind(guild.to_string()).fetch_all(connection).await?;
        let mut view = View {
            revision: 0,
            state: State::default(),
        };
        let mut identities = HashSet::new();
        let mut events = Vec::with_capacity(rows.len());
        let mut enrollment: Option<(u64, String, i64)> = None;
        let mut command_end: Option<(String, i64)> = None;
        let mut completed_commands = HashSet::new();
        for row in rows {
            let revision: i64 = row.try_get("revision")?;
            if Some(revision) != view.revision.checked_add(1) {
                return Err(StoreError::History("non-contiguous event revisions"));
            }
            let key: String = row.try_get("command_key")?;
            let accepted_at: i64 = row.try_get("accepted_at")?;
            let receipt_time: i64 = row.try_get("receipt_time")?;
            let last_revision: i64 = row.try_get("last_revision")?;
            let actor_id: String = row.try_get("actor_id")?;
            let actor: u64 = actor_id
                .parse()
                .map_err(|_| StoreError::History("invalid receipt actor"))?;
            if actor_id != actor.to_string()
                || accepted_at != receipt_time
                || last_revision < revision
            {
                return Err(StoreError::History("receipt disagrees with event metadata"));
            }
            if let Some((prior_key, prior_end)) = command_end.as_ref() {
                if prior_key != &key {
                    if *prior_end != view.revision
                        || !completed_commands.insert(prior_key.clone())
                        || completed_commands.contains(&key)
                    {
                        return Err(StoreError::History("non-contiguous command events"));
                    }
                } else if *prior_end != last_revision {
                    return Err(StoreError::History("inconsistent command boundary"));
                }
            }
            command_end = Some((key.clone(), last_revision));
            let cloud: Json<CloudEvent> = row.try_get("event")?;
            let event: Event = serde_json::from_value(cloud.data.clone())
                .map_err(|_| StoreError::History("unsupported domain payload"))?;
            let ctx = Context {
                application: self.application,
                guild,
                revision,
                command: &key,
                accepted_at,
            };
            cloud.validate(&ctx, event.name(), &event.subject())?;
            if !identities.insert((cloud.source.clone(), cloud.id.clone())) {
                return Err(StoreError::History("duplicate event identity"));
            }
            validate_event_time(&event, accepted_at)?;
            validate_event_actor(&event, actor)?;
            if let Some((user, command, timestamp)) = enrollment.take() {
                if !matches!(&event, Event::PointsGranted { user_id, reason: GrantReason::Initial, .. } if *user_id == user)
                    || key != command
                    || accepted_at != timestamp
                {
                    return Err(StoreError::History(
                        "enrollment is missing its initial grant",
                    ));
                }
            } else if matches!(
                &event,
                Event::PointsGranted {
                    reason: GrantReason::Initial,
                    ..
                }
            ) {
                return Err(StoreError::History("initial grant has no enrollment"));
            }
            if let Event::MemberEnrolled { user_id, .. } = &event {
                enrollment = Some((*user_id, key.clone(), accepted_at));
            }
            events.push(event);
            view.revision = revision;
        }
        if enrollment.is_some() {
            return Err(StoreError::History("incomplete enrollment"));
        }
        if command_end.is_some_and(|(_, end)| end != view.revision) {
            return Err(StoreError::History("incomplete command history"));
        }
        view.state = domain::replay(&events)?;
        Ok(view)
    }

    async fn publish(&self, guild: u64, view: View) -> Arc<View> {
        let mut views = self.views.lock().await;
        if let Some(current) = views.get(&guild)
            && current.revision >= view.revision
        {
            return Arc::clone(current);
        }
        let view = Arc::new(view);
        views.insert(guild, Arc::clone(&view));
        view
    }

    /// Replay committed events and publish an immutable projection.
    ///
    /// # Errors
    /// Returns an error for database failures or invalid event history and metadata.
    pub async fn view(&self, guild: u64) -> Result<Arc<View>, StoreError> {
        let mut connection = self.pool.acquire().await?;
        let view = self.load(&mut connection, guild).await?;
        Ok(self.publish(guild, view).await)
    }

    /// List guilds with committed event history.
    ///
    /// # Errors
    /// Returns an error if the query fails or a stored guild ID is invalid.
    pub async fn guilds(&self) -> Result<Vec<u64>, StoreError> {
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT guild_id FROM prediction_events ORDER BY guild_id")
                .fetch_all(&self.pool)
                .await?;
        ids.into_iter()
            .map(|id| {
                id.parse()
                    .map_err(|_| StoreError::History("invalid guild ID"))
            })
            .collect()
    }

    /// Issue due grants, logging and continuing past individual guild or command failures.
    ///
    /// # Errors
    /// Returns an error if guild discovery fails.
    pub async fn grant_due(&self) -> Result<(), StoreError> {
        for guild in self.guilds().await? {
            let view = match self.view(guild).await {
                Ok(view) => view,
                Err(error) => {
                    self.audit.on_event(&AuditEvent::GrantFailed {
                        guild: Some(guild),
                        outcome: store_outcome(Stage::Reconstruct, &error),
                        stage: Stage::Reconstruct,
                    });
                    continue;
                }
            };
            let now = chrono::Utc::now().timestamp();
            for (user, account) in &view.state.accounts {
                if account.next_grant <= now {
                    let key = format!("grant:{user}:{}", account.next_grant);
                    let actor = Actor {
                        user_id: 0,
                        moderator: false,
                        bot: false,
                    };
                    let _ = self
                        .execute(guild, &key, actor, &Command::Grant { user_id: *user })
                        .await;
                }
            }
        }
        Ok(())
    }
}

fn validate_event_actor(event: &Event, actor: u64) -> Result<(), StoreError> {
    let expected = match event {
        Event::GuildEconomyInitialized { .. } => {
            return if actor != 0 {
                Ok(())
            } else {
                Err(StoreError::History("system cannot enroll"))
            };
        }
        Event::MemberEnrolled { user_id, .. }
        | Event::BetPlaced { user_id, .. }
        | Event::PointsGranted {
            user_id,
            reason: GrantReason::Initial,
            ..
        } => *user_id,
        Event::PointsGranted {
            reason: GrantReason::Periodic,
            ..
        } => 0,
        Event::MarketCreated { creator, .. } => *creator,
        Event::MarketResolved { resolver, .. } => *resolver,
        Event::MarketCancelled { moderator, .. } => *moderator,
    };
    if actor != expected {
        return Err(StoreError::History("receipt actor disagrees with event"));
    }
    Ok(())
}

fn validate_event_time(event: &Event, accepted_at: i64) -> Result<(), StoreError> {
    let matches = match event {
        Event::GuildEconomyInitialized { .. } => true,
        Event::MemberEnrolled { enrolled_at, .. } => *enrolled_at == accepted_at,
        Event::PointsGranted { through_due, .. } => *through_due <= accepted_at,
        Event::MarketCreated { created_at, .. } => *created_at == accepted_at,
        Event::BetPlaced {
            accepted_at: time, ..
        } => *time == accepted_at,
        Event::MarketResolved { settled_at, .. } => *settled_at == accepted_at,
        Event::MarketCancelled { cancelled_at, .. } => *cancelled_at == accepted_at,
    };
    if !matches {
        return Err(StoreError::History(
            "payload timestamp disagrees with metadata",
        ));
    }
    Ok(())
}

/// Apply versioned schema and role migrations. Existing login passwords are preserved.
///
/// # Errors
/// Returns an error for an empty password, migration failure, or insufficient owner privileges.
pub async fn migrate(pool: &PgPool, runtime_password: &str) -> Result<(), StoreError> {
    if runtime_password.trim().is_empty() {
        return Err(StoreError::Configuration(
            "runtime password must not be empty",
        ));
    }
    let migrator = Migrator::with_migrations(vec![
        Migration::new(
            1,
            "event store".into(),
            MigrationType::Simple,
            include_str!("../../migrations/001_event_store.sql").into_sql_str(),
            false,
        ),
        Migration::new(
            2,
            "application login".into(),
            MigrationType::Simple,
            include_str!("../../migrations/002_application_login.sql").into_sql_str(),
            false,
        ),
    ]);
    let mut connection = pool.acquire().await?;
    // Never return the password-bearing session (or a failed migration's lock) to the pool.
    connection.close_on_drop();
    sqlx::query("SELECT set_config('prediction_bot.runtime_password', $1, false)")
        .bind(runtime_password)
        .execute(&mut *connection)
        .await?;
    migrator.run(&mut *connection).await?;
    Ok(())
}
