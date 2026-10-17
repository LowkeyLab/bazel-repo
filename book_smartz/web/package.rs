//! Verify that the executable and every adjacent asset belong to one package.
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
    time::{Duration, Instant},
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

// Readiness only reads the last result. One verifier performs blocking work,
// regardless of the number of concurrent public probes. Expire stale success
// if the verifier stalls, rather than continuing to advertise readiness.
pub struct IntegrityMonitor(tokio::sync::watch::Receiver<Option<Instant>>);

impl IntegrityMonitor {
    pub fn start() -> anyhow::Result<Self> {
        let (sender, receiver) = tokio::sync::watch::channel(Some(Instant::now()));
        // Detach this worker so stalled filesystem I/O cannot extend the HTTP
        // drain or Tokio runtime shutdown. Only this one thread scans packages.
        std::thread::Builder::new()
            .name("package-integrity".into())
            .spawn(move || {
                while !sender.is_closed() {
                    std::thread::sleep(Duration::from_secs(1));
                    let valid = load().is_ok();
                    sender.send_replace(valid.then(Instant::now));
                }
            })?;
        Ok(Self(receiver))
    }

    pub fn is_ready(&self) -> bool {
        self.0
            .borrow()
            .is_some_and(|verified| verified.elapsed() < Duration::from_secs(3))
    }
}
