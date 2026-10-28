//! Capture one explicit pool invocation and return bounded model-facing evidence.
mod capture;
mod summary;

use std::{
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use clap::{Parser, error::ErrorKind};
use serde_json::{Value, json};

use capture::Receipt;
const MAX_INTERPRET_BYTES: u64 = 16 * 1024 * 1024;
#[derive(Parser)]
#[command(
    name = "pool_receipt",
    about = "Capture one explicit pool command and return bounded evidence"
)]
struct Cli {
    #[arg(long)]
    task: String,
    #[arg(long)]
    receipt_dir: Option<PathBuf>,
    #[arg(long)]
    assignment: Option<String>,
    #[arg(last = true, required = true)]
    pool_args: Vec<OsString>,
}
fn root(explicit: Option<&PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.clone());
    }
    if let Some(state) = std::env::var_os("XDG_STATE_HOME").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(state).join("worktree-pool/agent-receipts"));
    }
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME missing")?)
            .join(".local/state/worktree-pool/agent-receipts"),
    )
}
fn execute(cli: &Cli, receipt: &mut Receipt) -> Result<Value> {
    receipt.run(&cli.pool_args, &cli.task, cli.assignment.as_deref())?;
    let stdout = receipt.path().join("stdout");
    ensure!(
        fs::metadata(&stdout)?.len() <= MAX_INTERPRET_BYTES,
        "raw response exceeds the 16 MiB interpretation budget"
    );
    let value: Value = serde_json::from_reader(fs::File::open(stdout)?)
        .context("raw pool JSON is partial or invalid")?;
    let first = cli.pool_args.first().and_then(|s| s.to_str());
    let inspect =
        first == Some("assignment") && cli.pool_args.get(1).is_some_and(|s| s == "inspect");
    let requires =
        cli.assignment.is_some() || matches!(first, Some("acquire" | "release")) || inspect;
    let interpreted = (|| -> Result<Value> {
        let (result, selected) =
            summary::interpret(&value, receipt.code, cli.assignment.as_deref(), requires)?;
        if let Some(selected) = selected {
            receipt.save("selected-assignment.json", &selected)?;
        }
        Ok(result)
    })();
    match interpreted {
        Ok(result) => Ok(result),
        Err(error) => {
            let reported = value["outcome"]
                .as_str()
                .filter(|s| matches!(*s, "completed" | "rejected" | "pending" | "unknown"));
            Ok(
                json!({"wrapper_exit":4,"pool_outcome":reported,"interpretation_failure":summary::excerpt(&format!("{error:#}"),256),"follow_up":"Stop effects. Inspect the recorded operation and assignment. Do not retry uncertain mutations or recover without exact human approval."}),
            )
        }
    }
}
fn main() -> Result<()> {
    // This synchronous executable sets its mask before creating files or children.
    rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o077));
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let help = error.kind() == ErrorKind::DisplayHelp;
            let mut result = json!({"schema_version":1,"wrapper_exit":if help {0} else {4},"pool_started":false,"capture_complete":false,"pool_exit":null});
            if help {
                result["usage_informational"] = json!(summary::excerpt(&error.to_string(), 4096));
            } else {
                result["argument_failure"] = json!(format!("{:?}", error.kind()));
                result["follow_up"] =
                    json!("Correct wrapper arguments before running a pool command.");
            }
            return report(result, None);
        }
    };
    let initialized = root(cli.receipt_dir.as_ref()).and_then(|root| Receipt::new(&root));
    let (result, directory) = match initialized {
        Ok(mut receipt) => {
            let outcome = execute(&cli, &mut receipt);
            let mut value = match outcome {
                Ok(value) => value,
                Err(error) => {
                    let mut failure = json!({"wrapper_exit":4,"follow_up":"Stop effects. Preserve this receipt. Inspect known identities and request human resolution when ownership is uncertain."});
                    let key = if receipt.complete {
                        "interpretation_failure"
                    } else {
                        "capture_failure"
                    };
                    failure[key] = json!(summary::excerpt(&format!("{error:#}"), 256));
                    failure
                }
            };
            value["schema_version"] = json!(1);
            value["task_informational"] = json!(summary::excerpt(&cli.task, 128));
            value["pool_exit"] = json!(receipt.code);
            value["signal"] = json!(receipt.signal);
            value["pool_pid"] = json!(receipt.pid);
            value["pool_started"] = json!(receipt.started);
            value["capture_complete"] = json!(receipt.complete);
            if value["wrapper_exit"] == 4
                && let Err(error) = receipt.save("wrapper-error.json", &value)
            {
                value["capture_failure"] = json!(summary::excerpt(&error.to_string(), 256));
            }
            (value, Some(receipt.path().to_owned()))
        }
        Err(error) => (
            json!({"schema_version":1,"wrapper_exit":4,"pool_started":false,"capture_complete":false,"capture_failure":summary::excerpt(&format!("{error:#}"),256),"follow_up":"Stop effects. Resolve receipt capture before running the command."}),
            None,
        ),
    };
    report(result, directory.as_deref())
}
fn report(result: Value, directory: Option<&Path>) -> Result<()> {
    let bytes = summary::encode(result, directory)?;
    let output: Value = serde_json::from_slice(&bytes)?;
    let exit = i32::try_from(
        output["wrapper_exit"]
            .as_i64()
            .context("wrapper exit missing")?,
    )?;
    std::io::stdout().lock().write_all(&bytes)?;
    std::process::exit(exit)
}
