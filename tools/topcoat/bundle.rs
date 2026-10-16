//! Stage declared local assets and call the published Topcoat bundler offline.
use std::{
    fs,
    path::{Component, PathBuf},
};

use anyhow::{Context, ensure};
use serde::Deserialize;
use topcoat_asset::{Bundler, BundlerConfig, RawAsset, Source};

#[derive(Deserialize)]
struct Input {
    path: PathBuf,
    relative: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 3,
        "expected binary, output directory, input manifest"
    );
    let binary = fs::canonicalize(&args[0])?;
    let output = std::env::current_dir()?.join(&args[1]);
    let inputs: Vec<Input> = serde_json::from_slice(&fs::read(&args[2])?)?;
    let inputs: Vec<_> = inputs
        .into_iter()
        .map(|i| Ok((fs::canonicalize(i.path)?, i.relative)))
        .collect::<anyhow::Result<_>>()?;
    let bytes = fs::read(&binary)?;
    let stage = tempfile::tempdir()?;
    for asset in RawAsset::find_in_binary(&bytes) {
        let Source::Path(destination) = asset.source() else {
            anyhow::bail!(
                "undeclared remote asset rejected: {:?}",
                asset.source().to_string()
            );
        };
        ensure!(
            !destination.is_absolute()
                && !destination.components().any(|p| p == Component::ParentDir),
            "asset declaration escapes staged inputs: {}",
            destination.display()
        );
        let matches: Vec<_> = inputs
            .iter()
            .filter(|(_, relative)| destination.ends_with(relative))
            .collect();
        ensure!(
            matches.len() == 1,
            "expected one declared input for {}, found {}",
            destination.display(),
            matches.len()
        );
        let staged = stage.path().join(destination);
        fs::create_dir_all(staged.parent().context("asset parent")?)?;
        fs::copy(&matches[0].0, staged)?;
    }
    fs::create_dir_all(&output)?;
    fs::copy(binary, output.join("app"))?;
    std::env::set_current_dir(stage.path())?;
    Bundler::new(&BundlerConfig::new().cache_dir(stage.path().join("empty-download-cache")))
        .bundle(&bytes, output.join("assets"))?;
    Ok(())
}
