use std::{env, sync::Arc};

use anyhow::{Result, anyhow};
use prediction_bot::{
    audit::{
        AuditEvent, Failure, FailureCategory, LifecycleKind, Outcome, SharedAudit, Stage,
        discord_failure, logging_listener, store_outcome,
    },
    discord,
    domain::Policy,
    store::{Store, StoreError, migrate},
};
use serenity::http::Http;

fn required(name: &str) -> Result<String> {
    let value = match env::var(name) {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => return Err(anyhow!("{name} is required")),
        Err(env::VarError::NotUnicode(_)) => {
            return Err(anyhow!("{name} is not valid Unicode"));
        }
    };
    if value.trim().is_empty() {
        return Err(anyhow!("{name} must not be empty"));
    }
    Ok(value)
}

fn positive(name: &str, default: i64) -> Result<i64> {
    match env::var(name) {
        Ok(value) => {
            let parsed: i64 = value
                .parse()
                .map_err(|_| anyhow!("{name} must be a positive integer"))?;
            if parsed <= 0 {
                return Err(anyhow!("{name} must be a positive integer"));
            }
            Ok(parsed)
        }
        Err(env::VarError::NotPresent) => Ok(default),
        Err(_) => Err(anyhow!("{name} is not valid Unicode")),
    }
}

struct BootstrapFailure {
    error: anyhow::Error,
    outcome: Outcome,
}

fn failure(category: FailureCategory) -> Outcome {
    Outcome::Failed(Failure {
        category,
        sqlstate: None,
        http_status: None,
        discord_code: None,
    })
}

fn lifecycle(
    audit: &SharedAudit,
    kind: LifecycleKind,
    application_id: Option<u64>,
    stage: Stage,
    outcome: Outcome,
) {
    audit.on_event(&AuditEvent::Lifecycle {
        kind,
        application_id,
        outcome,
        stage,
    });
}

fn configured<T>(audit: &SharedAudit, kind: LifecycleKind, result: Result<T>) -> Result<T> {
    result.map_err(|error| {
        lifecycle(
            audit,
            kind,
            None,
            Stage::Validate,
            failure(FailureCategory::Configuration),
        );
        error
    })
}

async fn application_id(token: &str) -> Result<u64, BootstrapFailure> {
    let app = Http::new(token)
        .get_current_application_info()
        .await
        .map_err(|error| BootstrapFailure {
            outcome: Outcome::Failed(discord_failure(&error)),
            error: anyhow!("cannot obtain Discord application ID"),
        })?;
    let expected = match env::var("DISCORD_APPLICATION_ID") {
        Ok(value) if !value.trim().is_empty() => {
            let id: u64 = value.parse().map_err(|_| BootstrapFailure {
                outcome: failure(FailureCategory::Configuration),
                error: anyhow!("DISCORD_APPLICATION_ID must be a positive integer"),
            })?;
            if id == 0 {
                return Err(BootstrapFailure {
                    outcome: failure(FailureCategory::Configuration),
                    error: anyhow!("DISCORD_APPLICATION_ID must be a positive integer"),
                });
            }
            Some(id)
        }
        Ok(_) | Err(env::VarError::NotPresent) => None,
        Err(env::VarError::NotUnicode(_)) => {
            return Err(BootstrapFailure {
                outcome: failure(FailureCategory::Configuration),
                error: anyhow!("DISCORD_APPLICATION_ID is not valid Unicode"),
            });
        }
    };
    discord::verify_application_id(app.id.get(), expected).map_err(|reason| BootstrapFailure {
        outcome: failure(FailureCategory::Configuration),
        error: anyhow!("{reason}"),
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_max_level(tracing::Level::INFO)
        .init();
    let audit = logging_listener();
    let mut args = env::args().skip(1);
    let migrate_only = configured(
        &audit,
        LifecycleKind::Startup,
        match (args.next().as_deref(), args.next()) {
            (None, None) => Ok(false),
            (Some("--migrate"), None) => Ok(true),
            _ => Err(anyhow!("usage: prediction_bot [--migrate]")),
        },
    )?;
    if migrate_only {
        let url = configured(
            &audit,
            LifecycleKind::Migration,
            required("MIGRATION_DATABASE_URL"),
        )?;
        let runtime_password = configured(
            &audit,
            LifecycleKind::Migration,
            required("POSTGRES_RUNTIME_PASSWORD"),
        )?;
        let pool = match sqlx::PgPool::connect(&url).await {
            Ok(pool) => pool,
            Err(error) => {
                lifecycle(
                    &audit,
                    LifecycleKind::Migration,
                    None,
                    Stage::Migrate,
                    store_outcome(Stage::Migrate, &StoreError::Database(error)),
                );
                return Err(anyhow!("migration database connection failed"));
            }
        };
        if let Err(error) = migrate(&pool, &runtime_password).await {
            lifecycle(
                &audit,
                LifecycleKind::Migration,
                None,
                Stage::Migrate,
                store_outcome(Stage::Migrate, &error),
            );
            return Err(anyhow!(
                "event-store migration failed; check owner and CREATEROLE privileges"
            ));
        }
        pool.close().await;
        lifecycle(
            &audit,
            LifecycleKind::Migration,
            None,
            Stage::Migrate,
            Outcome::Succeeded,
        );
        return Ok(());
    }
    let token = configured(&audit, LifecycleKind::Startup, required("DISCORD_TOKEN"))?;
    let url = configured(&audit, LifecycleKind::Startup, required("DATABASE_URL"))?;
    let defaults = Policy {
        amount: configured(
            &audit,
            LifecycleKind::Startup,
            positive("GRANT_AMOUNT", 100),
        )?,
        interval: configured(
            &audit,
            LifecycleKind::Startup,
            positive("GRANT_INTERVAL_SECONDS", 86_400),
        )?,
    };
    let app_id = match application_id(&token).await {
        Ok(app_id) => app_id,
        Err(error) => {
            lifecycle(
                &audit,
                LifecycleKind::Startup,
                None,
                Stage::Startup,
                error.outcome,
            );
            return Err(error.error);
        }
    };
    if app_id == 0 {
        lifecycle(
            &audit,
            LifecycleKind::Startup,
            Some(app_id),
            Stage::Validate,
            failure(FailureCategory::Configuration),
        );
        return Err(anyhow!("Discord application ID must be positive"));
    }
    let store = match Store::connect_with_audit(&url, app_id, defaults, Arc::clone(&audit)).await {
        Ok(store) => store,
        Err(error) => {
            lifecycle(
                &audit,
                LifecycleKind::Startup,
                Some(app_id),
                Stage::Startup,
                store_outcome(Stage::Startup, &error),
            );
            return Err(anyhow!("event-store startup failed: {error}"));
        }
    };
    lifecycle(
        &audit,
        LifecycleKind::Startup,
        Some(app_id),
        Stage::Startup,
        Outcome::Succeeded,
    );
    discord::run(Arc::new(store), token)
        .await
        .map_err(|error| anyhow!("prediction bot stopped: {error}"))?;
    Ok(())
}
