//! Prose policy through the configured Vale command and real styles.
use std::{collections::BTreeMap, process::Command};

use anyhow::Context;
use googletest::{assert_that, matchers::eq};
use runfiles::{Runfiles, rlocation};
use serde_json::Value;

fn check(text: &str) -> anyhow::Result<(bool, Vec<Value>)> {
    let files = Runfiles::create()?;
    let binary = rlocation!(files, std::env::var("VALE_BINARY")?).context("Vale binary missing")?;
    let config =
        rlocation!(files, std::env::var("VALE_CONFIG")?).context("Vale configuration missing")?;
    let styles = rlocation!(files, std::env::var("VALE_STYLES")?).context("Vale styles missing")?;
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("prose.md");
    std::fs::write(&source, text)?;
    let output = Command::new(binary)
        .arg("--config")
        .arg(config)
        .arg("--output=JSON")
        .arg(source)
        .env("VALE_STYLES_PATH", styles)
        .output()?;
    let reports: BTreeMap<String, Vec<Value>> = serde_json::from_slice(&output.stdout)
        .with_context(|| {
            format!(
                "Invalid Vale report: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
    Ok((
        output.status.success(),
        reports.into_values().flatten().collect(),
    ))
}

#[googletest::test]
fn clean_prose_succeeds() -> anyhow::Result<()> {
    let (success, diagnostics) = check("# Checks\n\nUse the current configuration.\n")?;
    assert_that!(success, eq(true));
    assert_that!(diagnostics.is_empty(), eq(true));
    Ok(())
}

#[googletest::test]
fn blocking_prose_fails_with_an_error_diagnostic() -> anyhow::Result<()> {
    let (success, diagnostics) = check("# Checks\n\nUse the CLI.\n")?;
    assert_that!(success, eq(false));
    assert_that!(
        diagnostics
            .iter()
            .any(|d| d["Check"] == "Google.WordList" && d["Severity"] == "error"),
        eq(true)
    );
    Ok(())
}

#[googletest::test]
fn advisory_prose_succeeds_with_a_suggestion() -> anyhow::Result<()> {
    let (success, diagnostics) = check("# Checks\n\nLLVM reads code.\n")?;
    assert_that!(success, eq(true));
    assert_that!(
        diagnostics
            .iter()
            .any(|d| d["Check"] == "Google.Acronyms" && d["Severity"] == "suggestion"),
        eq(true)
    );
    Ok(())
}

#[googletest::test]
fn strict_ste_prose_has_no_contraction_suggestion() -> anyhow::Result<()> {
    let (success, diagnostics) = check("# Checks\n\nDo not change the configuration.\n")?;
    assert_that!(success, eq(true));
    assert_that!(
        diagnostics
            .iter()
            .any(|d| d["Check"] == "Google.Contractions"),
        eq(false)
    );
    Ok(())
}
