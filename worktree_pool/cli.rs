use std::{
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
};

use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    catalog,
    diagnostics::{CommandObserved, Diagnostics},
    error::PoolError,
    paths::{EncodedPath, Paths},
};

#[derive(Parser)]
#[command(
    name = "worktree-pool",
    version,
    about = "Manage explicitly registered persistent Git worktrees"
)]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    catalog_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Catalog {
        #[command(subcommand)]
        command: CatalogCommand,
    },
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
}
#[derive(Subcommand)]
enum CatalogCommand {
    Init,
    Info,
    Check,
}
#[derive(Subcommand)]
enum EventsCommand {
    List,
}
#[derive(Debug, Serialize)]
pub struct Envelope {
    pub schema_version: u32,
    pub command: String,
    pub outcome: &'static str,
    pub reason_code: &'static str,
    pub context: Value,
    pub data: Value,
    pub warnings: Vec<&'static str>,
}
impl Envelope {
    #[must_use]
    pub fn failure(command: String, error: &PoolError) -> Self {
        Self {
            schema_version: 1,
            command,
            outcome: match error {
                PoolError::Pending => "pending",
                PoolError::CommitUnknown => "unknown",
                _ => "rejected",
            },
            reason_code: error.reason_code(),
            context: json!({}),
            data: json!({"next_action": error.to_string()}),
            warnings: Vec::new(),
        }
    }
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self.outcome {
            "completed" => 0,
            "pending" => 3,
            "unknown" => 4,
            _ => 2,
        }
    }
}
fn handle(cli: &Cli, paths: &Paths) -> Result<Envelope, PoolError> {
    let command = match &cli.command {
        Command::Catalog { command } => match command {
            CatalogCommand::Init => "catalog init",
            CatalogCommand::Info => "catalog info",
            CatalogCommand::Check => "catalog check",
        },
        Command::Events { .. } => "events list",
    };
    let (projection, data) = match &cli.command {
        Command::Catalog {
            command: CatalogCommand::Init,
        } => {
            let p = catalog::initialize(paths)?;
            let data = serde_json::to_value(&p).map_err(|_| PoolError::Corrupt)?;
            (p, data)
        }
        Command::Catalog { .. } => {
            let session = catalog::open(paths)?;
            let data = serde_json::to_value(&session.projection).map_err(|_| PoolError::Corrupt)?;
            (session.projection, data)
        }
        Command::Events { .. } => {
            let session = catalog::open(paths)?;
            let events = session.store().events()?;
            (session.projection, json!({"events": events}))
        }
    };
    Ok(Envelope {
        schema_version: 1,
        command: command.into(),
        outcome: "completed",
        reason_code: "ok",
        context: json!({"catalog_id":projection.catalog_id,"catalog_path":EncodedPath::from_path(&paths.catalog)}),
        data,
        warnings: Vec::new(),
    })
}
/// One result write; reporting failure never repeats a committed command.
pub fn run(args: &[OsString]) -> u8 {
    let requested_json = args.iter().any(|arg| arg == "--json")
        || std::env::var_os("WORKTREE_POOL_JSON").is_some_and(|v| v == "true" || v == "1");
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return 0;
            }
            if requested_json {
                let result = Envelope {
                    schema_version: 1,
                    command: "parse".into(),
                    outcome: "rejected",
                    reason_code: "invalid_arguments",
                    context: json!({}),
                    data: json!({"next_action":"use --help to inspect supported arguments"}),
                    warnings: vec![],
                };
                return emit(&result, true);
            }
            let _ = writeln!(io::stderr(), "worktree-pool: invalid arguments; use --help");
            return 2;
        }
    };
    let name = match &cli.command {
        Command::Catalog { command } => match command {
            CatalogCommand::Init => "catalog init",
            CatalogCommand::Info => "catalog info",
            CatalogCommand::Check => "catalog check",
        },
        Command::Events { .. } => "events list",
    }
    .to_owned();
    let paths = match Paths::load(cli.catalog_dir.clone(), cli.config.clone(), cli.json) {
        Ok(paths) => paths,
        Err(error) => {
            let _ = writeln!(
                io::stderr(),
                "worktree-pool: startup {}",
                error.reason_code()
            );
            return emit(&Envelope::failure(name, &error), requested_json);
        }
    };
    // Listener infrastructure is registered before any catalog work.
    let _ = tracing_subscriber::fmt()
        .with_writer(io::stderr)
        .with_max_level(tracing::Level::WARN)
        .with_ansi(false)
        .try_init();
    let mut diagnostics = Diagnostics::local(paths.json);
    let mut result = match handle(&cli, &paths) {
        Ok(result) => result,
        Err(error) => Envelope::failure(name, &error),
    };
    if matches!(result.outcome, "pending" | "unknown")
        && let Ok(observation) = catalog::inspect_authority(&paths)
    {
        result.context =
            json!({"catalog_id":observation.catalog_id,"catalog_path":observation.catalog_path});
        result.data = serde_json::to_value(observation).unwrap_or_else(|_| json!({}));
        result.data["next_action"] = json!(
            "preserve recorded catalog state; explicit initialization reconciliation is required"
        );
    }
    let fact = CommandObserved {
        command: result.command.clone(),
        outcome: result.outcome.into(),
        reason_code: result.reason_code.into(),
        catalog_id: result
            .context
            .get("catalog_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        revision: result.data.get("revision").and_then(Value::as_u64),
    };
    result.warnings.extend(diagnostics.observe(&fact));
    emit(&result, paths.json)
}
fn emit(result: &Envelope, json_mode: bool) -> u8 {
    let code = result.exit_code();
    let mut stdout = io::stdout().lock();
    let written = if json_mode {
        serde_json::to_writer(&mut stdout, result)
            .map_err(io::Error::other)
            .and_then(|()| writeln!(stdout))
    } else {
        writeln!(
            stdout,
            "{}: {} ({}){}",
            result.command,
            result.outcome,
            result.reason_code,
            result
                .data
                .get("next_action")
                .and_then(Value::as_str)
                .map(|s| format!("; {s}"))
                .unwrap_or_default()
        )
        .and_then(|()| {
            if result.outcome == "completed" {
                writeln!(stdout, "context: {}\ndata: {}", result.context, result.data)
            } else {
                Ok(())
            }
        })
    };
    if written.and_then(|()| stdout.flush()).is_err() {
        let _ = writeln!(
            io::stderr(),
            "worktree-pool: result_output_failed; inspect recorded catalog state before retry"
        );
        return 4;
    }
    code
}
