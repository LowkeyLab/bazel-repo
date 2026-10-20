use std::{env, fs};

use discord_server_repo::{Repo, ServerReader};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let schema = env::var("COVERAGE_RESEARCH_SCHEMA")?;
    anyhow::ensure!(
        matches!(schema.as_str(), "application_a" | "application_b"),
        "Invalid research schema"
    );
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&env::var("COVERAGE_RESEARCH_DSN")?)
        .await?;
    sqlx::query("SELECT set_config('search_path', $1, false)")
        .bind(format!("{schema},public"))
        .execute(&pool)
        .await?;
    sqlx::query("SET application_name TO 'coverage_research_rust'")
        .execute(&pool)
        .await?;
    let (before_pid,): (i32,) = sqlx::query_as("SELECT pg_backend_pid()")
        .fetch_one(&pool)
        .await?;
    let count = Repo::new(pool.clone()).count().await?;
    anyhow::ensure!(
        count == 1,
        "The maintained repository returned the wrong count"
    );
    let source_start: i32 = env::var("COVERAGE_SOURCE_START")?.parse()?;
    let source_end: i32 = env::var("COVERAGE_SOURCE_END")?.parse()?;
    let source_line: i32 = env::var("COVERAGE_SOURCE_LINE")?.parse()?;
    let (receipt_pid,): (i32,) = sqlx::query_as("INSERT INTO public.research_receipts(test_name,source_path,source_sha,start_byte,end_byte,start_line,runtime_sql_sha,backend_pid) VALUES($1,$2,$3,$4,$5,$6,$7,pg_backend_pid()) RETURNING backend_pid")
        .bind(&schema).bind("nicknamer2/src/discord_server/repo/lib.rs")
        .bind(env::var("COVERAGE_SOURCE_SHA")?).bind(source_start).bind(source_end).bind(source_line)
        .bind(env::var("COVERAGE_RUNTIME_SHA")?).fetch_one(&pool).await?;
    anyhow::ensure!(
        before_pid == receipt_pid,
        "The maintained operation changed its database session"
    );
    fs::write(
        env::var("COVERAGE_RESEARCH_RECEIPT")?,
        serde_json::to_vec_pretty(&json!({
            "operation": "discord_server_repo::Repo::count", "maintained_function_executed": true,
            "test_name": schema, "count": count, "backend_pid": receipt_pid, "process_id": std::process::id(),
            "source_sha256": env::var("COVERAGE_SOURCE_SHA")?, "source_line": source_line,
            "runtime_sql_sha256": env::var("COVERAGE_RUNTIME_SHA")?
        }))?,
    )?;
    pool.close().await;
    Ok(())
}
