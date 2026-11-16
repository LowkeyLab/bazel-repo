//! Exercise the target-configured formatter separately from its exec-configured adapter.
use std::{fs, process::Command};

use anyhow::Context;
use googletest::{assert_that, matchers::eq};
use runfiles::{Runfiles, rlocation};

#[googletest::test]
fn formatter_binary_checks_without_writing_and_converges() -> anyhow::Result<()> {
    let files = Runfiles::create()?;
    let formatter =
        rlocation!(files, std::env::var("FORMAT_IMPL_TOOL")?).context("formatter runfile")?;
    let rustfmt = rlocation!(files, std::env::var("RUSTFMT_TOOL")?).context("rustfmt runfile")?;
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("example.rs");
    let original = "fn ordinary () {let _value=1;}\nfn panel(){let _ = view!{<main><p>\"Hello\"</p></main>};}\n";
    fs::write(&source, original)?;
    let check = Command::new(&formatter)
        .arg(&rustfmt)
        .arg("--check")
        .arg(&source)
        .current_dir(directory.path())
        .output()?;
    assert_that!(check.status.success(), eq(false));
    assert_that!(fs::read_to_string(&source)?, eq(original));
    let formatted = Command::new(&formatter)
        .arg(&rustfmt)
        .arg(&source)
        .current_dir(directory.path())
        .output()?;
    anyhow::ensure!(
        formatted.status.success(),
        "formatter failed: {}",
        String::from_utf8_lossy(&formatted.stderr)
    );
    let once = fs::read_to_string(&source)?;
    assert_that!(once.contains("let _value = 1;"), eq(true));
    let check = Command::new(&formatter)
        .arg(&rustfmt)
        .arg("--check")
        .arg(&source)
        .current_dir(directory.path())
        .output()?;
    assert_that!(check.status.success(), eq(true));
    let second = Command::new(&formatter)
        .arg(&rustfmt)
        .arg(&source)
        .current_dir(directory.path())
        .output()?;
    assert_that!(second.status.success(), eq(true));
    assert_that!(fs::read_to_string(&source)?, eq(&once));
    Ok(())
}
