use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use serenity::http::HttpError;
use tokio::{
    sync::watch,
    task::{JoinHandle, JoinSet},
    time::Instant,
};

use super::{
    Clock, PendingAnnouncement,
    persistence::{finish_attempt, next_due, still_eligible},
};
use crate::{
    audit::{
        AnnouncementDecision, AuditEvent, Failure, FailureCategory, LifecycleKind, Outcome, Stage,
        discord_failure, store_outcome,
    },
    store::{Store, StoreError},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttemptOutcome {
    Delivered {
        message_id: u64,
    },
    Retry {
        reason: &'static str,
        provider_delay: Option<i64>,
    },
    Pause {
        reason: &'static str,
    },
}

pub(crate) fn retry_at(now: i64, failures: i64, provider_delay: Option<i64>) -> i64 {
    let exponent = u32::try_from(failures.saturating_sub(1).clamp(0, 6)).unwrap_or(6);
    let local_delay = 5_i64.saturating_mul(1_i64 << exponent).min(300);
    let delay = local_delay.max(provider_delay.unwrap_or(0).max(0));
    now.saturating_add(delay)
}

pub(crate) fn classify_delivery_failure(error: &serenity::Error) -> AttemptOutcome {
    match error {
        serenity::Error::Http(HttpError::UnsuccessfulRequest(response)) => {
            classify_http_failure(response.status_code.as_u16(), response.error.code)
        }
        serenity::Error::Http(HttpError::Request(error)) if error.is_timeout() => retry("timeout"),
        serenity::Error::Http(HttpError::Request(_))
        | serenity::Error::Io(_)
        | serenity::Error::Gateway(_)
        | serenity::Error::Tungstenite(_) => retry("transport"),
        serenity::Error::Http(
            HttpError::Url(_)
            | HttpError::InvalidWebhook
            | HttpError::InvalidHeader(_)
            | HttpError::InvalidScheme
            | HttpError::InvalidPort
            | HttpError::ApplicationIdMissing,
        )
        | serenity::Error::Model(_)
        | serenity::Error::Format(_)
        | serenity::Error::ExceededLimit(_, _)
        | serenity::Error::NotInRange(_, _, _, _)
        | serenity::Error::Url(_) => {
            pause("The announcement could not be prepared for Discord; contact an operator.")
        }
        serenity::Error::Json(_) => retry("discord_json"),
        serenity::Error::Http(_) => retry("discord_http"),
        _ => retry("discord_transport"),
    }
}

pub(crate) fn classify_http_failure(status: u16, discord_code: isize) -> AttemptOutcome {
    if status == 401 {
        return pause(
            "Discord authentication failed; an operator must correct the bot credentials.",
        );
    }
    if status == 403 || matches!(discord_code, 50_001 | 50_013) {
        return pause("Announcement delivery lacks permission to use the configured channel.");
    }
    if status == 404 || discord_code == 10_003 {
        return pause("The configured announcement channel is unavailable.");
    }
    if status == 429 {
        return retry("discord_rate_limit");
    }
    if (500..=599).contains(&status) {
        return retry("discord_server_error");
    }
    pause(
        "Discord rejected the announcement payload; reconfigure the destination or contact an operator.",
    )
}

fn retry(reason: &'static str) -> AttemptOutcome {
    AttemptOutcome::Retry {
        reason,
        provider_delay: None,
    }
}

fn pause(reason: &'static str) -> AttemptOutcome {
    AttemptOutcome::Pause { reason }
}

pub(crate) const SHUTDOWN_GRACE: Duration = Duration::from_secs(15);
pub(crate) type ShutdownDeadline = Arc<OnceLock<Instant>>;
type Attempts = JoinSet<Result<(), StoreError>>;

fn attempt_event(
    store: &Store,
    item: &PendingAnnouncement,
    decision: AnnouncementDecision,
    stage: Stage,
    outcome: Outcome,
) {
    store
        .audit()
        .on_event(&AuditEvent::AnnouncementAttemptCompleted {
            guild: item.guild,
            revision: item.revision,
            channel_id: item.channel_id,
            configuration_version: item.configuration_version,
            decision,
            stage,
            outcome,
        });
}

fn spawn_attempts(
    tasks: &mut Attempts,
    items: Vec<PendingAnnouncement>,
    store: &Arc<Store>,
    http: &Arc<serenity::http::Http>,
    clock: &Clock,
) {
    for item in items {
        let store = store.clone();
        let http = http.clone();
        let clock = clock.clone();
        tasks.spawn(async move {
            match still_eligible(&store, &item).await {
                Ok(false) => return Ok(()),
                Ok(true) => {}
                Err(error) => {
                    attempt_event(
                        &store,
                        &item,
                        AnnouncementDecision::NotSent,
                        Stage::Validate,
                        store_outcome(Stage::Validate, &error),
                    );
                    return Err(error);
                }
            }
            let request = serenity::all::ChannelId::new(item.channel_id)
                .send_message(&http, super::render::render(&item.snapshot));
            let (delivery, outcome) =
                match tokio::time::timeout(Duration::from_secs(30), request).await {
                    Ok(Ok(message)) => (
                        AttemptOutcome::Delivered {
                            message_id: message.id.get(),
                        },
                        Outcome::Succeeded,
                    ),
                    Ok(Err(error)) => (
                        classify_delivery_failure(&error),
                        Outcome::Failed(discord_failure(&error)),
                    ),
                    Err(_) => (
                        retry("timeout"),
                        Outcome::Failed(Failure::category(FailureCategory::Timeout)),
                    ),
                };
            let decision = match delivery {
                AttemptOutcome::Delivered { .. } => AnnouncementDecision::Delivered,
                AttemptOutcome::Retry { .. } => AnnouncementDecision::Retry,
                AttemptOutcome::Pause { .. } => AnnouncementDecision::Pause,
            };
            let result = finish_attempt(&store, &item, &delivery, clock()).await;
            match &result {
                Ok(()) => attempt_event(&store, &item, decision, Stage::Deliver, outcome),
                Err(error) => attempt_event(
                    &store,
                    &item,
                    decision,
                    Stage::Commit,
                    store_outcome(Stage::Commit, error),
                ),
            }
            result
        });
    }
}

async fn await_attempts(tasks: &mut Attempts) -> Result<(), StoreError> {
    let mut failure = None;
    while let Some(result) = tasks.join_next().await {
        let result = result.unwrap_or(Err(StoreError::History(
            "announcement delivery task failed",
        )));
        if let Err(error) = result
            && failure.is_none()
        {
            failure = Some(error);
        }
    }
    failure.map_or(Ok(()), Err)
}

/// Send due announcements, preserving revision order within each guild.
/// Only one scheduler may call this function for an application at a time.
///
/// # Errors
/// Returns storage or task failures after awaiting every guild in the current batch.
pub async fn deliver_due(
    store: Arc<Store>,
    http: Arc<serenity::http::Http>,
    clock: Clock,
) -> Result<(), StoreError> {
    let mut tasks = JoinSet::new();
    loop {
        let items = next_due(&store, clock(), 8).await?;
        if items.is_empty() {
            return Ok(());
        }
        spawn_attempts(&mut tasks, items, &store, &http, &clock);
        await_attempts(&mut tasks).await?;
    }
}

fn stopped(shutdown: &watch::Receiver<bool>) -> bool {
    *shutdown.borrow() || shutdown.has_changed().is_err()
}
async fn shutdown_requested(shutdown: &mut watch::Receiver<bool>) {
    while !stopped(shutdown) {
        if shutdown.changed().await.is_err() {
            return;
        }
    }
}
fn worker_failure(store: &Store, stage: Stage, error: &StoreError) {
    store
        .audit()
        .on_event(&AuditEvent::AnnouncementWorkerFailed {
            stage,
            outcome: store_outcome(stage, error),
        });
}

/// Poll immediately, then one second after each completed batch. The gateway owns
/// the single-scheduler invariant. Shutdown stops discovery and grants in-flight
/// attempts up to fifteen seconds to record acknowledgement before aborting them.
#[must_use]
pub fn start_announcement_worker(
    store: Arc<Store>,
    http: Arc<serenity::http::Http>,
    clock: Clock,
    shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    start_announcement_worker_with_deadline(store, http, clock, shutdown, Arc::new(OnceLock::new()))
}

pub(crate) fn start_announcement_worker_with_deadline(
    store: Arc<Store>,
    http: Arc<serenity::http::Http>,
    clock: Clock,
    mut shutdown: watch::Receiver<bool>,
    deadline: ShutdownDeadline,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! { biased;
                () = shutdown_requested(&mut shutdown) => break,
                _ = timer.tick() => {},
            }
            let items = tokio::select! { biased;
                () = shutdown_requested(&mut shutdown) => break,
                result = next_due(&store, clock(), 8) => match result {
                    Ok(items) => items,
                    Err(error) => { worker_failure(&store, Stage::Discover, &error); timer.reset(); continue; }
                },
            };
            if stopped(&shutdown) {
                break;
            }
            spawn_attempts(&mut tasks, items, &store, &http, &clock);
            tokio::select! { biased;
                () = shutdown_requested(&mut shutdown) => break,
                result = await_attempts(&mut tasks) => {
                    if let Err(error) = result { worker_failure(&store, Stage::Deliver, &error); }
                },
            }
            timer.reset();
        }
        let deadline = *deadline.get_or_init(|| Instant::now() + SHUTDOWN_GRACE);
        match tokio::time::timeout_at(deadline, await_attempts(&mut tasks)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => worker_failure(&store, Stage::Deliver, &error),
            Err(_) => {
                store.audit().on_event(&AuditEvent::Lifecycle {
                    kind: LifecycleKind::Shutdown,
                    application_id: Some(store.application_id()),
                    stage: Stage::AnnouncementWorkerShutdown,
                    outcome: Outcome::Failed(Failure::category(FailureCategory::Timeout)),
                });
                tasks.shutdown().await;
            }
        }
    })
}
