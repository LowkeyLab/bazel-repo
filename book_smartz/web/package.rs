//! Verify that the executable and every adjacent asset belong to one package.
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use topcoat::asset::AssetBundle;

#[derive(Deserialize)]
struct Integrity {
    binary: String,
    files: BTreeMap<String, String>,
}
fn digest(path: &Path) -> anyhow::Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

pub fn load() -> anyhow::Result<AssetBundle> {
    let exe = std::env::current_exe()?;
    let parent = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("package directory missing"))?;
    let integrity: Integrity = serde_json::from_slice(&fs::read(parent.join("integrity.json"))?)?;
    anyhow::ensure!(
        digest(&exe)? == integrity.binary,
        "package executable mismatch"
    );
    anyhow::ensure!(!integrity.files.is_empty(), "empty package assets");
    for (name, expected) in &integrity.files {
        let path = Path::new(name);
        anyhow::ensure!(
            path.components().all(|p| matches!(p, Component::Normal(_))),
            "invalid asset path"
        );
        anyhow::ensure!(
            digest(&parent.join("assets").join(path))? == *expected,
            "package asset mismatch"
        );
    }
    let bundle = AssetBundle::load()?;
    anyhow::ensure!(
        integrity.files.contains_key("manifest.toml"),
        "manifest absent from package integrity"
    );
    for asset in bundle.catalog().assets() {
        anyhow::ensure!(
            integrity.files.contains_key(asset.name()),
            "unverified bundled asset"
        );
    }
    Ok(bundle)
}

pub fn release() -> anyhow::Result<String> {
    digest(&std::env::current_exe()?)
}
