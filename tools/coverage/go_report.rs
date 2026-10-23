use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(
    about = "Apply native scope exclusions and validate required LCOV records.",
    infer_long_args = true,
    args_override_self = true
)]
struct Arguments {
    #[arg(long, default_value = "go", value_parser = ["go", "rust"])]
    language: String,
    #[arg(long)]
    events: PathBuf,
    #[arg(long)]
    expected_output: Option<PathBuf>,
    #[arg(long)]
    inventory: PathBuf,
    #[arg(long)]
    report: PathBuf,
}

fn main() {
    let arguments = Arguments::parse();
    if let Err(error) = coverage_reports::native_report::run(
        &arguments.language,
        &arguments.events,
        &arguments.inventory,
        &arguments.report,
        arguments.expected_output.as_deref(),
    ) {
        let family = if arguments.language == "rust" {
            "Rust"
        } else {
            "Go"
        };
        eprintln!("{family} coverage report failed: {error}");
        std::process::exit(1);
    }
}
