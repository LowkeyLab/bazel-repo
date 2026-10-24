use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(infer_long_args = true, args_override_self = true)]
struct Arguments {
    #[arg(long)]
    inventory: PathBuf,
    #[arg(long)]
    report: PathBuf,
    #[arg(long)]
    events: PathBuf,
    #[arg(long)]
    evidence: PathBuf,
    #[arg(long)]
    collector: PathBuf,
    #[arg(long)]
    merger: PathBuf,
}
fn main() {
    rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o077));
    let args = Arguments::parse();
    if let Err(error) = coverage_reports::javascript_report::run(
        &args.inventory,
        &args.events,
        &args.report,
        &args.evidence,
        &args.collector,
        &args.merger,
    ) {
        let class = match &error {
            coverage_reports::ReportError::Io(error)
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                "FileNotFoundError: "
            }
            coverage_reports::ReportError::Json(_) => "JSONDecodeError: ",
            _ => "",
        };
        eprintln!("JavaScript coverage report failed: {class}{error}");
        std::process::exit(1);
    }
}
