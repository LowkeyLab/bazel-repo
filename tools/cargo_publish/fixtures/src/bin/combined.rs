use clap::Parser;

#[derive(Parser)]
struct Options {
    #[arg(long)]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    let options = Options::try_parse_from(["fixture-cli"])?;
    anyhow::ensure!(!options.verbose, "fixture defaults");
    anyhow::ensure!(fixture_library::answer()? == 42, "selected library value");
    Ok(())
}
