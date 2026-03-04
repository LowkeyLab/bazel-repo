use std::sync::Arc;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let repo = name_repo::Repo::new(pool);
    let name_service = Arc::new(name_service::Service::new(repo));

    let jwks_validator = Arc::new(
        auth::JwksValidator::new(&config.casdoor_issuer_url, &config.casdoor_client_id).await?,
    );
    tracing::info!("JWKS keys loaded from {}", config.casdoor_issuer_url);

    let schema = Arc::new(graphql_schema::create_schema());

    let app = server::create_router(schema, name_service, jwks_validator);

    let address = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!("GraphQL server running on http://{}", address);
    tracing::info!("GraphiQL IDE available at http://{}/graphiql", address);

    axum::serve(listener, app).await?;
    Ok(())
}
