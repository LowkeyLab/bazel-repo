//! Test-only GitHub responses and externally recorded comments.
use std::{fs, io::Write, path::PathBuf};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
fn main() -> Result<()> {
    let path = PathBuf::from(std::env::var_os("REVIEW_REMOTE").context("missing remote fixture")?);
    let mut state: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.first().map(String::as_str) == Some("api")
            && args.get(1).map(String::as_str) == Some("--hostname")
            && args.get(2).map(String::as_str) == Some("github.com"),
        "unexpected gh command"
    );
    let endpoint = args.get(3).context("missing endpoint")?;
    validate_endpoint(endpoint, &state)?;
    let post = args.iter().any(|s| s == "POST");
    ensure!(
        !state["fail_read"].as_bool().unwrap_or(false) || post,
        "controlled read failure"
    );
    let user = json!({"id":42,"login":"reader","type":"User"});
    let mut reads = state["reads"].as_array().cloned().unwrap_or_default();
    if !post {
        reads.push(json!(endpoint));
    }
    state["reads"] = json!(reads);
    let mut fail = false;
    let response = if endpoint == "user" {
        user
    } else if endpoint.contains("/pulls/") {
        let reads = state["pull_reads"].as_u64().unwrap_or(0) + 1;
        state["pull_reads"] = json!(reads);
        let changed = reads.is_multiple_of(2);
        json!({"state":if changed && state["close_before_send"] == true { "closed" } else { state["state"].as_str().unwrap_or("open") }, "head":{"sha":if changed { state["head_before_send"].as_str().or_else(|| state["head"].as_str()) } else { state["head"].as_str() }}})
    } else if endpoint.contains("/check-runs?") {
        state["checks"].clone()
    } else if endpoint.contains("/status?") {
        state["statuses"].clone()
    } else if endpoint.contains("/actions/runs?") {
        state["workflows"].clone()
    } else if post {
        let body = args
            .iter()
            .find_map(|s| s.strip_prefix("body="))
            .context("missing comment body")?;
        let id = state["comments"]
            .as_array()
            .context("missing comments")?
            .len() as u64
            + 100;
        let at = state["now"].as_i64().context("missing clock")?;
        let date = chrono::DateTime::from_timestamp(at, 0)
            .context("invalid clock")?
            .to_rfc3339();
        let comment = json!({"id":id,"user":user,"body":body,"created_at":date});
        let outcome = state["send_outcome"]
            .as_str()
            .unwrap_or("success")
            .to_string();
        if outcome != "lost_without_record" {
            state["comments"]
                .as_array_mut()
                .context("missing comments")?
                .push(comment.clone());
        }
        if outcome != "lost_without_record" {
            state["provider"] = json!("running");
        }
        fail = outcome.starts_with("lost");
        if outcome == "invalid_response" {
            json!({})
        } else {
            comment
        }
    } else if endpoint.contains("/comments?") {
        ensure!(
            args.iter().any(|s| s == "--paginate") && args.iter().any(|s| s == "--slurp"),
            "comments must be paginated"
        );
        let mut comments = state["comments"]
            .as_array()
            .context("missing comments")?
            .clone();
        let provider = if state["pull_reads"].as_u64().unwrap_or(0).is_multiple_of(2)
            && state["complete_before_send"] == true
        {
            Some("completed")
        } else if state["pull_reads"].as_u64().unwrap_or(0).is_multiple_of(2)
            && state["start_before_send"] == true
        {
            Some("running")
        } else {
            state["provider"].as_str()
        };
        if let Some(status) = provider {
            let head = state["provider_head"]
                .as_str()
                .or_else(|| state["head"].as_str())
                .context("missing head")?;
            comments.push(json!({"id":9,"user":{"id":if state["spoof"] == true { 43 } else { 199_175_422 },"login":"chatgpt-codex-connector[bot]","type":"Bot"},"created_at":"1970-01-01T00:00:01Z","body":format!("<!-- codex-review-state:v1 {} -->", json!({"code_review":{"commit_sha":head,"status":if state["omit_provider_status"] == true { Value::Null } else { json!(status) },"has_any_findings":state["findings"].as_bool().unwrap_or(false),"has_unresolved_findings":state["findings"].as_bool().unwrap_or(false)}}))}));
        }
        json!([comments])
    } else {
        anyhow::bail!("unexpected endpoint {endpoint}")
    };
    fs::write(path, serde_json::to_vec(&state)?)?;
    ensure!(!fail, "controlled uncertain send");
    serde_json::to_writer(std::io::stdout().lock(), &response)?;
    std::io::stdout().write_all(b"\n")?;
    Ok(())
}

fn validate_endpoint(endpoint: &str, state: &Value) -> Result<()> {
    if endpoint == "user" {
        return Ok(());
    }
    let repository = state["repository"].as_str().context("missing repository")?;
    let head = state["head"].as_str().context("missing head")?;
    ensure!(
        [
            format!("repos/{repository}/commits/{head}/check-runs?per_page=100"),
            format!("repos/{repository}/commits/{head}/status?per_page=100"),
            format!("repos/{repository}/actions/runs?head_sha={head}&per_page=100"),
            format!("repos/{repository}/pulls/2775"),
            format!("repos/{repository}/issues/2775/comments"),
            format!("repos/{repository}/issues/2775/comments?per_page=100")
        ]
        .iter()
        .any(|expected| expected == endpoint),
        "wrong GitHub endpoint {endpoint}"
    );
    Ok(())
}
