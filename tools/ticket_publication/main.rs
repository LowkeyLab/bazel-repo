//! Prepare, inspect, and resume creation of a publication batch.
mod github;
mod plan;
mod publish;
mod store;
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use plan::{Batch, BatchId, Plan};
use serde_json::json;
use std::path::PathBuf;
use store::Store;

#[derive(Parser)]
#[command(about = "Prepare batches and resume ticket creation")]
struct Cli {
    #[arg(long, global = true)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Prepare {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        batch: Option<String>,
    },
    Publish {
        #[arg(long)]
        batch: String,
    },
    Status {
        #[arg(long)]
        batch: String,
    },
}
fn state_directory(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    if let Some(path) = std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        let path = PathBuf::from(path);
        ensure!(path.is_absolute(), "XDG_STATE_HOME must be absolute");
        return Ok(path.join("ticket-publication"));
    }
    let home = std::env::var_os("HOME").context("HOME is missing; specify --state-dir")?;
    ensure!(PathBuf::from(&home).is_absolute(), "HOME must be absolute");
    Ok(PathBuf::from(home).join(".local/state/ticket-publication"))
}
fn execute(cli: Cli) -> Result<serde_json::Value> {
    let root = state_directory(cli.state_dir)?;
    ensure!(
        root.to_str().is_some(),
        "state directory must use UTF-8 for JSON output"
    );
    match cli.command {
        Action::Prepare { plan, batch } => {
            let plan: Plan = serde_json::from_reader(std::fs::File::open(plan)?)?;
            let id = batch
                .map(BatchId::try_from)
                .transpose()?
                .unwrap_or_else(BatchId::new);
            let prepared = Batch::prepare(id.clone(), plan)?;
            let store = Store::open(&root, &id, true)?;
            if store.plan_path().try_exists()? {
                let saved = store.read(&id)?;
                ensure!(
                    saved.plan_fingerprint == prepared.plan_fingerprint,
                    "batch already contains a different plan"
                );
            } else {
                store.save_new(&prepared)?;
            }
            Ok(json!({"batch_id": id, "plan_path": store.plan_path(), "status": "prepared"}))
        }
        Action::Publish { batch } => {
            let id = BatchId::try_from(batch)?;
            let store = Store::open(&root, &id, false)?;
            let mut saved = store.read(&id)?;
            let result = publish::run(&mut saved, &store);
            let status = if result
                .as_ref()
                .err()
                .is_some_and(|e| e.downcast_ref::<publish::Conflict>().is_some())
            {
                "conflict"
            } else if result.is_err()
                && saved
                    .tickets
                    .iter()
                    .any(|p| matches!(p.status, plan::TicketStatus::Creating))
            {
                "uncertain"
            } else {
                "partial"
            };
            Ok(
                json!({"status":status, "phase":if result.is_ok() {"issues_created"} else {"stopped"}, "batch_id":saved.id, "tickets":saved.tickets, "error":result.err().map(|e|format!("{e:#}"))}),
            )
        }
        Action::Status { batch } => {
            let id = BatchId::try_from(batch)?;
            let store = Store::open(&root, &id, false)?;
            let saved = store.read(&id)?;
            Ok(
                json!({"status":if saved.tickets.iter().all(|p| matches!(p.status, plan::TicketStatus::Pending)) {"prepared"} else {"partial"}, "batch_id":saved.id, "plan_fingerprint":saved.plan_fingerprint, "plan":saved.plan, "tickets":saved.tickets}),
            )
        }
    }
}
fn main() -> Result<std::process::ExitCode> {
    let result = execute(Cli::parse())?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(if result.get("error").is_some_and(|e| !e.is_null()) {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    })
}
