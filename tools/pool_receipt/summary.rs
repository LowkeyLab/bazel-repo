//! Select bounded evidence without choosing ownership or taking recovery actions.
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub const MAX_OUTPUT: usize = 8192;
pub fn excerpt(value: &str, limit: usize) -> String {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
fn identity(value: &Value, key: &str) -> Result<String> {
    let text = value[key]
        .as_str()
        .with_context(|| format!("missing identity: {key}"))?;
    uuid::Uuid::parse_str(text).with_context(|| format!("invalid identity: {key}"))?;
    ensure!(text.len() == 36, "identity is not canonical: {key}");
    Ok(text.to_owned())
}
fn selected<'a>(value: &'a Value, requested: Option<&str>) -> Result<Option<&'a Value>> {
    let data = &value["data"];
    if let Some(requested) = requested {
        let wanted = uuid::Uuid::parse_str(requested)?;
        let mut candidates = Vec::new();
        if let Some(row) = data.get("assignment") {
            candidates.push(row);
        }
        if let Some(rows) = data["assignments"].as_array() {
            candidates.extend(rows);
        }
        let matching: Vec<_> = candidates
            .into_iter()
            .filter(|row| {
                row["assignment_handle"]
                    .as_str()
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
                    == Some(wanted)
            })
            .collect();
        ensure!(
            matching.len() == 1,
            "selected assignment is missing or ambiguous"
        );
        return Ok(Some(matching[0]));
    }
    Ok(data.get("assignment").filter(|v| !v.is_null()))
}
fn assignment(value: &Value) -> Result<Value> {
    let handle = identity(value, "assignment_handle")?;
    let operation = identity(value, "operation_id")?;
    let repository = identity(value, "repository_id")?;
    let worktree = identity(value, "worktree_id")?;
    let state = value["state"]
        .as_str()
        .context("assignment state missing")?;
    ensure!(
        matches!(state, "active" | "preparing" | "released"),
        "assignment state is unknown"
    );
    let path = &value["path"];
    let bytes = path["bytes"]
        .as_array()
        .context("lossless path bytes missing")?;
    ensure!(
        path["encoding"] == "unix-bytes"
            && bytes.first().and_then(Value::as_u64) == Some(47)
            && bytes
                .iter()
                .all(|b| b.as_u64().is_some_and(|n| n > 0 && n <= 255)),
        "lossless path bytes invalid"
    );
    Ok(
        json!({"assignment_handle":handle,"operation_id":operation,"repository_id":repository,"worktree_id":worktree,"state":state,"path":{"display_informational":path["display"].as_str().map(|s|excerpt(s,512)),"lossless_receipt":"selected-assignment.json"}}),
    )
}
fn catalog(value: &Value) -> Result<Value> {
    let context = &value["context"];
    let data = &value["data"];
    let authority = data.get("authority");
    let catalog = context
        .get("catalog_id")
        .filter(|v| !v.is_null())
        .map(|_| identity(context, "catalog_id"))
        .transpose()?;
    if data.get("catalog_id").is_some_and(|v| !v.is_null()) {
        ensure!(
            Some(identity(data, "catalog_id")?) == catalog,
            "catalog identities conflict"
        );
    }
    if let Some(authority) = authority {
        ensure!(
            Some(identity(authority, "catalog_id")?) == catalog,
            "catalog authority identities conflict"
        );
    }
    let rows = data["operations"].as_array();
    let mut pending = Vec::new();
    let mut count = 0;
    if let Some(rows) = rows {
        for row in rows {
            let state = row["state"].as_str().context("operation state missing")?;
            if matches!(state, "completed" | "rejected" | "reconciled" | "abandoned") {
                continue;
            }
            count += 1;
            if pending.len() < 8 {
                let mut item = json!({"operation_id":identity(row,"operation_id")?,"state":excerpt(state,64),"last_checkpoint":row["last_checkpoint"].as_str().map(|s|excerpt(s,80))});
                for key in ["assignment_handle", "repository_id", "worktree_id"] {
                    if row.get(key).is_some_and(|v| !v.is_null()) {
                        item[key] = json!(identity(row, key)?);
                    }
                }
                pending.push(item);
            }
        }
    }
    let path = authority
        .map(|a| &a["catalog_path"])
        .or_else(|| context.get("catalog_path"))
        .or_else(|| context.get("selected_catalog_path"));
    Ok(
        json!({"catalog_id":catalog,"identity_missing":catalog.is_none(),"path":{"display_informational":path.and_then(|p|p["display"].as_str()).map(|s|excerpt(s,256)),"lossless_receipt":"stdout"},"phase":authority.and_then(|a|a["phase"].as_str()).map(|s|excerpt(s,64)),"mutations_available":data["catalog_mutations_available"].as_bool(),"pending_count":rows.map(|_|count),"pending_operations":pending,"pending_summary_truncated":count>8,"pending_inventory_returned":rows.is_some()}),
    )
}
pub fn interpret(
    value: &Value,
    code: Option<i32>,
    requested: Option<&str>,
    requires_assignment: bool,
) -> Result<(Value, Option<Value>)> {
    ensure!(
        value["schema_version"].as_u64() == Some(1),
        "unsupported or missing pool schema"
    );
    let outcome = value["outcome"].as_str().context("pool outcome missing")?;
    let expected = match outcome {
        "completed" => 0,
        "rejected" => 2,
        "pending" => 3,
        "unknown" => 4,
        _ => anyhow::bail!("pool outcome is unknown"),
    };
    ensure!(
        code == Some(expected),
        "pool outcome and exit status conflict"
    );
    let reported_command = value["command"]
        .as_str()
        .context("reported command missing")?;
    let reason = value["reason_code"]
        .as_str()
        .context("reason code missing")?;
    let mut result = json!({"pool_outcome":outcome,"reported_command":excerpt(reported_command,80),"reason_code":excerpt(reason,128),"catalog":catalog(value)?,"selection_status":"not_requested","assignment":null});
    ensure!(
        expected != 0
            || !result["catalog"]["identity_missing"]
                .as_bool()
                .unwrap_or(true),
        "completed command did not return its catalog identity"
    );
    let mut full = None;
    let candidate = selected(value, requested);
    match candidate {
        Ok(Some(row)) => {
            result["assignment"] = assignment(row)?;
            result["selection_status"] = json!(if requested.is_some() {
                "explicit_handle"
            } else {
                "command_result"
            });
            full = Some(row.clone());
        }
        Ok(None) => {
            ensure!(
                !requires_assignment || expected != 0,
                "completed command did not return its required assignment identity"
            );
        }
        Err(error) => {
            if expected == 0 {
                return Err(error);
            }
            result["selection_status"] = json!("missing_or_ambiguous");
            result["selection_failure"] = json!(excerpt(&error.to_string(), 160));
        }
    }
    let mut context = serde_json::Map::new();
    for key in [
        "operation_id",
        "assignment_handle",
        "repository_id",
        "worktree_id",
    ] {
        if value["context"].get(key).is_some_and(|v| !v.is_null()) {
            context.insert(key.into(), json!(identity(&value["context"], key)?));
        }
    }
    result["command_identity"] = Value::Object(context);
    result["wrapper_exit"] = json!(expected);
    result["follow_up"] = json!(match expected {
        0 =>
            "Apply ownership and authority checks before further effects; completion does not transfer an assignment.",
        2 => "Inspect the rejection receipt. Do not retry until its cause is resolved.",
        _ =>
            "Stop effects. Inspect the recorded operation and assignment. Do not retry uncertain mutations or recover without exact human approval.",
    });
    let blocked = result["catalog"]["pending_count"]
        .as_u64()
        .is_some_and(|n| n > 0)
        || result["catalog"]["mutations_available"].as_bool() == Some(false)
        || result["catalog"]["phase"]
            .as_str()
            .is_some_and(|s| s != "active");
    result["known_catalog_blocker"] = json!(blocked);
    if blocked {
        result["follow_up"] = json!(
            "Stop effects. Inspect the recorded catalog and pending operations. Preserve ownership and obtain exact human approval before recovery."
        );
    }
    Ok((result, full))
}
pub fn encode(mut result: Value, directory: Option<&Path>) -> Result<Vec<u8>> {
    result["receipt_directory"] = json!(directory);
    let mut bytes = serde_json::to_vec(&result)?;
    if bytes.len() >= MAX_OUTPUT {
        result = json!({"schema_version":1,"wrapper_exit":4,"pool_exit":result["pool_exit"],"pool_outcome":result["pool_outcome"],"capture_complete":result["capture_complete"],"pool_started":result["pool_started"],"receipt_directory":directory,"summary_overflow":true,"follow_up":"Stop effects. Read the complete receipt before further action."});
        bytes = serde_json::to_vec(&result)?;
    }
    ensure!(
        bytes.len() < MAX_OUTPUT,
        "summary cannot fit the output budget"
    );
    bytes.push(b'\n');
    Ok(bytes)
}
