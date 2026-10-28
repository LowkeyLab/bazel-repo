//! Controlled pool process for byte capture and uncertain-result tests.
use std::{fs, io::Write, os::unix::ffi::OsStrExt, path::PathBuf};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
fn main() -> Result<()> {
    let fixture =
        PathBuf::from(std::env::var_os("POOL_TEST_RESPONSE").context("response fixture")?);
    let value: Value = serde_json::from_slice(&fs::read(&fixture)?)?;
    let arguments: Vec<_> = std::env::args_os()
        .skip(1)
        .map(|s| s.as_bytes().to_vec())
        .collect();
    let parent = fixture.parent().context("fixture parent")?;
    let mut calls = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(parent.join("calls"))?;
    serde_json::to_writer(&mut calls, &json!(arguments))?;
    calls.write_all(b"\n")?;
    fs::write(parent.join("mask-marker"), b"private")?;
    fs::create_dir_all(parent.join("mask-directory"))?;
    if value["damage_status"].as_bool() == Some(true) {
        let root = parent.join("receipts");
        let entries = fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
        ensure!(entries.len() == 1, "one task-owned invocation required");
        fs::create_dir(entries[0].path().join("exit-status.json"))?;
    }
    if let Some(path) = value["stdout_file"].as_str() {
        std::io::copy(&mut fs::File::open(path)?, &mut std::io::stdout().lock())?;
    } else {
        let stdout: Vec<u8> = serde_json::from_value(value["stdout"].clone())?;
        std::io::stdout().lock().write_all(&stdout)?;
    }
    let stderr: Vec<u8> = serde_json::from_value(value["stderr"].clone())?;
    std::io::stderr().lock().write_all(&stderr)?;
    std::io::stdout().lock().flush()?;
    std::io::stderr().lock().flush()?;
    std::process::exit(i32::try_from(value["exit"].as_i64().context("exit")?)?);
}
