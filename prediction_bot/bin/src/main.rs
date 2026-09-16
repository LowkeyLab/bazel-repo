use std::{env, sync::Arc};

use anyhow::{Context, Result, anyhow};
use prediction_bot::{
    discord,
    domain::Policy,
    store::{Store, migrate},
};
use serenity::http::Http;

fn required(name: &str) -> Result<String> {
    let value = env::var(name).with_context(|| format!("{name} is required"))?;
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

async fn application_id(token: &str) -> Result<u64> {
    let app = Http::new(token)
        .get_current_application_info()
        .await
        .map_err(|_| anyhow!("cannot obtain Discord application ID"))?;
    let expected = match env::var("DISCORD_APPLICATION_ID") {
        Ok(value) if !value.trim().is_empty() => {
            let id: u64 = value
                .parse()
                .map_err(|_| anyhow!("DISCORD_APPLICATION_ID must be a positive integer"))?;
            if id == 0 {
                return Err(anyhow!("DISCORD_APPLICATION_ID must be a positive integer"));
            }
            Some(id)
        }
        Ok(_) | Err(env::VarError::NotPresent) => None,
        Err(_) => return Err(anyhow!("DISCORD_APPLICATION_ID is not valid Unicode")),
    };
    discord::verify_application_id(app.id.get(), expected).map_err(|reason| anyhow!("{reason}"))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    let mut args = env::args().skip(1);
    let migrate_only = match (args.next().as_deref(), args.next()) {
        (None, None) => false,
        (Some("--migrate"), None) => true,
        _ => return Err(anyhow!("usage: prediction_bot [--migrate]")),
    };
    if migrate_only {
        let url = required("MIGRATION_DATABASE_URL")?;
        let runtime_password = required("POSTGRES_RUNTIME_PASSWORD")?;
        let pool = sqlx::PgPool::connect(&url)
            .await
            .map_err(|_| anyhow!("migration database connection failed"))?;
        migrate(&pool, &runtime_password).await.map_err(|_| {
            anyhow!("event-store migration failed; check owner and CREATEROLE privileges")
        })?;
        pool.close().await;
        tracing::info!("event-store migration complete");
        return Ok(());
    }
    let token = required("DISCORD_TOKEN")?;
    let url = required("DATABASE_URL")?;
    let defaults = Policy {
        amount: positive("GRANT_AMOUNT", 100)?,
        interval: positive("GRANT_INTERVAL_SECONDS", 86_400)?,
    };
    let app_id = application_id(&token).await?;
    if app_id == 0 {
        return Err(anyhow!("Discord application ID must be positive"));
    }
    let store = Store::connect(&url, app_id, defaults)
        .await
        .map_err(|error| anyhow!("event-store startup failed: {error}"))?;
    tracing::info!(application_id = app_id, "event history reconstructed");
    discord::run(Arc::new(store), token)
        .await
        .map_err(|error| anyhow!("prediction bot stopped: {error}"))?;
    Ok(())
}
