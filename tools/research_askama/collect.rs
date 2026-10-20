use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn digest(value: impl AsRef<[u8]>) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

type Key = (String, u64, Option<u64>);

fn lcov(files: &[Value], counts: &BTreeMap<Key, u64>) -> Result<String> {
    let mut text = String::new();
    for file in files {
        let path = file["path"].as_str().context("Missing source identity")?;
        text += &format!("TN:askama-research\nSF:{path}\n");
        let mut lines = BTreeMap::<u64, u64>::new();
        for node in file["nodes"].as_array().context("Missing AST inventory")? {
            let id = node["id"].as_u64().context("Missing node identity")?;
            let line = node["line"].as_u64().context("Missing source line")?;
            let count = *counts.get(&(path.to_owned(), id, None)).unwrap_or(&0);
            lines
                .entry(line)
                .and_modify(|c| *c = (*c).max(count))
                .or_insert(count);
        }
        for (line, count) in lines {
            text += &format!("DA:{line},{count}\n");
        }
        for node in file["nodes"].as_array().context("Missing nodes")? {
            let id = node["id"].as_u64().context("Missing id")?;
            for arm in 0..node["arms"].as_u64().context("Missing arms")? {
                text += &format!(
                    "BRDA:{},{id},{arm},{}\n",
                    node["line"],
                    counts.get(&(path.to_owned(), id, Some(arm))).unwrap_or(&0)
                );
            }
        }
        text += "end_of_record\n";
    }
    Ok(text)
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        args.len() == 5,
        "Expected baseline, profile, receipt, native LCOV, and output directory"
    );
    let artifact_dir = PathBuf::from(&args[1])
        .parent()
        .context("Missing artifact directory")?
        .to_owned();
    ensure!(
        std::fs::read(artifact_dir.join("native-rendered.html"))?
            == std::fs::read(artifact_dir.join("adapter-rendered.html"))?,
        "Instrumentation changes rendered output"
    );
    let baseline_bytes = std::fs::read(&args[0])?;
    let profile_bytes = std::fs::read(&args[1])?;
    let baseline: Value = serde_json::from_slice(&baseline_bytes)?;
    let profile: Value = serde_json::from_slice(&profile_bytes)?;
    let receipt: Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    ensure!(
        baseline["schema"] == 1 && profile["schema"] == 1,
        "Unsupported coverage schema"
    );
    ensure!(
        profile["baseline_sha256"] == digest(&baseline_bytes),
        "Profile baseline identity mismatch"
    );
    ensure!(
        receipt["complete"] == true && receipt["profile_sha256"] == digest(&profile_bytes),
        "Incomplete or corrupt template collection"
    );
    let files = baseline["files"]
        .as_array()
        .context("Missing baseline files")?;
    ensure!(files.len() == 3, "Incomplete bounded inventory");
    let mut zero = BTreeMap::<Key, u64>::new();
    for file in files {
        let path = file["path"].as_str().context("Missing path")?;
        ensure!(
            file["sha256"] == digest(std::fs::read(path)?),
            "Original source hash mismatch: {path}"
        );
        for node in file["nodes"].as_array().context("Missing nodes")? {
            let id = node["id"].as_u64().context("Missing id")?;
            zero.insert((path.to_owned(), id, None), 0);
            for arm in 0..node["arms"].as_u64().context("Missing arms")? {
                zero.insert((path.to_owned(), id, Some(arm)), 0);
            }
        }
    }
    let mut measured = BTreeMap::<Key, u64>::new();
    for event in profile["events"]
        .as_array()
        .context("Missing profile events")?
    {
        let row = event.as_array().context("Invalid event")?;
        ensure!(row.len() == 3, "Invalid event width");
        let key = (
            row[0].as_str().context("Missing event source")?.to_owned(),
            row[1].as_u64().context("Missing event id")?,
            if row[2].is_null() {
                None
            } else {
                Some(row[2].as_u64().context("Invalid arm")?)
            },
        );
        ensure!(zero.contains_key(&key), "Unexpected template event");
        *measured.entry(key).or_default() += 1;
    }
    let mut first = zero.clone();
    for (key, count) in &measured {
        *first.entry(key.clone()).or_default() += count;
    }
    let mut last = measured.clone();
    for (key, count) in &zero {
        *last.entry(key.clone()).or_default() += count;
    }
    ensure!(first == last, "Merge order changes counters");
    let output = PathBuf::from(&args[4]);
    std::fs::create_dir_all(&output)?;
    std::fs::write(output.join("baseline.lcov"), lcov(files, &zero)?)?;
    std::fs::write(output.join("measured.lcov"), lcov(files, &measured)?)?;
    std::fs::write(
        output.join("combined-zero-first.lcov"),
        lcov(files, &first)?,
    )?;
    std::fs::write(output.join("combined-zero-last.lcov"), lcov(files, &last)?)?;
    let native = std::fs::read_to_string(&args[3])?;
    ensure!(
        !native.contains(".html"),
        "Unexpected native template identity"
    );
    ensure!(
        native.contains("DA:") && native.contains("native.rs"),
        "Missing native Rust mapping"
    );
    std::fs::write(
        output.join("combined-native-template.lcov"),
        native + &lcov(files, &first)?,
    )?;
    std::fs::write(
        output.join("collection-result.json"),
        serde_json::to_vec_pretty(
            &json!({"complete":true,"both_orders_equal":true,"original_template_files":files.len(),"supported_branch":"simple Boolean if/else","loop_branches":"not measured"}),
        )?,
    )?;
    if measured.is_empty() {
        bail!("No template observations");
    }
    Ok(())
}
