use sqlx::{Row, types::Json};

use super::{AnnouncementStatus, ConfigurationChange, SnapshotV1};
use crate::{
    domain::{Actor, Event, State},
    store::{Store, StoreError},
};

pub(crate) async fn enqueue(
    tx: &mut sqlx::PgConnection,
    guild: u64,
    revision: i64,
    event: &Event,
    state: &State,
) -> Result<(), StoreError> {
    let enabled: Option<bool> = sqlx::query_scalar(
        "SELECT enabled FROM prediction_announcement_settings WHERE guild_id=$1",
    )
    .bind(guild.to_string())
    .fetch_optional(&mut *tx)
    .await?;
    if enabled != Some(true) {
        return Ok(());
    }

    let (snapshot, occurred_at) = match event {
        Event::MarketCreated {
            id,
            creator,
            question,
            options,
            closes_at,
            created_at,
        } => (
            SnapshotV1::Created {
                id: id.clone(),
                question: question.clone(),
                creator: *creator,
                options: options.clone(),
                closes_at: *closes_at,
                occurred_at: *created_at,
            },
            *created_at,
        ),
        Event::MarketResolved {
            id,
            outcome,
            refunded,
            settled_at,
            ..
        } => {
            let market = state.markets.get(id).ok_or(StoreError::History(
                "announcement event disagrees with applied state",
            ))?;
            let winner = market.options.get(*outcome).ok_or(StoreError::History(
                "announcement event disagrees with applied state",
            ))?;
            (
                SnapshotV1::Resolved {
                    id: id.clone(),
                    question: market.question.clone(),
                    winner: winner.clone(),
                    refunded: *refunded,
                    occurred_at: *settled_at,
                },
                *settled_at,
            )
        }
        Event::MarketCancelled {
            id, cancelled_at, ..
        } => {
            let market = state.markets.get(id).ok_or(StoreError::History(
                "announcement event disagrees with applied state",
            ))?;
            (
                SnapshotV1::Cancelled {
                    id: id.clone(),
                    question: market.question.clone(),
                    occurred_at: *cancelled_at,
                },
                *cancelled_at,
            )
        }
        _ => return Ok(()),
    };

    sqlx::query(
        "INSERT INTO prediction_announcement_outbox(guild_id,revision,snapshot_version,snapshot,next_attempt_at) VALUES ($1,$2,1,$3,$4)",
    )
    .bind(guild.to_string())
    .bind(revision)
    .bind(Json(snapshot))
    .bind(occurred_at)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

fn validate_administrator(guild: u64, actor: Actor) -> Result<(), StoreError> {
    if guild == 0 || actor.user_id == 0 || !actor.moderator || actor.bot {
        return Err(StoreError::Configuration(
            "announcement configuration requires a guild administrator",
        ));
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<(), StoreError> {
    if key.is_empty()
        || key.len() > 200
        || key.chars().any(char::is_control)
        || !key.starts_with("discord:")
        || key.ends_with(':')
    {
        return Err(StoreError::Configuration("invalid guild or command key"));
    }
    Ok(())
}

impl Store {
    /// Change a guild's announcement destination or disable future delivery.
    ///
    /// # Errors
    /// Returns an error for unauthorized actors, invalid identifiers, history conflicts, or
    /// database failures.
    pub async fn configure_announcements(
        &self,
        guild: u64,
        key: &str,
        actor: Actor,
        change: ConfigurationChange,
    ) -> Result<String, StoreError> {
        validate_administrator(guild, actor)?;
        validate_key(key)?;
        if matches!(change, ConfigurationChange::Set { channel_id: 0 }) {
            return Err(StoreError::Configuration("invalid announcement channel"));
        }

        let guild_id = guild.to_string();
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(i64::from_ne_bytes(guild.to_ne_bytes()))
            .execute(&mut *tx)
            .await?;

        let receipt = sqlx::query(
            "SELECT actor_id, response FROM prediction_commands WHERE guild_id=$1 AND command_key=$2",
        )
        .bind(&guild_id)
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(receipt) = receipt {
            if receipt.try_get::<String, _>("actor_id")? != actor.user_id.to_string() {
                return Err(StoreError::History("command actor mismatch"));
            }
            let response = receipt.try_get("response")?;
            tx.commit().await?;
            return Ok(response);
        }

        let current_version: Option<i64> = sqlx::query_scalar(
            "SELECT configuration_version FROM prediction_announcement_settings WHERE guild_id=$1",
        )
        .bind(&guild_id)
        .fetch_optional(&mut *tx)
        .await?;
        let version = current_version
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(StoreError::History("configuration version overflow"))?;
        let accepted_at = chrono::Utc::now().timestamp();
        let response = match change {
            ConfigurationChange::Set { channel_id } => {
                sqlx::query(
                    "INSERT INTO prediction_announcement_settings(guild_id,channel_id,enabled,configuration_version,pause_reason) VALUES ($1,$2,TRUE,$3,NULL) ON CONFLICT (guild_id) DO UPDATE SET channel_id=EXCLUDED.channel_id,enabled=TRUE,configuration_version=EXCLUDED.configuration_version,pause_reason=NULL",
                )
                .bind(&guild_id)
                .bind(channel_id.to_string())
                .bind(version)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE prediction_announcement_outbox SET attempts=0,next_attempt_at=$2,last_failure=NULL WHERE guild_id=$1 AND state='pending'",
                )
                .bind(&guild_id)
                .bind(accepted_at)
                .execute(&mut *tx)
                .await?;
                format!("Announcements enabled for <#{channel_id}>.")
            }
            ConfigurationChange::Disable => {
                sqlx::query(
                    "INSERT INTO prediction_announcement_settings(guild_id,channel_id,enabled,configuration_version,pause_reason) VALUES ($1,NULL,FALSE,$2,NULL) ON CONFLICT (guild_id) DO UPDATE SET enabled=FALSE,configuration_version=EXCLUDED.configuration_version,pause_reason=NULL",
                )
                .bind(&guild_id)
                .bind(version)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE prediction_announcement_outbox SET state='discarded' WHERE guild_id=$1 AND state='pending'",
                )
                .bind(&guild_id)
                .execute(&mut *tx)
                .await?;
                "Announcements disabled.".to_owned()
            }
        };
        let revision: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(revision), 0) FROM prediction_events WHERE guild_id=$1",
        )
        .bind(&guild_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO prediction_commands(guild_id,command_key,actor_id,accepted_at,response,last_revision) VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(&guild_id)
        .bind(key)
        .bind(actor.user_id.to_string())
        .bind(accepted_at)
        .bind(&response)
        .bind(revision)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(response)
    }

    /// Return a guild's announcement configuration and queued-work count.
    ///
    /// # Errors
    /// Returns an error for unauthorized actors, invalid identifiers, corrupt settings, or
    /// database failures.
    pub async fn announcement_status(
        &self,
        guild: u64,
        actor: Actor,
    ) -> Result<AnnouncementStatus, StoreError> {
        validate_administrator(guild, actor)?;
        let guild_id = guild.to_string();
        let row = sqlx::query(
            "SELECT channel_id,enabled,configuration_version,pause_reason,(SELECT COUNT(*) FROM prediction_announcement_outbox WHERE guild_id=$1 AND state='pending') AS pending FROM prediction_announcement_settings WHERE guild_id=$1",
        )
        .bind(&guild_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(AnnouncementStatus {
                channel_id: None,
                enabled: false,
                version: 0,
                pause_reason: None,
                pending: 0,
            });
        };
        let channel_id = row
            .try_get::<Option<String>, _>("channel_id")?
            .map(|channel| {
                channel
                    .parse()
                    .map_err(|_| StoreError::History("invalid announcement channel ID"))
            })
            .transpose()?;
        Ok(AnnouncementStatus {
            channel_id,
            enabled: row.try_get("enabled")?,
            version: row.try_get("configuration_version")?,
            pause_reason: row.try_get("pause_reason")?,
            pending: row.try_get("pending")?,
        })
    }
}
