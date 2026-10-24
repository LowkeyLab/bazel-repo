use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(
    about = "Merge standard JaCoCo analysis with exact configured source ownership.",
    infer_long_args = true,
    args_override_self = true
)]
struct Arguments {
    #[arg(long)]
    inventory: PathBuf,
    #[arg(long)]
    events: PathBuf,
    #[arg(long)]
    report: PathBuf,
    #[arg(long)]
    evidence: PathBuf,
    #[arg(long)]
    analyzer: PathBuf,
}

fn main() {
    let arguments = Arguments::parse();
    if let Err(error) = coverage_reports::jvm_report::run(
        &arguments.inventory,
        &arguments.events,
        &arguments.report,
        &arguments.evidence,
        &arguments.analyzer,
    ) {
        eprintln!("JVM coverage report failed: {error}");
        std::process::exit(1);
    }
}
