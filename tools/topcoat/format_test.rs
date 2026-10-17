//! The composed formatter preserves check mode and converges for Rust and views.
use std::{fs, process::Command};

use anyhow::Context;
use googletest::{assert_that, matchers::eq};
use runfiles::{Runfiles, rlocation};

#[googletest::test]
fn formatter_checks_without_writing_and_converges() -> anyhow::Result<()> {
    let files = Runfiles::create()?;
    let formatter =
        rlocation!(files, std::env::var("FORMAT_TOOL")?).context("formatter runfile")?;
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("example.rs");
    let original = "fn ordinary () {let _value=1;}\nfn panel(){let _ = view!{<main><p>\"Hello\"</p></main>};}\n";
    fs::write(&source, original)?;
    let check = Command::new(&formatter)
        .arg("--check")
        .arg(&source)
        .current_dir(directory.path())
        .output()?;
    assert_that!(check.status.success(), eq(false));
    assert_that!(fs::read_to_string(&source)?, eq(original));
    let formatted = Command::new(&formatter)
        .arg(&source)
        .current_dir(directory.path())
        .output()?;
    anyhow::ensure!(
        formatted.status.success(),
        "formatter failed: {}",
        String::from_utf8_lossy(&formatted.stderr)
    );
    let once = fs::read_to_string(&source)?;
    let check = Command::new(&formatter)
        .arg("--check")
        .arg(&source)
        .current_dir(directory.path())
        .output()?;
    assert_that!(check.status.success(), eq(true));
    let second = Command::new(&formatter)
        .arg(&source)
        .current_dir(directory.path())
        .output()?;
    assert_that!(second.status.success(), eq(true));
    assert_that!(fs::read_to_string(&source)?, eq(&once));
    Ok(())
}
