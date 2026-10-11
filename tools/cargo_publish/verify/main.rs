mod manifest;
mod resolution;
mod run;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(about = "Validate declared Cargo publishing inputs with isolated pinned Cargo/rustc")]
struct Args {
    #[arg(long)]
    package: PathBuf,
    #[arg(long)]
    contract: PathBuf,
    #[arg(long)]
    cargo: PathBuf,
    #[arg(long)]
    rustc: PathBuf,
    #[arg(long)]
    sysroot: PathBuf,
    #[arg(long)]
    output_root: PathBuf,
    #[arg(long)]
    registry_cache: Option<PathBuf>,
    #[arg(long)]
    offline: bool,
}

fn main() {
    if let Err(error) = execute() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}

fn execute() -> Result<()> {
    let mut args = Args::parse();
    for path in [
        &mut args.package,
        &mut args.contract,
        &mut args.cargo,
        &mut args.rustc,
        &mut args.sysroot,
        &mut args.output_root,
    ] {
        *path = std::path::absolute(&*path)?;
    }
    let mut run = run::ValidationRun::new(args)?;
    if let Err(error) = run.execute() {
        run.fail(&format!("{error:#}"))?;
        anyhow::bail!(
            "{}: {error:#}; retained report: {}",
            run.phase_name(),
            run.path.display()
        );
    }
    Ok(())
}
