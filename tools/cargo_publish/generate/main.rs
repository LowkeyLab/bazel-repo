use std::path::PathBuf;

use anyhow::Result;
use cargo_publish_generate::{GenerateOptions, generate};
use clap::Parser;

#[derive(Parser)]
#[command(about = "Generate declared Cargo inputs from Bazel analysis, without registry access")]
struct Args {
    #[arg(long)]
    spec: PathBuf,
    #[arg(long)]
    root_manifest: PathBuf,
    #[arg(long)]
    root_lockfile: PathBuf,
    #[arg(long)]
    module: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    contract: PathBuf,
}

fn run() -> Result<()> {
    let args = Args::parse();
    generate(&GenerateOptions {
        spec: args.spec,
        root_manifest: args.root_manifest,
        root_lockfile: args.root_lockfile,
        module: args.module,
        output: args.output,
        contract: args.contract,
    })?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("cargo_publish generation: {error}");
        std::process::exit(1);
    }
}
