//! Run the retained command fixtures with native child-profile collection.
use std::process::Command;

use anyhow::Context;
use googletest::{assert_that, gtest, matchers::eq};
use runfiles::{Runfiles, rlocation};

#[gtest]
fn retained_command_fixtures_pass_with_the_declared_native_child() -> anyhow::Result<()> {
    let files = Runfiles::create()?;
    let cases = rlocation!(files, std::env::var("COMMAND_CASES")?)
        .context("The retained command fixture executable is missing.")?;
    let result = Command::new(cases)
        .args([
            "-o",
            &std::env::var("COMMAND_FILTER")
                .unwrap_or_else(|_| "python_files=go_report_test.py".to_owned()),
        ])
        .output()?;
    assert_that!(
        result.status.success(),
        eq(true),
        "Command fixtures failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    Ok(())
}
