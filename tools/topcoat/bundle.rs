//! Stage declared local assets and call the published Topcoat bundler offline.
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use topcoat_asset::{Bundler, BundlerConfig, RawAsset, Source};

#[derive(Deserialize)]
struct Input {
    path: PathBuf,
    relative: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 4,
        "expected binary, output directory, input manifest, executable name"
    );
    let executable_name = PathBuf::from(&args[3]);
    ensure!(
        executable_name.file_name() == Some(executable_name.as_os_str()),
        "expected an executable basename"
    );
    let binary = fs::canonicalize(&args[0])?;
    let output = std::env::current_dir()?.join(&args[1]);
    let inputs: Vec<Input> = serde_json::from_slice(&fs::read(&args[2])?)?;
    let inputs: Vec<_> = inputs
        .into_iter()
        .map(|i| Ok((fs::canonicalize(&i.path)?, i.relative, i.path)))
        .collect::<anyhow::Result<_>>()?;
    let bytes = fs::read(&binary)?;
    let stage = tempfile::tempdir()?;
    // Topcoat anchors file!() paths by probing source-file existence. Mirror every
    // declared companion before resolving assets, including coverage-prefixed paths.
    for (source, relative, original) in &inputs {
        for destination in [relative, original] {
            ensure!(
                !destination.is_absolute()
                    && !destination.components().any(|p| p == Component::ParentDir),
                "declared input escapes staging"
            );
            let staged = stage.path().join(destination);
            stage_input(source, &staged)?;
        }
    }
    std::env::set_current_dir(stage.path())?;
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
            .filter(|(_, relative, _)| destination.ends_with(relative))
            .collect();
        ensure!(
            matches.len() == 1,
            "expected one declared input for {}, found {}",
            destination.display(),
            matches.len()
        );
        let staged = stage.path().join(destination);
        stage_input(&matches[0].0, &staged)?;
    }
    fs::create_dir_all(&output)?;
    fs::copy(binary, output.join(&executable_name))?;
    Bundler::new(&BundlerConfig::new().cache_dir(stage.path().join("empty-download-cache")))
        .bundle(&bytes, output.join("assets"))?;
    let mut files = BTreeMap::new();
    inventory(&output.join("assets"), &output.join("assets"), &mut files)?;
    fs::write(
        output.join("integrity.json"),
        serde_json::to_vec(&serde_json::json!({"binary": digest(&bytes)?, "files": files}))?,
    )?;
    Ok(())
}

fn stage_input(source: &Path, destination: &Path) -> anyhow::Result<()> {
    if destination.exists() {
        ensure!(
            fs::read(destination)? == fs::read(source)?,
            "different declared inputs use the staged path {}",
            destination.display()
        );
        return Ok(());
    }
    fs::create_dir_all(destination.parent().context("staged input parent")?)?;
    fs::copy(source, destination)?;
    Ok(())
}

fn inventory(
    root: &std::path::Path,
    directory: &std::path::Path,
    files: &mut BTreeMap<String, String>,
) -> anyhow::Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            inventory(root, &path, files)?;
        } else {
            files.insert(
                path.strip_prefix(root)?.to_string_lossy().into_owned(),
                digest(&fs::read(&path)?)?,
            );
        }
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> anyhow::Result<String> {
    let mut hash = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(hash, "{byte:02x}")?;
    }
    Ok(hash)
}
