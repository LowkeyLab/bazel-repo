//! Verify one reviewed PR before and after its authorized merge.
mod github;
mod verify;

use std::{fs, io::Write, path::PathBuf};

use anyhow::{Context, Result, ensure};
use clap::{Parser, ValueEnum};
use serde_json::{Value, json};

use github::Github;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Phase {
    Before,
    After,
}
#[derive(Parser)]
struct Cli {
    #[arg(value_enum)]
    phase: Phase,
    #[arg(long)]
    repo: String,
    #[arg(long)]
    pr: u64,
    #[arg(long)]
    head: String,
    #[arg(long)]
    reviewed_files: PathBuf,
    #[arg(long)]
    source_review: PathBuf,
    #[arg(long, required_if_eq("phase", "after"))]
    before_receipt: Option<PathBuf>,
    #[arg(long)]
    receipt_dir: Option<PathBuf>,
}
fn inputs(cli: &Cli) -> Result<(Vec<String>, Value)> {
    ensure!(
        cli.repo.eq_ignore_ascii_case("LowkeyLab/bazel-repo"),
        "unsupported repository policy"
    );
    ensure!(cli.pr > 0, "PR number must be positive");
    verify::commit(&cli.head)?;
    let files: Vec<String> = serde_json::from_slice(&fs::read(&cli.reviewed_files)?)?;
    ensure!(!files.is_empty(), "reviewed files are empty");
    let mut unique = std::collections::BTreeSet::new();
    for file in &files {
        ensure!(
            !file.starts_with('/')
                && file
                    .split('/')
                    .all(|p| !p.is_empty() && p != "." && p != ".."),
            "invalid reviewed path"
        );
        ensure!(unique.insert(file), "duplicate reviewed path");
    }
    let review: Value = serde_json::from_slice(&fs::read(&cli.source_review)?)?;
    ensure!(
        review["head"] == cli.head
            && review["standards_findings"].as_u64() == Some(0)
            && review["spec_findings"].as_u64() == Some(0),
        "source reviews do not accept this head"
    );
    Ok((files, review))
}
fn root(explicit: Option<&PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.clone());
    }
    if let Some(state) = std::env::var_os("XDG_STATE_HOME").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(state).join("merge-verification"));
    }
    Ok(
        PathBuf::from(std::env::var_os("HOME").context("HOME is missing")?)
            .join(".local/state/merge-verification"),
    )
}
fn run(cli: &Cli, gh: &mut Github) -> Result<Value> {
    let (files, review) = inputs(cli)?;
    gh.save("source-review.json", &review)?;
    gh.save("inputs.json", &json!({"repo":cli.repo,"pr":cli.pr,"head":cli.head,"files":files,"phase":format!("{:?}",cli.phase)}))?;
    match cli.phase {
        Phase::Before => verify::before(cli, &files, gh),
        Phase::After => verify::after(cli, &files, gh),
    }
}
fn main() -> Result<()> {
    let cli = Cli::parse();
    let result = (|| -> Result<(Value, bool)> {
        let mut gh = Github::new(&root(cli.receipt_dir.as_ref())?)?;
        let outcome = run(&cli, &mut gh);
        let (mut evidence, passed) = match outcome {
            Ok(v) => (v, true),
            Err(e) => (json!({"reasons":[format!("{e:#}")]}), false),
        };
        evidence["schema_version"] = json!(1);
        evidence["passed"] = json!(passed);
        evidence["repository"] = json!(cli.repo.to_ascii_lowercase());
        evidence["pr"] = json!(cli.pr);
        evidence["head"] = json!(cli.head);
        evidence["phase"] = json!(if cli.phase == Phase::Before {
            "before"
        } else {
            "after"
        });
        evidence["receipt_directory"] = json!(gh.directory());
        gh.save("result.json", &evidence)?;
        Ok((evidence, passed))
    })();
    let (value, passed) = match result {
        Ok(v) => v,
        Err(e) => (json!({"passed":false,"reasons":[format!("{e:#}")]}), false),
    };
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &value)?;
    out.write_all(b"\n")?;
    if !passed {
        std::process::exit(1);
    }
    Ok(())
}
