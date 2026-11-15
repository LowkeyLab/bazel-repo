//! Embedded asset declarations must not retain an absolute build sandbox path.
use std::{fs, path::Path};

use anyhow::Context;
use googletest::{assert_that, matchers::eq};
use topcoat::asset::{RawAsset, Source, asset};

#[googletest::test]
fn compiled_asset_path_is_relative_to_the_execution_root() -> anyhow::Result<()> {
    let fixture = asset!("asset_path_test.rs");
    let id = fixture.id();
    let declarations = RawAsset::find_in_binary(&fs::read(std::env::current_exe()?)?);
    let declaration = declarations
        .iter()
        .find(|declaration| declaration.id() == id)
        .context("compiled fixture asset declaration")?;
    let Source::Path(path) = declaration.source() else {
        anyhow::bail!("fixture must be a local file");
    };
    assert_that!(path.is_absolute(), eq(false));
    assert_that!(
        path.ends_with(Path::new("tools/topcoat/asset_path_test.rs")),
        eq(true)
    );
    Ok(())
}
