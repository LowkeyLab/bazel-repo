//! Prepare, inspect, and resume creation of a publication batch.
mod github;
mod graph;
mod plan;
mod publish;
mod recovery;
mod store;
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use plan::{Batch, BatchId, IssueNumber, Plan, TicketKey, TicketStatus};
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
    Export {
        #[arg(long)]
        batch: String,
    },
    Publish {
        #[arg(long)]
        batch: String,
    },
    Recover {
        #[command(subcommand)]
        command: Recovery,
    },
    Status {
        #[arg(long)]
        batch: String,
    },
}
#[derive(Subcommand)]
enum Recovery {
    Reconstruct {
        #[arg(long)]
        batch: String,
        #[arg(long, required_unless_present = "export", conflicts_with = "export")]
        plan: Option<PathBuf>,
        #[arg(long, required_unless_present = "plan")]
        export: Option<PathBuf>,
        #[arg(long, required = true)]
        publisher_stopped: bool,
    },
    AuthorizeRetry {
        #[arg(long)]
        batch: String,
        #[arg(long)]
        ticket: TicketKey,
        #[arg(long, required = true)]
        acknowledge_duplicate_risk: bool,
    },
    Adopt {
        #[arg(long)]
        batch: String,
        #[arg(long)]
        ticket: TicketKey,
        #[arg(long)]
        issue: IssueNumber,
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
            let saved = if store.plan_path().try_exists()? {
                let saved = store.read(&id)?;
                ensure!(
                    saved.plan_fingerprint == prepared.plan_fingerprint,
                    "batch already contains a different plan"
                );
                saved
            } else {
                store.save_new(&prepared)?;
                prepared
            };
            Ok(
                json!({"batch_id": id, "plan_path": store.plan_path(), "status": local_status(&saved)}),
            )
        }
        Action::Export { batch } => {
            let id = BatchId::try_from(batch)?;
            let store = Store::open(&root, &id, false)?;
            Ok(serde_json::to_value(store.read(&id)?)?)
        }
        Action::Publish { batch } => {
            let id = BatchId::try_from(batch)?;
            let store = Store::open(&root, &id, false)?;
            let mut saved = store.read(&id)?;
            let result = publish::run(&mut saved, &store);
            if result.is_err() {
                saved.graph.verified = false;
            }
            let status = if result
                .as_ref()
                .err()
                .is_some_and(|e| e.downcast_ref::<github::Conflict>().is_some())
            {
                "conflict"
            } else if result.is_err()
                && saved.tickets.iter().any(|p| {
                    matches!(
                        p.status,
                        plan::TicketStatus::Creating | plan::TicketStatus::RetryAuthorized
                    )
                })
            {
                "uncertain"
            } else {
                "partial"
            };
            Ok(
                json!({"status":status, "phase":if result.is_ok() {"graph_checked"} else {"stopped"}, "publication_complete":false, "graph":saved.graph, "batch_id":saved.id, "tickets":saved.tickets, "error":result.err().map(|e|format!("{e:#}"))}),
            )
        }
        Action::Recover {
            command:
                Recovery::Reconstruct {
                    batch,
                    plan,
                    export,
                    publisher_stopped,
                },
        } => {
            ensure!(
                publisher_stopped,
                "stop the previous publisher before reconstruction"
            );
            let id = BatchId::try_from(batch)?;
            let mut saved = if let Some(path) = export {
                let exported: Batch = serde_json::from_reader(std::fs::File::open(path)?)?;
                exported.check(&id)?;
                exported
            } else {
                let plan: Plan = serde_json::from_reader(std::fs::File::open(
                    plan.context("original plan is required")?,
                )?)?;
                Batch::prepare(id.clone(), plan)?
            };
            let store = Store::open(&root, &id, true)?;
            ensure!(
                !store.plan_path().try_exists()?,
                "reconstruction cannot replace existing state"
            );
            let conflicts = recovery::reconstruct(&mut saved, &store)?;
            Ok(
                json!({"status":if conflicts {"conflict"} else {local_status(&saved)}, "batch_id":saved.id, "tickets":saved.tickets, "error":if conflicts {Some("multiple identity matches remain unresolved; use recover adopt")} else {None}}),
            )
        }
        Action::Recover {
            command:
                Recovery::AuthorizeRetry {
                    batch,
                    ticket,
                    acknowledge_duplicate_risk,
                },
        } => {
            ensure!(
                acknowledge_duplicate_risk,
                "acknowledge duplicate risk before retry authorization"
            );
            let id = BatchId::try_from(batch)?;
            let store = Store::open(&root, &id, false)?;
            let mut saved = store.read(&id)?;
            recovery::authorize_retry(&mut saved, &ticket, &store)?;
            Ok(json!({"status":local_status(&saved), "batch_id":saved.id, "tickets":saved.tickets}))
        }
        Action::Recover {
            command:
                Recovery::Adopt {
                    batch,
                    ticket,
                    issue,
                },
        } => {
            let id = BatchId::try_from(batch)?;
            let store = Store::open(&root, &id, false)?;
            let mut saved = store.read(&id)?;
            recovery::adopt(&mut saved, &ticket, &issue, &store)?;
            Ok(json!({"status":local_status(&saved), "batch_id":saved.id, "tickets":saved.tickets}))
        }
        Action::Status { batch } => {
            let id = BatchId::try_from(batch)?;
            let store = Store::open(&root, &id, false)?;
            let saved = store.read(&id)?;
            let observed = publish::observe(&saved);
            let graph = if observed.is_ok()
                && saved
                    .tickets
                    .iter()
                    .all(|p| matches!(p.status, TicketStatus::Created(_)))
            {
                Some(graph::observe(&saved))
            } else {
                None
            };
            let status = match &observed {
                Err(error) if error.downcast_ref::<github::Conflict>().is_some() => "conflict",
                Err(_) => "unverified",
                Ok(mappings)
                    if saved.tickets.iter().zip(mappings).any(|(p, m)| {
                        !matches!(p.status, TicketStatus::Pending) && m.is_none()
                    }) =>
                {
                    "uncertain"
                }
                Ok(mappings) if mappings.iter().any(Option::is_some) => "partial",
                Ok(_) => local_status(&saved),
            };
            let mut status = status;
            let (remote_mappings, mut error) = match observed {
                Ok(mappings) => (Some(mappings), None),
                Err(error) => (None, Some(format!("{error:#}"))),
            };
            let graph = match graph {
                Some(Ok(progress)) => Some(progress),
                Some(Err(cause)) => {
                    status = if cause.downcast_ref::<github::Conflict>().is_some() {
                        "conflict"
                    } else {
                        "unverified"
                    };
                    error = Some(format!("{cause:#}"));
                    None
                }
                None => None,
            };
            Ok(
                json!({"status":status, "phase":"graph", "graph":graph, "publication_complete":false, "batch_id":saved.id, "plan_fingerprint":saved.plan_fingerprint, "plan":saved.plan, "tickets":saved.tickets, "remote_mappings":remote_mappings, "error":error}),
            )
        }
    }
}
fn local_status(batch: &Batch) -> &'static str {
    if batch.tickets.iter().any(|p| {
        matches!(
            p.status,
            TicketStatus::Creating | TicketStatus::RetryAuthorized
        )
    }) {
        "uncertain"
    } else if batch
        .tickets
        .iter()
        .all(|p| matches!(p.status, plan::TicketStatus::Pending))
    {
        "prepared"
    } else {
        "partial"
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
