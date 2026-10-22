//! The bundle command accepts repeated read-only inputs and rejects path conflicts.
use std::{fs, process::Command};

use anyhow::Context;
use googletest::{
    assert_that,
    matchers::{contains_substring, eq},
};
use runfiles::{Runfiles, rlocation};
use topcoat_asset::{AssetId, AssetOptions, RawAsset};

fn bundler() -> anyhow::Result<std::path::PathBuf> {
    let files = Runfiles::create()?;
    rlocation!(files, std::env::var("BUNDLE_TOOL")?).context("bundle runfile")
}

#[googletest::test]
fn repeated_read_only_inputs_produce_the_declared_asset() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let source = root.join("asset.txt");
    fs::write(&source, "read-only asset")?;
    let mut permissions = fs::metadata(&source)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&source, permissions)?;
    let options = AssetOptions::NONE;
    let binary = RawAsset::encode(
        AssetId::new("fixture", "lib.rs", "asset.txt", &options),
        "asset.txt",
        "fixture",
        "",
        "lib.rs",
        &options,
    );
    fs::write(root.join("binary"), binary)?;
    fs::write(
        root.join("inputs.json"),
        br#"[{"path":"asset.txt","relative":"asset.txt"}]"#,
    )?;
    let result = Command::new(bundler()?)
        .current_dir(root)
        .args(["binary", "output", "inputs.json", "application"])
        .output()?;
    anyhow::ensure!(
        result.status.success(),
        "bundle failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_that!(
        fs::read(root.join("output/application"))?,
        eq(&binary.to_vec())
    );
    let integrity: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("output/integrity.json"))?)?;
    let asset = integrity["files"]
        .as_object()
        .context("asset inventory")?
        .keys()
        .find(|name| name.ends_with(".txt"))
        .context("bundled text asset")?;
    assert_that!(
        fs::read_to_string(root.join("output/assets").join(asset))?,
        eq("read-only asset")
    );
    Ok(())
}

#[googletest::test]
fn conflicting_declared_inputs_cannot_replace_a_staged_file() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    fs::write(root.join("binary"), "bundle fixture")?;
    fs::write(root.join("first.txt"), "first asset")?;
    fs::write(root.join("second.txt"), "second asset")?;
    fs::write(root.join("inputs.json"), br#"[{"path":"first.txt","relative":"shared.txt"},{"path":"second.txt","relative":"shared.txt"}]"#)?;
    let result = Command::new(bundler()?)
        .current_dir(root)
        .args(["binary", "output", "inputs.json", "application"])
        .output()?;
    assert_that!(result.status.success(), eq(false));
    assert_that!(
        String::from_utf8_lossy(&result.stderr).as_ref(),
        contains_substring("different declared inputs")
    );
    assert_that!(root.join("output/application").exists(), eq(false));
    Ok(())
}
