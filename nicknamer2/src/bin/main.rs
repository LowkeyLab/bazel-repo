use std::sync::Arc;

use auth_claims::AuthService;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Rustls crypto provider is already installed"))?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let config = nicknamer2_config::Config::from_env()?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.db_url)
        .await?;

    migrations::run_migrations(&pool).await?;
    tracing::info!("Database migrations applied successfully");

    let name_repo = name_repo::Repo::new(pool.clone());
    let name_service = Arc::new(name_service::Service::new(name_repo));

    let server_repo = discord_server_repo::Repo::new(pool);
    let server_service = Arc::new(discord_server_service::Service::new(server_repo));

    let jwks_validator: Arc<dyn AuthService> = match &config.casdoor_client_id {
        Some(client_id) => {
            let v = auth::JwksValidator::new(
                &config.casdoor_issuer_url,
                client_id,
                config.casdoor_jwks_url.as_deref(),
            )
            .await?;
            let jwks_source = config
                .casdoor_jwks_url
                .as_deref()
                .unwrap_or(&config.casdoor_issuer_url);
            tracing::info!("JWKS keys loaded from {}", jwks_source);
            Arc::new(v)
        }
        None => {
            tracing::warn!(
                "CASDOOR_CLIENT_ID not set — mutations will reject all requests as unauthenticated"
            );
            Arc::new(auth_claims::AlwaysDeny)
        }
    };

    let schema = Arc::new(graphql_schema::create_schema());

    let app = server::create_router(
        schema,
        name_service,
        server_service,
        jwks_validator,
        config.static_dir.as_deref(),
    );

    let address = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!("GraphQL server running on http://{}", address);
    tracing::info!("GraphiQL IDE available at http://{}/graphiql", address);
    if let Some(ref dir) = config.static_dir {
        tracing::info!("Serving frontend from {}", dir);
    }

    axum::serve(listener, app).await?;
    Ok(())
}
