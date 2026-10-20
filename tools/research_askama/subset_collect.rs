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
        "Expected baseline, events, completion, output directory, and control"
    );
    let mut baseline: Value = serde_json::from_slice(&std::fs::read(&args[0])?)?;
    let mut profile: Value = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let mut receipt: Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let output = PathBuf::from(&args[3]);
    std::fs::create_dir_all(&output)?;
    match args[4].as_str() {
        "valid" => {}
        "missing-include" => {
            baseline["files"]
                .as_array_mut()
                .context("Missing files")?
                .remove(1);
        }
        "source-hash" => baseline["files"][0]["sha256"] = json!("wrong"),
        "unknown-event" => profile
            .as_array_mut()
            .context("Missing events")?
            .push(json!(["unknown.html", 3, null])),
        "incomplete" => receipt["functional"] = json!(false),
        "profile-hash" => receipt["events_sha256"] = json!("wrong"),
        other => bail!("Unknown control: {other}"),
    }
    std::fs::write(
        output.join("baseline-input.json"),
        serde_json::to_vec_pretty(&baseline)?,
    )?;
    std::fs::write(
        output.join("events-input.json"),
        serde_json::to_vec_pretty(&profile)?,
    )?;
    std::fs::write(
        output.join("completion-input.json"),
        serde_json::to_vec_pretty(&receipt)?,
    )?;
    ensure!(baseline["schema"] == 1, "Unsupported coverage schema");
    ensure!(
        receipt["functional"] == true,
        "Functional completion is missing"
    );
    ensure!(
        receipt["events_sha256"] == digest(std::fs::read(&args[1])?),
        "Incomplete or corrupt template collection"
    );
    let files = baseline["files"]
        .as_array()
        .context("Missing baseline files")?;
    let expected = [
        (
            "nicknamer/server/lib/templates/names/names_table.html",
            vec![1, 31, 41],
        ),
        (
            "nicknamer/server/lib/templates/names/name_row.html",
            vec![1, 2, 3, 4, 8, 30],
        ),
        ("tools/research_askama/templates/unused.html", vec![1, 2]),
    ];
    ensure!(
        files.len() == expected.len(),
        "Incomplete bounded inventory"
    );
    for (path, lines) in &expected {
        let file = files
            .iter()
            .find(|file| file["path"] == *path)
            .context("Expected source is missing")?;
        ensure!(file["path"] == *path, "Unexpected source identity");
        let actual = file["nodes"]
            .as_array()
            .context("Missing nodes")?
            .iter()
            .map(|n| n["line"].as_u64().context("Missing line"))
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        ensure!(
            actual == lines.iter().copied().collect(),
            "Executable line inventory differs from independent expectations"
        );
    }
    let mut zero = BTreeMap::<Key, u64>::new();
    for file in files {
        let path = file["path"].as_str().context("Missing path")?;
        ensure!(
            file["sha256"]
                == digest(std::fs::read(
                    if let Some(relative) = path.strip_prefix("nicknamer/server/lib/templates/") {
                        format!("tools/research_askama/maintained_fixture/templates/{relative}")
                    } else {
                        path.to_owned()
                    }
                )?),
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
    for event in profile.as_array().context("Missing profile events")? {
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
    std::fs::write(output.join("baseline.lcov"), lcov(files, &zero)?)?;
    std::fs::write(output.join("measured.lcov"), lcov(files, &measured)?)?;
    std::fs::write(
        output.join("combined-zero-first.lcov"),
        lcov(files, &first)?,
    )?;
    std::fs::write(output.join("combined-zero-last.lcov"), lcov(files, &last)?)?;
    std::fs::write(
        output.join("collection-result.json"),
        serde_json::to_vec_pretty(
            &json!({"bounded_subset_complete":true,"both_orders_equal":true,"original_template_files":files.len(),"supported_branch":"simple Boolean if/else","loop_branches":"not measured"}),
        )?,
    )?;
    if measured.is_empty() {
        bail!("No template observations");
    }
    Ok(())
}
