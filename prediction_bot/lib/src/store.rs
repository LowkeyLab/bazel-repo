//! Commands append individual events atomically; queries expose immutable replayed views.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use sqlx::{PgConnection, PgPool, Row, postgres::PgPoolOptions, types::Json};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::domain::{self, Actor, Command, DomainError, Event, GrantReason, Policy, State};
use crate::events::{CloudEvent, Context, EventError};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
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
}

impl Store {
    pub fn new(pool: PgPool, application: u64, defaults: Policy) -> Self {
        Self {
            pool,
            application,
            defaults,
            views: Mutex::new(HashMap::new()),
        }
    }

    pub async fn connect(
        url: &str,
        application: u64,
        defaults: Policy,
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
        let store = Self::new(pool, application, defaults);
        for guild in store.guilds().await? {
            store.view(guild).await?;
        }
        Ok(store)
    }

    /// Keep this connection alive for the gateway lifetime. Two-key locks occupy
    /// a separate namespace from the one-key guild transaction locks.
    pub async fn gateway_guard(
        &self,
    ) -> Result<sqlx::pool::PoolConnection<sqlx::Postgres>, StoreError> {
        let mut connection = self.pool.acquire().await?;
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1, $2)")
            .bind((self.application >> 32) as i32)
            .bind(self.application as i32)
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
        if guild == 0
            || key.is_empty()
            || key.len() > 200
            || key.chars().any(char::is_control)
            || !(key.starts_with("discord:") || key.starts_with("grant:"))
            || key.ends_with(':')
            || (actor.user_id == 0) != key.starts_with("grant:")
        {
            return Err(StoreError::Configuration("invalid guild or command key"));
        }
        for attempt in 0..3 {
            let result = self.transact(guild, key, actor, command, now).await;
            if let Err(StoreError::Database(sqlx::Error::Database(ref error))) = result {
                if attempt < 2
                    && matches!(error.code().as_deref(), Some("40001" | "40P01" | "23505"))
                {
                    continue;
                }
            }
            return result;
        }
        unreachable!("bounded retry returns on its last attempt")
    }

    async fn transact(
        &self,
        guild: u64,
        key: &str,
        actor: Actor,
        command: &Command,
        time: Option<i64>,
    ) -> Result<String, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(guild as i64)
            .execute(&mut *tx)
            .await?;
        let receipt = sqlx::query("SELECT actor_id, response FROM prediction_commands WHERE guild_id=$1 AND command_key=$2")
            .bind(guild.to_string()).bind(key).fetch_optional(&mut *tx).await?;
        if let Some(receipt) = receipt {
            if receipt.try_get::<String, _>("actor_id")? != actor.user_id.to_string() {
                return Err(StoreError::History("command actor mismatch"));
            }
            let response: String = receipt.try_get("response")?;
            tx.commit().await?;
            self.view(guild).await?;
            return Ok(response);
        }
        let mut view = self.load(&mut tx, guild).await?;
        // Capture production time only after acquiring the guild lock.
        let accepted_at = time.unwrap_or_else(|| chrono::Utc::now().timestamp());
        let decision = domain::decide(&view.state, actor, command, accepted_at, self.defaults)?;
        for event in &decision.events {
            validate_event_actor(event, actor.user_id)?;
            validate_event_time(event, accepted_at)?;
            view.revision = view
                .revision
                .checked_add(1)
                .ok_or(StoreError::History("revision overflow"))?;
            let ctx = Context {
                application: self.application,
                guild,
                revision: view.revision,
                command: key,
                accepted_at,
            };
            let data = serde_json::to_value(event)
                .map_err(|_| StoreError::History("event cannot be serialized"))?;
            let cloud = CloudEvent::new(&ctx, event.name(), event.subject(), data)?;
            domain::apply(&mut view.state, event)?;
            sqlx::query("INSERT INTO prediction_events(guild_id, revision, command_key, accepted_at, event) VALUES ($1,$2,$3,$4,$5)")
                .bind(guild.to_string()).bind(view.revision).bind(key).bind(accepted_at).bind(Json(cloud))
                .execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO prediction_commands(guild_id,command_key,actor_id,accepted_at,response,last_revision) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(guild.to_string()).bind(key).bind(actor.user_id.to_string()).bind(accepted_at)
            .bind(&decision.response).bind(view.revision).execute(&mut *tx).await?;
        tx.commit().await?;
        self.publish(guild, view).await;
        Ok(decision.response)
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
        if let Some(current) = views.get(&guild) {
            if current.revision >= view.revision {
                return Arc::clone(current);
            }
        }
        let view = Arc::new(view);
        views.insert(guild, Arc::clone(&view));
        view
    }

    pub async fn view(&self, guild: u64) -> Result<Arc<View>, StoreError> {
        let mut connection = self.pool.acquire().await?;
        let view = self.load(&mut connection, guild).await?;
        Ok(self.publish(guild, view).await)
    }

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

    pub async fn grant_due(&self) -> Result<(), StoreError> {
        for guild in self.guilds().await? {
            let view = match self.view(guild).await {
                Ok(view) => view,
                Err(_) => {
                    tracing::error!(guild, "cannot reconstruct grant schedule");
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
                    if self
                        .execute(guild, &key, actor, &Command::Grant { user_id: *user })
                        .await
                        .is_err()
                    {
                        tracing::error!(guild, user, "periodic grant failed");
                    }
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
        Event::MemberEnrolled { user_id, .. } | Event::BetPlaced { user_id, .. } => *user_id,
        Event::PointsGranted {
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

pub async fn migrate(pool: &PgPool) -> Result<(), StoreError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(173728393, 1)")
        .execute(&mut *tx)
        .await?;
    sqlx::raw_sql(include_str!("../../migrations/001_event_store.sql"))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
