//! Observe one scheduled wake and retain notification decisions across restarts.
use std::{fs::File, path::Path};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

use crate::{
    Cli,
    github::{Github, review, review_state},
    review_action,
    store::{Commit, State, Store},
};
use policy::Review;

pub fn run(
    cli: &Cli,
    head: Commit,
    store: &Store,
    saved: &mut State,
    gh: &Github,
) -> Result<Value> {
    let automation = cli
        .automation_id
        .as_deref()
        .context("automation-id is required")?;
    ensure!(
        !automation.is_empty()
            && automation
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b)),
        "invalid automation identity"
    );
    let receipt = cli
        .assignment_receipt
        .as_deref()
        .context("assignment receipt is required")?;
    let assignment = retained_assignment(receipt)?;
    let previous = store.read_wake()?;
    if let Some(old) = &previous {
        ensure!(
            old["version"] == 1
                && old["automation_id"] == automation
                && old["assignment_receipt"]
                    == receipt.to_str().context("receipt path must be UTF-8")?
                && old["assignment"] == assignment,
            "wake identity changed. Retain the original assignment receipt and automation identity"
        );
        ensure!(
            old["observed_at"].as_u64().is_some_and(|at| cli.now >= at),
            "observation time moved backwards"
        );
        ensure!(old["condition"].is_object(), "invalid saved wake condition");
    }
    let expected_head = head.as_str().to_owned();
    let observation = observe(cli, head, store, saved, gh);
    let (snapshot, mut condition) = match observation {
        Ok(value) => value,
        Err(error) => (
            json!({"status":"uncertain", "detail":format!("{error:#}")}),
            json!({"kind":"uncertain"}),
        ),
    };
    condition["expected_head"] = json!(expected_head);
    let changed = previous
        .as_ref()
        .is_none_or(|old| old["condition"] != condition);
    let restored = condition["kind"] == "waiting"
        && previous.as_ref().is_some_and(|old| {
            matches!(
                old["condition"]["kind"].as_str(),
                Some(
                    "uncertain"
                        | "failure"
                        | "review_findings"
                        | "head_changed"
                        | "action_required"
                        | "review_unavailable"
                )
            )
        });
    let notify = (changed && condition["kind"] != "waiting") || restored;
    let decision = if notify { "NOTIFY" } else { "DONT_NOTIFY" };
    let message = message(
        condition["kind"].as_str().context("missing condition")?,
        restored,
    );
    let record = json!({"version":1, "automation_id":automation, "assignment_receipt":receipt, "assignment":assignment, "observed_at":cli.now, "condition":condition, "observation":snapshot, "last_successful_observation":if snapshot["status"] == "uncertain" { previous.as_ref().map_or(Value::Null, |old| old["last_successful_observation"].clone()) } else { snapshot }, "decision":decision});
    // Persist the decision before returning it. A restart cannot repeat this transition.
    store.save_wake(&record)?;
    Ok(
        json!({"status":record["condition"]["kind"], "decision":decision, "message":message, "observation":record["observation"], "assignment_receipt":receipt, "heartbeat":format!("<heartbeat>\n  <automation_id>{automation}</automation_id>\n  <decision>{decision}</decision>\n  <message>{message}</message>\n</heartbeat>")}),
    )
}

fn retained_assignment(receipt: &Path) -> Result<Value> {
    ensure!(receipt.is_absolute(), "assignment receipt must be absolute");
    let selected: Value =
        serde_json::from_reader(File::open(receipt.join("selected-assignment.json"))?)?;
    for key in [
        "assignment_handle",
        "operation_id",
        "repository_id",
        "worktree_id",
    ] {
        let id = selected[key]
            .as_str()
            .context("missing assignment identity")?;
        ensure!(
            !uuid::Uuid::parse_str(id)?.is_nil(),
            "invalid assignment identity"
        );
    }
    ensure!(
        selected["path"]["bytes"]
            .as_array()
            .is_some_and(|bytes| !bytes.is_empty()
                && bytes.iter().all(|b| b.as_u64().is_some_and(|n| n <= 255))),
        "missing lossless assignment path"
    );
    Ok(selected)
}

fn observe(
    cli: &Cli,
    head: Commit,
    store: &Store,
    saved: &mut State,
    gh: &Github,
) -> Result<(Value, Value)> {
    let pull = gh.pull()?;
    ensure!(
        matches!(pull.state.as_str(), "open" | "closed"),
        "unknown PR state"
    );
    if pull.state == "closed" {
        return Ok((
            json!({"status":"closed", "head":pull.head.sha}),
            json!({"kind":"closed", "head":pull.head.sha}),
        ));
    }
    if pull.head.sha != head.as_str() {
        return Ok((
            json!({"status":"head_changed", "head":pull.head.sha}),
            json!({"kind":"head_changed", "head":pull.head.sha}),
        ));
    }
    let comments = gh.comments()?;
    let provider = review_state(&comments, head.as_str())?;
    let review_kind = review(&comments, head.as_str())?;
    let checks = gh.evidence(head.as_str(), "checks")?;
    let statuses = gh.evidence(head.as_str(), "statuses")?;
    let workflows = gh.evidence(head.as_str(), "workflows")?;
    let (pending, failures) = ci_status(&checks, &statuses, &workflows)?;
    let result = review_action(cli, head, store, saved, gh, &pull, &comments)?;
    let status = result["status"]
        .as_str()
        .context("missing review action status")?;
    let findings = if review_kind == Review::Complete {
        let provider = provider
            .as_ref()
            .context("missing completed provider state")?;
        let any = provider["has_any_findings"]
            .as_bool()
            .context("missing findings status")?;
        let unresolved = provider["has_unresolved_findings"]
            .as_bool()
            .context("missing unresolved findings status")?;
        any || unresolved
    } else {
        false
    };
    let condition = if !failures.is_empty() {
        json!({"kind":"failure", "failures":failures})
    } else if findings {
        json!({"kind":"review_findings"})
    } else if status == "requested" {
        json!({"kind":"review_requested", "request_id":result["request_id"]})
    } else if !matches!(status, "waiting" | "complete" | "reconciled") {
        json!({"kind":"action_required", "review_status":status})
    } else if review_kind == Review::Missing {
        json!({"kind":"review_unavailable"})
    } else if !pending && review_kind == Review::Complete {
        json!({"kind":"external_work_complete"})
    } else {
        json!({"kind":"waiting"})
    };
    Ok((
        json!({"status":"observed", "head":pull.head.sha,"review":provider,"review_action":result,"checks":checks,"statuses":statuses,"workflows":workflows}),
        condition,
    ))
}

fn ci_status(checks: &Value, statuses: &Value, workflows: &Value) -> Result<(bool, Vec<Value>)> {
    let mut pending = false;
    let mut failures = Vec::new();
    for (pages, field, count, category) in [
        (checks, "check_runs", "total_count", "checks"),
        (statuses, "statuses", "total_count", "statuses"),
        (workflows, "workflow_runs", "total_count", "workflows"),
    ] {
        let records = complete_pages(pages, field, count)?;
        if category != "statuses" && records.is_empty() {
            pending = true;
        }
        let mut identities = std::collections::BTreeSet::new();
        for record in records {
            let identity = if category == "statuses" {
                record["context"]
                    .as_str()
                    .context("missing status context")?
                    .to_owned()
            } else {
                record["id"]
                    .as_u64()
                    .filter(|id| *id > 0)
                    .context("missing check identity")?
                    .to_string()
            };
            ensure!(identities.insert(identity), "duplicate response identity");
            let name = record[if category == "statuses" {
                "context"
            } else {
                "name"
            }]
            .as_str()
            .context("missing check name")?;
            let state = record[if category == "statuses" {
                "state"
            } else {
                "status"
            }]
            .as_str()
            .context("missing check status")?;
            let conclusion = if category == "statuses" {
                state
            } else if state == "completed" {
                record["conclusion"]
                    .as_str()
                    .context("missing check conclusion")?
            } else {
                state
            };
            match conclusion {
                "success" | "skipped" | "neutral" => {}
                "queued" | "pending" | "in_progress" | "requested" | "waiting" => pending = true,
                "failure" | "error" | "cancelled" | "timed_out" | "action_required"
                | "startup_failure" | "stale" => {
                    failures.push(json!({"category":category,"name":name,"conclusion":conclusion}));
                }
                _ => anyhow::bail!("unknown check state"),
            }
        }
    }
    failures.sort_by_key(Value::to_string);
    Ok((pending, failures))
}

fn complete_pages<'a>(pages: &'a Value, field: &str, count: &str) -> Result<Vec<&'a Value>> {
    let pages = pages.as_array().context("missing response pages")?;
    ensure!(!pages.is_empty(), "empty response pages");
    let total = pages[0][count].as_u64().context("missing total count")?;
    let mut records = Vec::new();
    for page in pages {
        ensure!(
            page[count].as_u64() == Some(total),
            "response count changed between pages"
        );
        records.extend(
            page[field]
                .as_array()
                .context("missing response inventory")?,
        );
    }
    ensure!(
        u64::try_from(records.len())? == total,
        "incomplete response pages"
    );
    Ok(records)
}

fn message(kind: &str, restored: bool) -> &'static str {
    if restored {
        return "The previous blocking condition cleared. External work remains pending.";
    }
    match kind {
        "waiting" => "External work remains pending. No user action is required.",
        "uncertain" => {
            "An observation failed or returned incomplete evidence. Inspect the retained state before further effects."
        }
        "closed" => "The PR is closed. Check delivery evidence before ending the task.",
        "head_changed" => "The PR commit changed. Review the new commit before further effects.",
        "failure" => "An external check failed. Inspect the retained observation.",
        "review_findings" => {
            "Hosted review reported findings. Inspect the review before further effects."
        }
        "review_requested" => "The shared timeout rule requested another hosted review.",
        "review_unavailable" => {
            "Current hosted review evidence is unavailable. Inspect the provider state."
        }
        "external_work_complete" => {
            "External work completed. Run the exact-head merge verifier before merge."
        }
        _ => {
            "The review requires inspection. Pending work and assignment receipts remain preserved."
        }
    }
}
