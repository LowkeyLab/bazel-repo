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
    #[arg(long, global = true)]
    repo: Option<OsString>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Recover {
        #[command(subcommand)]
        command: RecoveryCommand,
    },
    Pool {
        #[command(subcommand)]
        command: PoolCommand,
    },
    Release {
        assignment_handle: String,
    },
    Acquire {
        reference: Option<OsString>,
    },
    Assignment {
        #[command(subcommand)]
        command: RecordedCommand,
    },
    Operation {
        #[command(subcommand)]
        command: RecordedCommand,
    },
    Repo {
        #[command(subcommand)]
        command: RepoCommand,
    },
    Worktree {
        #[command(subcommand)]
        command: WorktreeCommand,
    },
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
enum RecoveryCommand {
    Apply {
        #[arg(long, conflicts_with_all=["assignment","worktree","abandon"])]
        catalog: bool,
        #[arg(long, conflicts_with_all=["operation","worktree"])]
        assignment: Option<String>,
        #[arg(long, conflicts_with_all=["operation","assignment"])]
        worktree: Option<OsString>,
        #[arg(long, conflicts_with = "abandon")]
        operation: Option<String>,
        #[arg(long)]
        abandon: bool,
    },
    Preview {
        #[arg(long, conflicts_with_all=["assignment","worktree"])]
        catalog: bool,
        #[arg(long, conflicts_with_all=["operation","worktree"])]
        assignment: Option<String>,
        #[arg(long, conflicts_with_all=["operation","assignment"])]
        worktree: Option<OsString>,
        #[arg(long)]
        operation: Option<String>,
    },
}
#[derive(Subcommand)]
enum PoolCommand {
    Configure {
        #[arg(long)]
        max_worktrees: u32,
    },
}
#[derive(Subcommand)]
enum RecordedCommand {
    List,
    Inspect { selector: String },
}
#[derive(Subcommand)]
enum RepoCommand {
    Register { path: PathBuf },
    List,
    Inspect { selector: Option<OsString> },
    Refresh { selector: Option<OsString> },
}
#[derive(Subcommand)]
enum WorktreeCommand {
    Register { path: PathBuf },
    List,
    Inspect { selector: OsString },
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
                PoolError::Pending | PoolError::OperationPending => "pending",
                PoolError::CommitUnknown => "unknown",
                _ => "rejected",
            },
            reason_code: error.reason_code(),
            context: match error {
                PoolError::PoolExhausted { repository_id, .. } => {
                    json!({"repository_id":repository_id})
                }
                _ => json!({}),
            },
            data: match error {
                PoolError::PoolExhausted {
                    maximum,
                    registered_count,
                    assigned_count,
                    ..
                } => {
                    json!({"maximum":maximum,"registered_count":registered_count,"assigned_count":assigned_count,"next_action":error.to_string()})
                }
                _ => json!({"next_action": error.to_string()}),
            },
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
const fn command_name(cli: &Cli) -> &'static str {
    match &cli.command {
        Command::Recover { command } => match command {
            RecoveryCommand::Preview { .. } => "recover preview",
            RecoveryCommand::Apply { .. } => "recover apply",
        },
        Command::Pool { .. } => "pool configure",
        Command::Catalog { command } => match command {
            CatalogCommand::Init => "catalog init",
            CatalogCommand::Info => "catalog info",
            CatalogCommand::Check => "catalog check",
        },
        Command::Events { .. } => "events list",
        Command::Release { .. } => "release",
        Command::Acquire { .. } => "acquire",
        Command::Assignment { command } => match command {
            RecordedCommand::List => "assignment list",
            RecordedCommand::Inspect { .. } => "assignment inspect",
        },
        Command::Operation { command } => match command {
            RecordedCommand::List => "operation list",
            RecordedCommand::Inspect { .. } => "operation inspect",
        },
        Command::Repo { command } => match command {
            RepoCommand::Register { .. } => "repo register",
            RepoCommand::List => "repo list",
            RepoCommand::Inspect { .. } => "repo inspect",
            RepoCommand::Refresh { .. } => "repo refresh",
        },
        Command::Worktree { command } => match command {
            WorktreeCommand::Register { .. } => "worktree register",
            WorktreeCommand::List => "worktree list",
            WorktreeCommand::Inspect { .. } => "worktree inspect",
        },
    }
}
fn handle(cli: &Cli, paths: &Paths) -> Result<Envelope, PoolError> {
    let command = command_name(cli);
    if let Command::Recover { command: recovery } = &cli.command {
        return handle_recovery(cli, paths, recovery);
    }

    if let Command::Operation {
        command: RecordedCommand::Inspect { selector },
    } = &cli.command
        && let Some(envelope) = crate::catalog_recovery_cli::inspect_known_operation(
            paths,
            cli.repo.as_deref(),
            selector,
        )
    {
        return Ok(envelope);
    }
    if matches!(
        &cli.command,
        Command::Operation {
            command: RecordedCommand::List
        }
    ) && cli.repo.is_none()
    {
        let lifecycle = crate::catalog_recovery_cli::lifecycle_operations(paths)?;
        match resources(cli, paths, command) {
            Ok(mut envelope) => {
                envelope.data["operations"]
                    .as_array_mut()
                    .ok_or(PoolError::Corrupt)?
                    .extend(lifecycle);
                envelope.data["repository_operations_available"] = json!(true);
                return Ok(envelope);
            }
            Err(error @ (PoolError::Pending | PoolError::Missing)) => {
                let mut envelope = Envelope::failure(command.into(), &error);
                envelope.data["operations"] = json!(lifecycle);
                envelope.data["repository_operations_available"] = json!(false);
                return Ok(envelope);
            }
            Err(error) => return Err(error),
        }
    }
    if matches!(
        cli.command,
        Command::Pool { .. }
            | Command::Repo { .. }
            | Command::Worktree { .. }
            | Command::Release { .. }
            | Command::Acquire { .. }
            | Command::Assignment { .. }
            | Command::Operation { .. }
    ) {
        return resources(cli, paths, command);
    }
    let (projection, data) = match &cli.command {
        Command::Catalog {
            command: CatalogCommand::Init,
        } => {
            let p = catalog::initialize(paths)?;
            let data = serde_json::to_value(&p).map_err(|_| PoolError::Corrupt)?;
            (p, data)
        }
        Command::Catalog { .. } => {
            let projection = catalog::inspect_projection(paths)?;
            let data = serde_json::to_value(&projection).map_err(|_| PoolError::Corrupt)?;
            (projection, data)
        }
        Command::Recover { .. }
        | Command::Pool { .. }
        | Command::Repo { .. }
        | Command::Worktree { .. }
        | Command::Release { .. }
        | Command::Acquire { .. }
        | Command::Assignment { .. }
        | Command::Operation { .. } => unreachable!(),
        Command::Events { .. } => {
            let (projection, events) = catalog::inspect_events(paths)?;
            (projection, json!({"events": events}))
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
fn resources(cli: &Cli, paths: &Paths, command: &str) -> Result<Envelope, PoolError> {
    use crate::workflows;
    if let Command::Pool {
        command: PoolCommand::Configure { max_worktrees },
    } = &cli.command
    {
        return capacity_command(cli, paths, *max_worktrees);
    }
    if let Command::Release { assignment_handle } = &cli.command {
        return release_command(cli, paths, assignment_handle);
    }
    let mut context = json!({});
    let mut outcome = "completed";
    let mut reason_code = "ok";
    let (state, mut data) = match &cli.command {
        Command::Acquire { reference } => {
            let (state, assignment) = crate::acquisition_workflow::acquire(
                paths,
                cli.repo.as_deref(),
                reference.as_deref(),
            )?;
            context["repository_id"] = json!(assignment.repository_id);
            context["worktree_id"] = json!(assignment.worktree_id);
            context["assignment_handle"] = json!(assignment.assignment_handle);
            context["operation_id"] = json!(assignment.operation_id);
            if assignment.state != crate::acquisition::AssignmentState::Active {
                outcome = "pending";
                reason_code = "operation_pending";
            }
            (
                state,
                json!({"assignment":assignment,"next_action":if outcome=="completed" {"use the assigned checkout; retain the assignment handle"} else {"inspect the recorded operation before explicit reconciliation"}}),
            )
        }
        Command::Repo {
            command: RepoCommand::Refresh { selector },
        } => {
            if selector.is_some() && cli.repo.is_some() {
                let state = { catalog::inspect_projection(paths)? };
                if workflows::repository(&state, selector.as_deref())?.repository_id
                    != workflows::repository(&state, cli.repo.as_deref())?.repository_id
                {
                    return Err(PoolError::Selectors);
                }
            }
            let (state, operation) =
                workflows::refresh(paths, selector.as_deref().or(cli.repo.as_deref()))?;
            context["repository_id"] = json!(operation.repository_id);
            context["operation_id"] = json!(operation.operation_id);
            if operation.state != crate::management::RefreshState::Completed {
                outcome = "pending";
                reason_code = if operation.last_checkpoint == "fetch_failed" {
                    "refresh_failed"
                } else {
                    "operation_pending"
                };
            }
            (
                state,
                json!({"operation":operation,"next_action":if outcome == "completed" { "inspect the refreshed repository" } else { "inspect the recorded operation; explicit reconciliation is required before retry" }}),
            )
        }
        Command::Repo {
            command: RepoCommand::Register { path },
        } => {
            let (state, repository) =
                workflows::register_repository(paths, path, cli.repo.as_deref())?;
            context["repository_id"] = json!(repository.repository_id);
            (state, json!({"repository": repository}))
        }
        Command::Worktree {
            command: WorktreeCommand::Register { path },
        } => {
            let (state, worktree) = workflows::register_worktree(paths, path, cli.repo.as_deref())?;
            context["repository_id"] = json!(worktree.repository_id);
            context["worktree_id"] = json!(worktree.worktree_id);
            let view = worktree_view(&worktree, &state);
            (state, json!({"worktree": view}))
        }
        _ => {
            let state = catalog::inspect_projection(paths)?;
            let data = resource_query(cli, &state, &mut context)?;
            (state, data)
        }
    };
    data["revision"] = json!(state.revision);
    context["catalog_id"] = json!(state.catalog_id);
    context["catalog_path"] = json!(EncodedPath::from_path(&paths.catalog));
    Ok(Envelope {
        schema_version: 1,
        command: command.into(),
        outcome,
        reason_code,
        context,
        data,
        warnings: Vec::new(),
    })
}
fn resource_query(
    cli: &Cli,
    state: &crate::domain::CatalogProjection,
    context: &mut Value,
) -> Result<Value, PoolError> {
    use crate::{management::WorktreeId, workflows};
    let data = match &cli.command {
        Command::Assignment { command } => assignment_query(cli, state, command, context)?,

        Command::Operation { command } => operation_query(cli, state, command, context)?,

        Command::Repo {
            command: RepoCommand::List,
        } => {
            if cli.repo.is_some() {
                json!({"repositories": [workflows::repository(state, cli.repo.as_deref())?]})
            } else {
                json!({"repositories": state.repositories})
            }
        }
        Command::Repo {
            command: RepoCommand::Inspect { selector },
        } => {
            let repository =
                workflows::repository(state, selector.as_deref().or(cli.repo.as_deref()))?;
            if cli.repo.is_some()
                && workflows::repository(state, cli.repo.as_deref())?.repository_id
                    != repository.repository_id
            {
                return Err(PoolError::Selectors);
            }
            context["repository_id"] = json!(repository.repository_id);
            json!({"repository":repository,"registered_count":state.worktrees.iter().filter(|w| w.repository_id == repository.repository_id).count(), "operations":state.operations.iter().filter(|o| o.repository_id == repository.repository_id).collect::<Vec<_>>(), "creations":state.creations.iter().filter(|o| o.repository_id == repository.repository_id).collect::<Vec<_>>()})
        }
        Command::Worktree {
            command: WorktreeCommand::List,
        } => {
            let selected = cli
                .repo
                .as_deref()
                .map(|s| workflows::repository(state, Some(s)))
                .transpose()?;
            let worktrees: Vec<_> = state
                .worktrees
                .iter()
                .filter(|w| {
                    selected
                        .as_ref()
                        .is_none_or(|r| r.repository_id == w.repository_id)
                })
                .map(|w| worktree_view(w, state))
                .collect();
            json!({"worktrees":worktrees})
        }
        Command::Worktree {
            command: WorktreeCommand::Inspect { selector },
        } => {
            let id = selector.to_str().and_then(|s| s.parse::<WorktreeId>().ok());
            let path = if id.is_none() {
                let p = PathBuf::from(selector);
                Some(if p.is_absolute() {
                    p
                } else {
                    std::env::current_dir()?.join(p)
                })
            } else {
                None
            };
            let worktree = state
                .worktrees
                .iter()
                .find(|w| {
                    Some(w.worktree_id) == id
                        || path
                            .as_ref()
                            .is_some_and(|p| w.path.bytes == EncodedPath::from_path(p).bytes)
                })
                .ok_or(PoolError::Unregistered)?;
            if cli.repo.is_some()
                && workflows::repository(state, cli.repo.as_deref())?.repository_id
                    != worktree.repository_id
            {
                return Err(PoolError::Selectors);
            }
            context["repository_id"] = json!(worktree.repository_id);
            context["worktree_id"] = json!(worktree.worktree_id);
            json!({"worktree":worktree_view(worktree, state)})
        }
        _ => unreachable!(),
    };
    Ok(data)
}
fn worktree_view(
    worktree: &crate::management::Worktree,
    state: &crate::domain::CatalogProjection,
) -> Value {
    let registration_state = registration_state(worktree, state);
    let mut value = serde_json::to_value(worktree).unwrap_or_else(|_| json!({}));
    value["registration_state"] = json!(registration_state);
    let assignment = state.assignments.iter().find(|a| {
        a.worktree_id == worktree.worktree_id
            && a.state != crate::acquisition::AssignmentState::Released
    });
    value["ownership"] = json!(assignment.map_or("unassigned", |a| {
        if a.state == crate::acquisition::AssignmentState::Active {
            "assigned"
        } else {
            "preparing"
        }
    }));
    value["assignment_handle"] = json!(assignment.map(|a| a.assignment_handle));
    let withheld = state
        .withheld_worktrees
        .iter()
        .find(|w| w.worktree_id == worktree.worktree_id);
    let mut pending: Vec<Value> = state
        .operations
        .iter()
        .filter(|o| o.repository_id == worktree.repository_id && o.state.is_pending())
        .map(|o| json!(o))
        .collect();
    pending.extend(
        state
            .acquisitions
            .iter()
            .filter(|o| o.worktree_id == worktree.worktree_id && o.state.is_pending())
            .map(|o| json!(o)),
    );
    pending.extend(
        state
            .releases
            .iter()
            .filter(|o| o.worktree_id == worktree.worktree_id && o.state.is_pending())
            .map(|o| json!(o)),
    );
    value["availability"] = json!(if registration_state == "registered"
        && pending.is_empty()
        && assignment.is_none()
        && withheld.is_none()
    {
        "unverified"
    } else {
        "withheld"
    });
    value["withheld_reason"] = if registration_state != "registered" {
        json!(registration_state)
    } else if let Some(withheld) = withheld {
        json!(withheld.reason)
    } else if assignment.is_some() {
        json!("owned")
    } else if pending.is_empty() {
        Value::Null
    } else {
        json!("operation_pending")
    };
    let creation = state
        .creations
        .iter()
        .find(|o| o.worktree_id == worktree.worktree_id);
    value["creation"] = json!(creation);
    if let Some(creation) = creation.filter(|o| o.state.is_pending()) {
        pending.push(json!(creation));
    }
    value["pending_work"] = json!(pending);
    value
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
    let name = command_name(&cli).to_owned();
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
    if matches!(cli.command, Command::Catalog { .. })
        && matches!(result.outcome, "pending" | "unknown")
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

fn assignment_query(
    cli: &Cli,
    state: &crate::domain::CatalogProjection,
    command: &RecordedCommand,
    context: &mut Value,
) -> Result<Value, PoolError> {
    Ok({
        let selected = cli
            .repo
            .as_deref()
            .map(|s| crate::workflows::repository(state, Some(s)))
            .transpose()?;
        let assignments: Vec<_> = state
            .assignments
            .iter()
            .filter(|a| {
                selected
                    .as_ref()
                    .is_none_or(|r| r.repository_id == a.repository_id)
            })
            .collect();
        match command {
            RecordedCommand::List => json!({"assignments":assignments}),
            RecordedCommand::Inspect { selector } => {
                let a = assignments
                    .into_iter()
                    .find(|a| a.assignment_handle.to_string() == *selector)
                    .ok_or(PoolError::Unregistered)?;
                context["assignment_handle"] = json!(a.assignment_handle);
                context["repository_id"] = json!(a.repository_id);
                context["worktree_id"] = json!(a.worktree_id);
                context["operation_id"] = json!(a.operation_id);
                json!({"assignment":a})
            }
        }
    })
}
fn operation_query(
    cli: &Cli,
    state: &crate::domain::CatalogProjection,
    command: &RecordedCommand,
    context: &mut Value,
) -> Result<Value, PoolError> {
    Ok({
        let selected = cli
            .repo
            .as_deref()
            .map(|s| crate::workflows::repository(state, Some(s)))
            .transpose()?;
        let mut operations: Vec<Value> = state
            .operations
            .iter()
            .filter(|o| {
                selected
                    .as_ref()
                    .is_none_or(|r| r.repository_id == o.repository_id)
            })
            .map(|o| json!(o))
            .collect();
        operations.extend(
            state
                .acquisitions
                .iter()
                .filter(|o| {
                    selected
                        .as_ref()
                        .is_none_or(|r| r.repository_id == o.repository_id)
                })
                .map(|o| json!(o)),
        );
        operations.extend(
            state
                .releases
                .iter()
                .filter(|o| {
                    selected
                        .as_ref()
                        .is_none_or(|r| r.repository_id == o.repository_id)
                })
                .map(|o| json!(o)),
        );
        operations.extend(
            state
                .creations
                .iter()
                .filter(|o| {
                    selected
                        .as_ref()
                        .is_none_or(|r| r.repository_id == o.repository_id)
                })
                .map(|o| json!(o)),
        );
        operations.extend(
            state
                .repository_recoveries
                .iter()
                .filter(|o| {
                    selected
                        .as_ref()
                        .is_none_or(|r| r.repository_id == o.repository_id)
                })
                .map(|o| json!(o)),
        );
        operations.extend(
            state
                .recoveries
                .iter()
                .filter(|o| {
                    selected
                        .as_ref()
                        .is_none_or(|r| r.repository_id == o.repository_id)
                })
                .map(|o| json!(o)),
        );
        match command {
            RecordedCommand::List => json!({"operations":operations}),
            RecordedCommand::Inspect { selector } => {
                let o = operations
                    .into_iter()
                    .find(|o| o["operation_id"].as_str() == Some(selector))
                    .ok_or(PoolError::Unregistered)?;
                context["operation_id"] = o["operation_id"].clone();
                context["repository_id"] = o["repository_id"].clone();
                json!({"operation":o})
            }
        }
    })
}

fn release_command(
    cli: &Cli,
    paths: &Paths,
    assignment_handle: &str,
) -> Result<Envelope, PoolError> {
    let handle = assignment_handle
        .parse()
        .map_err(|_| PoolError::UnknownAssignment)?;
    let (state, assignment, operation, already) =
        crate::release_workflow::release(paths, cli.repo.as_deref(), handle)?;
    let (outcome, reason_code) = if already {
        ("completed", "already_released")
    } else if operation.as_ref().is_some_and(|o| o.state.is_pending()) {
        ("pending", "operation_pending")
    } else {
        ("completed", "ok")
    };
    let recovery_operation = state.recoveries.iter().rev().find(|o| {
        o.assignment_handle == Some(assignment.assignment_handle)
            && o.disposition == crate::recovery::RecoveryDisposition::Released
            && o.state == crate::recovery::RecoveryState::Completed
    });
    let operation_id = operation
        .as_ref()
        .map(|o| o.operation_id)
        .or_else(|| recovery_operation.map(|o| o.operation_id));
    Ok(Envelope {
        schema_version: 1,
        command: "release".into(),
        outcome,
        reason_code,
        context: json!({"catalog_id":state.catalog_id,"catalog_path":EncodedPath::from_path(&paths.catalog),"repository_id":assignment.repository_id,"worktree_id":assignment.worktree_id,"assignment_handle":assignment.assignment_handle,"operation_id":operation_id}),
        data: json!({"assignment":assignment,"operation":operation,"recovery_operation":recovery_operation,"already_released":already,"current_availability":null,"revision":state.revision,"next_action":if outcome=="pending" {"inspect the recorded release; explicit reconciliation is required"}else{"stop using this assignment handle; inspect the worktree before another acquisition"}}),
        warnings: Vec::new(),
    })
}

fn capacity_command(cli: &Cli, paths: &Paths, maximum: u32) -> Result<Envelope, PoolError> {
    let (state, repository) =
        crate::workflows::configure_capacity(paths, cli.repo.as_deref(), maximum)?;
    let count = state
        .worktrees
        .iter()
        .filter(|w| w.repository_id == repository.repository_id)
        .count();
    Ok(Envelope {
        schema_version: 1,
        command: "pool configure".into(),
        outcome: "completed",
        reason_code: "ok",
        context: json!({"catalog_id":state.catalog_id,"catalog_path":EncodedPath::from_path(&paths.catalog),"repository_id":repository.repository_id}),
        data: json!({"repository":repository,"registered_count":count,"revision":state.revision}),
        warnings: Vec::new(),
    })
}

fn registration_state(
    worktree: &crate::management::Worktree,
    state: &crate::domain::CatalogProjection,
) -> &'static str {
    let path = worktree.path.to_path();
    match path.and_then(|p| {
        std::fs::symlink_metadata(&p)
            .map(|_| p)
            .map_err(PoolError::from)
    }) {
        Err(PoolError::Io(ref e)) if e.kind() == io::ErrorKind::NotFound => "missing",
        Err(_) => "unreadable",
        Ok(path) => match crate::git::checkout(&path) {
            Ok(observed)
                if EncodedPath::from_path(&observed.path).bytes == worktree.path.bytes
                    && EncodedPath::from_path(&observed.git_directory).bytes
                        == worktree.git_directory.bytes
                    && state.repositories.iter().any(|r| {
                        r.repository_id == worktree.repository_id
                            && r.common_directory.bytes
                                == EncodedPath::from_path(&observed.common_directory).bytes
                    }) =>
            {
                "registered"
            }
            Ok(_) => "mismatched",
            Err(_) => "unreadable",
        },
    }
}

fn handle_recovery(
    cli: &Cli,
    paths: &Paths,
    recovery: &RecoveryCommand,
) -> Result<Envelope, PoolError> {
    let command = command_name(cli);

    let (apply, catalog_scope, operation, abandon) = match recovery {
        RecoveryCommand::Apply {
            catalog,
            operation,
            abandon,
            ..
        } => (true, *catalog, operation.as_deref(), *abandon),
        RecoveryCommand::Preview {
            catalog, operation, ..
        } => (false, *catalog, operation.as_deref(), false),
    };
    if catalog_scope {
        return Ok(crate::catalog_recovery_cli::recover_catalog(
            paths,
            cli.repo.as_deref(),
            apply,
            operation,
        ));
    }
    if let Some(operation) = operation
        && let Some(envelope) = crate::catalog_recovery_cli::recover_known_operation(
            paths,
            cli.repo.as_deref(),
            false,
            operation,
        )
    {
        if abandon {
            return Err(PoolError::Selectors);
        }
        return Ok(if apply {
            crate::catalog_recovery_cli::recover_known_operation(
                paths,
                cli.repo.as_deref(),
                true,
                operation,
            )
            .ok_or(PoolError::Unregistered)?
        } else {
            envelope
        });
    }
    let (state, data) = recovery_resource(cli, paths, recovery)?;
    let pending = matches!(recovery, RecoveryCommand::Apply { .. })
        && data["operation"]["state"]
            .as_str()
            .is_some_and(|state| state != "completed");
    Ok(Envelope {
        schema_version: 1,
        command: command.into(),
        outcome: if pending { "pending" } else { "completed" },
        reason_code: if pending {
            "operation_pending"
        } else {
            "recovery_observed"
        },
        context: json!({"catalog_id":state.catalog_id,"catalog_path":EncodedPath::from_path(&paths.catalog),"repository_id":data["repository_id"],"worktree_id":data["worktree_id"],"assignment_handle":data["assignment_handle"],"operation_id":data["operation_id"]}),
        data,
        warnings: Vec::new(),
    })
}

fn recovery_resource(
    cli: &Cli,
    paths: &Paths,
    recovery: &RecoveryCommand,
) -> Result<(crate::domain::CatalogProjection, Value), PoolError> {
    Ok(match recovery {
        RecoveryCommand::Preview {
            worktree: Some(worktree),
            ..
        } => crate::recovery_workflow::recover_worktree(
            paths,
            cli.repo.as_deref(),
            worktree,
            false,
            false,
        )?,
        RecoveryCommand::Apply {
            worktree: Some(worktree),
            abandon,
            ..
        } => crate::recovery_workflow::recover_worktree(
            paths,
            cli.repo.as_deref(),
            worktree,
            true,
            *abandon,
        )?,
        RecoveryCommand::Preview {
            assignment: None,
            operation: None,
            ..
        } => crate::recovery_inspection::preview_candidates(paths, cli.repo.as_deref())?,
        RecoveryCommand::Apply {
            assignment: Some(assignment),
            abandon: false,
            ..
        } => {
            crate::recovery_workflow::reconcile_assignment(paths, cli.repo.as_deref(), assignment)?
        }
        RecoveryCommand::Preview {
            assignment: Some(assignment),
            ..
        } => crate::recovery_workflow::preview_assignment(paths, cli.repo.as_deref(), assignment)?,
        RecoveryCommand::Preview {
            operation: Some(operation),
            ..
        } => crate::recovery_workflow::preview_recovery(paths, cli.repo.as_deref(), operation)?,
        RecoveryCommand::Apply {
            assignment: Some(assignment),
            abandon: true,
            ..
        } => crate::recovery_workflow::abandon_assignment(paths, cli.repo.as_deref(), assignment)?,
        RecoveryCommand::Apply {
            operation: Some(operation),
            ..
        } => crate::recovery_workflow::resume_recovery(paths, cli.repo.as_deref(), operation)?,
        _ => return Err(PoolError::Configuration),
    })
}
