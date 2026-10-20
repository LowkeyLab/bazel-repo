use std::io::{self, BufRead, Write};
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::Request;
use testcontainers_modules::testcontainers::{ImageExt, runners::AsyncRunner};
use tower::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let container = test_images::postgres()
        .await
        .with_host_config_modifier(|config| {
            config.memory = Some(512 * 1024 * 1024);
            config.nano_cpus = Some(1_000_000_000);
        })
        .start()
        .await?;
    let docker =
        testcontainers_modules::testcontainers::core::client::docker_client_instance().await?;
    let inspection = docker.inspect_container(container.id(), None::<testcontainers_modules::testcontainers::bollard::query_parameters::InspectContainerOptions>).await?;
    let limits = inspection
        .host_config
        .ok_or_else(|| anyhow::anyhow!("Missing container limits"))?;
    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(5432).await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&format!(
            "postgres://postgres:postgres@{host}:{port}/postgres"
        ))
        .await?;
    migrations::run_migrations(&pool).await?;
    let app = server::create_router(
        Arc::new(graphql_schema::create_schema()),
        Arc::new(name_service::Service::new(name_repo::Repo::new(
            pool.clone(),
        ))),
        Arc::new(discord_server_service::Service::new(
            discord_server_repo::Repo::new(pool.clone()),
        )),
        Arc::new(auth_claims::AlwaysAllow),
        None,
    );
    println!(
        "{}",
        serde_json::json!({"ready":true,"container":container.id(),"memory":limits.memory,"nano_cpus":limits.nano_cpus,"path":"Apollo/HttpLink -> Axum oneshot -> Juniper -> services -> PostgreSQL"})
    );
    io::stdout().flush()?;
    for payload in io::stdin().lock().lines() {
        let payload = payload?;
        let response = app
            .clone()
            .oneshot(
                Request::post("/graphql")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer research-fixture")
                    .body(Body::from(payload))?,
            )
            .await?;
        let status = response.status().as_u16();
        let body = to_bytes(response.into_body(), usize::MAX).await?;
        let body: serde_json::Value = serde_json::from_slice(&body)?;
        println!("{}", serde_json::json!({"status":status,"body":body}));
        io::stdout().flush()?;
    }
    let servers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM servers")
        .fetch_one(&pool)
        .await?;
    let names: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM names")
        .fetch_one(&pool)
        .await?;
    eprintln!(
        "{}",
        serde_json::json!({"final_servers":servers,"final_names":names})
    );
    Ok(())
}
