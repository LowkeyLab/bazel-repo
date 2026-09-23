use sqlx::{Row, types::Json};

use super::{AnnouncementStatus, ConfigurationChange, SnapshotV1};
use crate::events::{CloudEvent, Context};
use crate::odds::OutcomeOdds;
use crate::types::{ChannelId, ConfigurationVersion, EventRevision, GuildId};
use crate::{
    domain::{Actor, Event, State},
    store::{Store, StoreError},
};

pub(crate) async fn enqueue(
    tx: &mut sqlx::PgConnection,
    guild: GuildId,
    revision: EventRevision,
    event: &Event,
    state: &State,
    previous_odds: &[OutcomeOdds],
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

    let Some((snapshot, occurred_at)) = event_snapshot(event, state, previous_odds)? else {
        return Ok(());
    };

    sqlx::query(
        "INSERT INTO prediction_announcement_outbox(guild_id,revision,snapshot_version,snapshot,next_attempt_at) VALUES ($1,$2,1,$3,$4)",
    )
    .bind(guild.to_string())
    .bind(revision.0)
    .bind(Json(snapshot))
    .bind(occurred_at)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

fn event_snapshot(
    event: &Event,
    state: &State,
    previous_odds: &[OutcomeOdds],
) -> Result<Option<(SnapshotV1, i64)>, StoreError> {
    let snapshot = match event {
        Event::AnnouncementsEnabled { enabled_at, .. } => (
            SnapshotV1::Enabled {
                occurred_at: *enabled_at,
            },
            *enabled_at,
        ),
        Event::MemberEnrolled {
            user_id,
            enrolled_at,
        } => (
            SnapshotV1::MemberEnrolled {
                user_id: *user_id,
                occurred_at: *enrolled_at,
            },
            *enrolled_at,
        ),
        Event::BetPlaced {
            id, accepted_at, ..
        } => {
            let market = state.markets.get(id).ok_or(StoreError::History(
                "announcement event disagrees with applied state",
            ))?;
            (
                SnapshotV1::BetPlaced {
                    id: id.clone(),
                    question: market.question.clone(),
                    bet_count: market.bets.len(),
                    odds: OutcomeOdds::for_market(market)
                        .into_iter()
                        .enumerate()
                        .map(|(index, odds)| match previous_odds.get(index) {
                            Some(previous) => odds.with_previous(previous),
                            None => odds,
                        })
                        .collect(),
                    occurred_at: *accepted_at,
                },
                *accepted_at,
            )
        }
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
            let winner = market.options.get(outcome.0).ok_or(StoreError::History(
                "announcement event disagrees with applied state",
            ))?;
            (
                SnapshotV1::Resolved {
                    id: id.clone(),
                    question: market.question.clone(),
                    winner: winner.clone(),
                    refunded: *refunded,
                    odds: OutcomeOdds::for_market(market),
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
                    odds: OutcomeOdds::for_market(market),
                    occurred_at: *cancelled_at,
                },
                *cancelled_at,
            )
        }
        _ => return Ok(None),
    };
    Ok(Some(snapshot))
}

fn validate_administrator(guild: GuildId, actor: Actor) -> Result<(), StoreError> {
    if guild.0 == 0 || actor.user_id.0 == 0 || !actor.moderator || actor.bot {
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

fn validate_configuration_request(
    guild: GuildId,
    key: &str,
    actor: Actor,
    change: ConfigurationChange,
) -> Result<(), StoreError> {
    validate_administrator(guild, actor)?;
    validate_key(key)?;
    if matches!(
        change,
        ConfigurationChange::Set {
            channel_id: ChannelId(0)
        }
    ) {
        return Err(StoreError::Configuration("invalid announcement channel"));
    }
    Ok(())
}

impl Store {
    // A committed receipt is immutable and can be recovered before external validation.
    pub(crate) async fn announcement_configuration_receipt(
        &self,
        guild: GuildId,
        key: &str,
        actor: Actor,
    ) -> Result<Option<String>, StoreError> {
        validate_administrator(guild, actor)?;
        validate_key(key)?;
        let receipt: Option<(String, String)> = sqlx::query_as(
            "SELECT actor_id, response FROM prediction_commands WHERE guild_id=$1 AND command_key=$2",
        )
        .bind(guild.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        match receipt {
            Some((actor_id, _)) if actor_id != actor.user_id.to_string() => {
                Err(StoreError::History("command actor mismatch"))
            }
            Some((_, response)) => Ok(Some(response)),
            None => Ok(None),
        }
    }

    /// Change a guild's announcement destination or disable future delivery.
    ///
    /// # Errors
    /// Returns an error for unauthorized actors, invalid identifiers, history conflicts, or
    /// database failures.
    pub async fn configure_announcements(
        &self,
        guild: GuildId,
        key: &str,
        actor: Actor,
        change: ConfigurationChange,
    ) -> Result<String, StoreError> {
        validate_configuration_request(guild, key, actor, change)?;

        let guild_id = guild.to_string();
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(i64::from_ne_bytes(guild.0.to_ne_bytes()))
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

        let current: Option<(i64, bool, Option<String>)> = sqlx::query_as(
            "SELECT configuration_version,enabled,channel_id FROM prediction_announcement_settings WHERE guild_id=$1",
        )
        .bind(&guild_id)
        .fetch_optional(&mut *tx)
        .await?;
        let welcome_channel = match change {
            ConfigurationChange::Set { channel_id }
                if !current.as_ref().is_some_and(|(_, enabled, channel)| {
                    *enabled && channel.as_deref() == Some(channel_id.to_string().as_str())
                }) =>
            {
                Some(channel_id)
            }
            _ => None,
        };
        let current_version = current
            .map(|(version, _, _)| ConfigurationVersion(version))
            .unwrap_or_default();
        let version = ConfigurationVersion(
            current_version
                .0
                .checked_add(1)
                .ok_or(StoreError::History("configuration version overflow"))?,
        );
        let accepted_at = chrono::Utc::now().timestamp();
        let response = match change {
            ConfigurationChange::Set { channel_id } => {
                sqlx::query(
                    "INSERT INTO prediction_announcement_settings(guild_id,channel_id,enabled,configuration_version,pause_reason) VALUES ($1,$2,TRUE,$3,NULL) ON CONFLICT (guild_id) DO UPDATE SET channel_id=EXCLUDED.channel_id,enabled=TRUE,configuration_version=EXCLUDED.configuration_version,pause_reason=NULL",
                )
                .bind(&guild_id)
                .bind(channel_id.to_string())
                .bind(version.0)
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
                .bind(version.0)
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
        let mut revision = EventRevision(
            sqlx::query_scalar(
                "SELECT COALESCE(MAX(revision), 0) FROM prediction_events WHERE guild_id=$1",
            )
            .bind(&guild_id)
            .fetch_one(&mut *tx)
            .await?,
        );
        if let Some(channel_id) = welcome_channel {
            revision = revision
                .next()
                .ok_or(StoreError::History("revision overflow"))?;
            let event = Event::AnnouncementsEnabled {
                channel_id,
                moderator: actor.user_id,
                enabled_at: accepted_at,
            };
            let context = Context {
                application: self.application_id(),
                guild,
                revision,
                command: key,
                accepted_at,
            };
            let data = serde_json::to_value(&event)
                .map_err(|_| StoreError::History("event cannot be serialized"))?;
            let cloud = CloudEvent::new(&context, event.name(), event.subject(), data)?;
            sqlx::query("INSERT INTO prediction_events(guild_id,revision,command_key,accepted_at,event) VALUES ($1,$2,$3,$4,$5)")
                .bind(&guild_id)
                .bind(revision.0)
                .bind(key)
                .bind(accepted_at)
                .bind(Json(cloud))
                .execute(&mut *tx)
                .await?;
            enqueue(&mut tx, guild, revision, &event, &State::default(), &[]).await?;
        }
        sqlx::query(
            "INSERT INTO prediction_commands(guild_id,command_key,actor_id,accepted_at,response,last_revision) VALUES ($1,$2,$3,$4,$5,$6)",
        )
        .bind(&guild_id)
        .bind(key)
        .bind(actor.user_id.to_string())
        .bind(accepted_at)
        .bind(&response)
        .bind(revision.0)
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
        guild: GuildId,
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
                version: ConfigurationVersion(0),
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
            version: ConfigurationVersion(row.try_get("configuration_version")?),
            pause_reason: row.try_get("pause_reason")?,
            pending: row.try_get("pending")?,
        })
    }
}

pub(crate) async fn next_due(
    store: &Store,
    now: i64,
    limit: i64,
) -> Result<Vec<super::PendingAnnouncement>, StoreError> {
    let rows = sqlx::query(
        "SELECT o.guild_id,o.revision,o.snapshot_version,o.snapshot,o.attempts,s.channel_id,s.configuration_version
         FROM prediction_announcement_outbox o
         JOIN prediction_announcement_settings s USING (guild_id)
         WHERE o.state='pending' AND s.enabled AND s.pause_reason IS NULL
           AND NOT EXISTS (SELECT 1 FROM prediction_announcement_outbox older
                           WHERE older.guild_id=o.guild_id AND older.state='pending' AND older.revision<o.revision)
           AND o.next_attempt_at<=$1
         ORDER BY o.next_attempt_at,o.guild_id LIMIT $2",
    ).bind(now).bind(limit.clamp(0, 8)).fetch_all(&store.pool).await?;
    rows.into_iter()
        .map(|row| {
            if row.try_get::<i32, _>("snapshot_version")? != 1 {
                return Err(StoreError::History(
                    "unsupported announcement snapshot version",
                ));
            }
            let snapshot = serde_json::from_value(row.try_get("snapshot")?)
                .map_err(|_| StoreError::History("invalid announcement snapshot"))?;
            Ok(super::PendingAnnouncement {
                guild: row
                    .try_get::<String, _>("guild_id")?
                    .parse()
                    .map_err(|_| StoreError::History("invalid announcement guild ID"))?,
                revision: EventRevision(row.try_get("revision")?),
                channel_id: row
                    .try_get::<String, _>("channel_id")?
                    .parse()
                    .map_err(|_| StoreError::History("invalid announcement channel ID"))?,
                configuration_version: ConfigurationVersion(row.try_get("configuration_version")?),
                attempts: row.try_get("attempts")?,
                snapshot,
            })
        })
        .collect()
}

pub(crate) async fn still_eligible(
    store: &Store,
    item: &super::PendingAnnouncement,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (
           SELECT 1 FROM prediction_announcement_outbox o
           JOIN prediction_announcement_settings s USING (guild_id)
           WHERE o.guild_id=$1 AND o.revision=$2 AND o.state='pending'
             AND s.enabled AND s.pause_reason IS NULL AND s.channel_id=$3 AND s.configuration_version=$4
             AND NOT EXISTS (SELECT 1 FROM prediction_announcement_outbox older
                             WHERE older.guild_id=o.guild_id AND older.state='pending' AND older.revision<o.revision))",
    ).bind(item.guild.to_string()).bind(item.revision.0).bind(item.channel_id.to_string())
        .bind(item.configuration_version.0).fetch_one(&store.pool).await?)
}

pub(crate) async fn finish_attempt(
    store: &Store,
    item: &super::PendingAnnouncement,
    outcome: &super::worker::AttemptOutcome,
    completed_at: i64,
) -> Result<(), StoreError> {
    use super::worker::{AttemptOutcome, retry_at};
    let mut tx = store.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(i64::from_ne_bytes(item.guild.0.to_ne_bytes()))
        .execute(&mut *tx)
        .await?;
    let guild = item.guild.to_string();
    if let AttemptOutcome::Delivered { message_id } = outcome {
        // A successful in-flight send remains an actual old-channel delivery even after Set.
        // Disable wins by discarding the row before this conditional update.
        sqlx::query("UPDATE prediction_announcement_outbox SET state='delivered',delivered_channel_id=$3,delivered_message_id=$4,last_failure=NULL WHERE guild_id=$1 AND revision=$2 AND state='pending'")
            .bind(&guild).bind(item.revision.0).bind(item.channel_id.to_string()).bind(message_id.to_string())
            .execute(&mut *tx).await?;
    } else {
        let applicable: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM prediction_announcement_outbox o JOIN prediction_announcement_settings s USING(guild_id)
             WHERE o.guild_id=$1 AND o.revision=$2 AND o.state='pending' AND s.configuration_version=$3)",
        ).bind(&guild).bind(item.revision.0).bind(item.configuration_version.0).fetch_one(&mut *tx).await?;
        if applicable {
            let attempts = item.attempts.saturating_add(1);
            let (reason, deadline) = match outcome {
                AttemptOutcome::Retry {
                    reason,
                    provider_delay,
                } => (*reason, retry_at(completed_at, attempts, *provider_delay)),
                AttemptOutcome::Pause { reason } => {
                    sqlx::query("UPDATE prediction_announcement_settings SET pause_reason=$2 WHERE guild_id=$1")
                        .bind(&guild).bind(reason).execute(&mut *tx).await?;
                    (*reason, completed_at)
                }
                AttemptOutcome::Delivered { .. } => unreachable!(),
            };
            sqlx::query("UPDATE prediction_announcement_outbox SET attempts=$3,next_attempt_at=$4,last_failure=$5 WHERE guild_id=$1 AND revision=$2 AND state='pending'")
                .bind(&guild).bind(item.revision.0).bind(attempts).bind(deadline).bind(reason).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}
