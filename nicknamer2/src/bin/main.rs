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
    let service = Arc::new(name_service::Service::new(repo));

    let schema = Arc::new(graphql_schema::create_schema());
    let context = Arc::new(graphql_context::Context {
        name_service: service,
    });

    let app = server::create_router(schema, context);

    let address = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!("GraphQL server running on http://{}", address);
    tracing::info!("GraphiQL IDE available at http://{}/graphiql", address);

    axum::serve(listener, app).await?;
    Ok(())
}
