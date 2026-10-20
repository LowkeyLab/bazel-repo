use std::io::{self, Read};
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::Request;
use tower::ServiceExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut payload = String::new();
    io::stdin().read_to_string(&mut payload)?;
    let pool = sqlx::postgres::PgPoolOptions::new().connect_lazy_with(
        sqlx::postgres::PgConnectOptions::new()
            .host("127.0.0.1")
            .port(1),
    );
    let app = server::create_router(
        Arc::new(graphql_schema::create_schema()),
        Arc::new(name_service::Service::new(name_repo::Repo::new(
            pool.clone(),
        ))),
        Arc::new(discord_server_service::Service::new(
            discord_server_repo::Repo::new(pool),
        )),
        Arc::new(auth_claims::AlwaysAllow),
        None,
    );
    let response = app
        .oneshot(
            Request::post("/graphql")
                .header("content-type", "application/json")
                .body(Body::from(payload))?,
        )
        .await?;
    let status = response.status().as_u16();
    let body = to_bytes(response.into_body(), usize::MAX).await?;
    let body: serde_json::Value = serde_json::from_slice(&body)?;
    println!("{}", serde_json::json!({"status": status, "body": body}));
    Ok(())
}
