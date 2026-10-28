//! Command behavior uses real private files and a controlled gh process.
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

use anyhow::{Context, Result, ensure};
use googletest::{
    assert_that,
    matchers::{eq, ne},
};
use runfiles::{Runfiles, rlocation};
use serde_json::{Value, json};
const HEAD: &str = "1111111111111111111111111111111111111111";
const NEXT: &str = "2222222222222222222222222222222222222222";
struct Fixture {
    directory: tempfile::TempDir,
    tool: PathBuf,
}
impl Fixture {
    fn new() -> Result<Self> {
        let files = Runfiles::create()?;
        let tool = rlocation!(files, std::env::var("REVIEW_TOOL")?).context("tool runfile")?;
        let fake = rlocation!(files, std::env::var("REVIEW_FAKE_GH")?).context("gh runfile")?;
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("bin"))?;
        std::os::unix::fs::symlink(fake, directory.path().join("bin/gh"))?;
        let f = Self { directory, tool };
        fs::write(
            f.remote(),
            serde_json::to_vec(
                &json!({"repository":"lowkeylab/bazel-repo","head":HEAD,"state":"open","provider":null,"comments":[]}),
            )?,
        )?;
        Ok(f)
    }
    fn wake_fixture() -> Result<Self> {
        let f = Self::new()?;
        fs::create_dir(f.directory.path().join("acquisition"))?;
        fs::write(
            f.directory
                .path()
                .join("acquisition/selected-assignment.json"),
            serde_json::to_vec(
                &json!({"assignment_handle":"837e8948-2ec2-4ddd-ac74-da70e07b8f2f", "operation_id":"dad9ec85-1b1b-46db-86b8-7b65de033516", "repository_id":"207f1904-1165-417f-a4f0-399e8498ed22", "worktree_id":"30e88f2f-0dae-452d-97a8-75a93c769771", "state":"active", "path":{"bytes":[47,255],"display":"informational"}}),
            )?,
        )?;
        f.set("provider", json!("running"))?;
        f.ci("in_progress", &Value::Null)?;
        Ok(f)
    }
    fn ci(&self, status: &str, conclusion: &Value) -> Result<()> {
        self.set("checks", json!([{"total_count":1,"check_runs":[{"id":1,"name":"test","status":status,"conclusion":conclusion}]}]))?;
        self.set("workflows", json!([{"total_count":1,"workflow_runs":[{"id":2,"name":"Bazel Tests","status":status,"conclusion":conclusion}]}]))?;
        self.set("statuses", json!([{"total_count":0,"statuses":[]}]))
    }
    fn wake(&self, now: u64, decision: &str, status: &str) -> Result<Value> {
        let value = self.ok("wake", now, HEAD)?;
        assert_that!(value["decision"].as_str(), eq(Some(decision)));
        assert_that!(value["status"].as_str(), eq(Some(status)));
        let xml = value["heartbeat"].as_str().context("heartbeat")?;
        assert_that!(
            xml.contains(&format!("<decision>{decision}</decision>")),
            eq(true)
        );
        assert_that!(xml.starts_with("<heartbeat>\n"), eq(true));
        assert_that!(xml.ends_with("</heartbeat>"), eq(true));
        Ok(value)
    }
    fn remote(&self) -> PathBuf {
        self.directory.path().join("remote.json")
    }
    fn data(&self) -> Result<Value> {
        Ok(serde_json::from_slice(&fs::read(self.remote())?)?)
    }
    fn set(&self, key: &str, value: Value) -> Result<()> {
        let mut data = self.data()?;
        data[key] = value;
        fs::write(self.remote(), serde_json::to_vec(&data)?)?;
        Ok(())
    }
    fn sends(&self) -> Result<usize> {
        Ok(self.data()?["comments"]
            .as_array()
            .context("comments")?
            .len())
    }
    fn run(&self, action: &str, now: u64, head: &str) -> Result<Output> {
        self.run_for_repository(action, now, head, "LowkeyLab/bazel-repo")
    }
    fn run_for_repository(
        &self,
        action: &str,
        now: u64,
        head: &str,
        repository: &str,
    ) -> Result<Output> {
        self.set("now", json!(now))?;
        let mut command = Command::new(&self.tool);
        if action == "wake" {
            command
                .args([
                    "--automation-id",
                    "implement-tickets",
                    "--assignment-receipt",
                ])
                .arg(self.directory.path().join("acquisition"));
        }
        if action == "track" {
            command.args(["--request-comment", "50"]);
        }
        Ok(command
            .args([
                action,
                "--repo",
                repository,
                "--pr",
                "2775",
                "--head",
                head,
                "--now",
                &now.to_string(),
                "--state-dir",
                "state",
            ])
            .current_dir(self.directory.path())
            .env("PATH", self.directory.path().join("bin"))
            .env("REVIEW_REMOTE", self.remote())
            .output()?)
    }
    fn ok(&self, action: &str, now: u64, head: &str) -> Result<Value> {
        let output = self.run(action, now, head)?;
        ensure!(
            output.status.success(),
            "command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    fn status(&self, action: &str, now: u64, head: &str, expected: &str) -> Result<()> {
        assert_that!(
            self.ok(action, now, head)?["status"].as_str(),
            eq(Some(expected))
        );
        Ok(())
    }
}
#[googletest::test]
fn boundary_and_restart_preserve_one_request_per_eligible_interval() -> Result<()> {
    let f = Fixture::new()?;
    let first = f.ok("request", 100, HEAD)?;
    for now in [100, 3699, 3700] {
        f.status("poll", now, HEAD, "waiting")?;
    }
    assert_that!(f.sends()?, eq(1));
    let second = f.ok("poll", 3701, HEAD)?;
    assert_that!(second["status"].as_str(), eq(Some("requested")));
    assert_that!(&first["request_id"], ne(&second["request_id"]));
    f.status("poll", 3701, HEAD, "waiting")?;
    f.status("request", 3701, HEAD, "waiting")?;
    f.status("poll", 7301, HEAD, "waiting")?;
    assert_that!(f.sends()?, eq(2));
    Ok(())
}
#[googletest::test]
fn completion_closure_and_missing_provider_stop_retries() -> Result<()> {
    let f = Fixture::new()?;
    f.ok("request", 100, HEAD)?;
    f.set("provider", json!("completed"))?;
    f.status("poll", 4000, HEAD, "complete")?;
    f.set("provider", Value::Null)?;
    f.status("poll", 4000, HEAD, "waiting")?;
    f.set("state", json!("closed"))?;
    f.status("poll", 4000, HEAD, "closed")?;
    assert_that!(f.sends()?, eq(1));
    Ok(())
}
#[googletest::test]
fn changed_head_needs_a_separate_request_and_ignores_old_completion() -> Result<()> {
    let f = Fixture::new()?;
    f.ok("request", 100, HEAD)?;
    f.set("head", json!(NEXT))?;
    f.set("provider_head", json!(HEAD))?;
    f.set("provider", json!("completed"))?;
    f.status("poll", 4000, HEAD, "head_changed")?;
    f.status("poll", 4000, NEXT, "untracked_head")?;
    f.status("request", 4000, NEXT, "requested")?;
    assert_that!(f.sends()?, eq(2));
    Ok(())
}
#[googletest::test]
fn lost_response_reconciles_the_same_request_without_another_send() -> Result<()> {
    for outcome in ["lost_with_record", "invalid_response"] {
        let f = Fixture::new()?;
        f.set("send_outcome", json!(outcome))?;
        assert_that!(f.run("request", 100, HEAD)?.status.success(), eq(false));
        f.status("poll", 101, HEAD, "reconciled")?;
        f.status("poll", 101, HEAD, "waiting")?;
        assert_that!(f.sends()?, eq(1));
    }
    Ok(())
}
#[googletest::test]
fn absent_remote_match_does_not_authorize_resending_uncertain_intent() -> Result<()> {
    let f = Fixture::new()?;
    f.set("send_outcome", json!("lost_without_record"))?;
    assert_that!(f.run("request", 100, HEAD)?.status.success(), eq(false));
    f.set("send_outcome", json!("success"))?;
    for action in ["poll", "request"] {
        f.status(action, 10000, HEAD, "uncertain")?;
    }
    assert_that!(f.sends()?, eq(0));
    Ok(())
}
#[googletest::test]
fn failed_reads_and_forged_provider_state_do_not_establish_timeout() -> Result<()> {
    let f = Fixture::new()?;
    f.ok("request", 100, HEAD)?;
    f.set("fail_read", json!(true))?;
    assert_that!(f.run("poll", 4000, HEAD)?.status.success(), eq(false));
    f.set("fail_read", json!(false))?;
    f.set("spoof", json!(true))?;
    f.status("poll", 4000, HEAD, "waiting")?;
    assert_that!(f.sends()?, eq(1));
    Ok(())
}
#[googletest::test]
fn final_pr_and_provider_reads_prevent_stale_sends() -> Result<()> {
    for key in [
        "close_before_send",
        "head_before_send",
        "complete_before_send",
    ] {
        let f = Fixture::new()?;
        f.set(
            key,
            if key == "head_before_send" {
                json!(NEXT)
            } else {
                json!(true)
            },
        )?;
        let status = if key == "complete_before_send" {
            "complete"
        } else {
            "changed_before_send"
        };
        f.status("request", 100, HEAD, status)?;
        assert_that!(f.sends()?, eq(0));
    }
    Ok(())
}
#[googletest::test]
fn state_inside_a_checkout_is_rejected_before_any_request() -> Result<()> {
    let f = Fixture::new()?;
    fs::create_dir(f.directory.path().join(".git"))?;
    assert_that!(f.run("request", 100, HEAD)?.status.success(), eq(false));
    assert_that!(f.sends()?, eq(0));
    Ok(())
}

#[googletest::test]
fn unknown_status_and_external_requests_prevent_automatic_retries() -> Result<()> {
    let f = Fixture::new()?;
    f.ok("request", 100, HEAD)?;
    f.set("provider", json!("unknown_status"))?;
    assert_that!(f.run("poll", 4000, HEAD)?.status.success(), eq(false));
    f.set("provider", json!("running"))?;
    let mut data = f.data()?;
    data["comments"].as_array_mut().context("comments")?.push(json!({"id":200,"user":{"id":43,"login":"another-reader","type":"User"},"created_at":"1970-01-01T00:03:20Z","body":"@codex review"}));
    fs::write(f.remote(), serde_json::to_vec(&data)?)?;
    f.status("poll", 4000, HEAD, "external_request")?;
    assert_that!(f.sends()?, eq(2));
    Ok(())
}

#[googletest::test]
fn an_existing_review_without_request_history_cannot_be_restarted() -> Result<()> {
    let f = Fixture::new()?;
    f.set("provider", json!("running"))?;
    f.status("request", 4000, HEAD, "untracked_head")?;
    assert_that!(f.sends()?, eq(0));
    Ok(())
}

#[googletest::test]
fn an_explicit_existing_request_can_be_tracked_without_sending() -> Result<()> {
    let f = Fixture::new()?;
    f.set("provider", json!("running"))?;
    f.set("comments",json!([{"id":50,"user":{"id":42,"login":"reader","type":"User"},"created_at":"1970-01-01T00:01:40Z","body":format!("@codex review\nReview commit `{HEAD}`.")}]))?;
    f.status("track", 100, HEAD, "tracked")?;
    f.status("track", 200, HEAD, "already_tracked")?;
    f.status("poll", 3700, HEAD, "waiting")?;
    f.status("poll", 3701, HEAD, "requested")?;
    assert_that!(f.sends()?, eq(2));
    Ok(())
}

#[googletest::test]
fn an_automatic_review_start_before_send_prevents_an_initial_request() -> Result<()> {
    let f = Fixture::new()?;
    f.set("start_before_send", json!(true))?;
    f.status("request", 100, HEAD, "provider_unavailable_before_send")?;
    assert_that!(f.sends()?, eq(0));
    Ok(())
}

#[googletest::test]
fn repository_components_do_not_change_the_pull_endpoint() -> Result<()> {
    let f = Fixture::new()?;
    f.set("repository", json!("issues/issues"))?;
    let output = f.run_for_repository("request", 100, HEAD, "issues/issues")?;
    ensure!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout)?;
    assert_that!(result["status"].as_str(), eq(Some("requested")));
    assert_that!(f.sends()?, eq(1));
    Ok(())
}

#[googletest::test]
fn scheduled_wakes_remain_quiet_across_restarts_and_running_changes() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("provider", Value::Null)?;
    f.ok("request", 100, HEAD)?;
    for now in [101, 102, 103] {
        f.wake(now, "DONT_NOTIFY", "waiting")?;
    }
    f.ci("queued", &Value::Null)?;
    f.wake(104, "DONT_NOTIFY", "waiting")?;
    f.ci("completed", &json!("success"))?;
    f.wake(105, "DONT_NOTIFY", "waiting")?;
    assert_that!(f.sends()?, eq(1));
    let saved: Value = serde_json::from_slice(&fs::read(
        f.directory
            .path()
            .join("state/lowkeylab/bazel-repo/2775/wake.json"),
    )?)?;
    assert_that!(
        saved["assignment"]["path"]["bytes"][1].as_u64(),
        eq(Some(255))
    );
    assert_that!(
        f.directory
            .path()
            .join("acquisition/selected-assignment.json")
            .exists(),
        eq(true)
    );
    Ok(())
}

#[googletest::test]
fn one_wake_reads_each_inventory_once_without_a_polling_loop() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("provider", json!("completed"))?;
    f.wake(100, "DONT_NOTIFY", "waiting")?;
    let data = f.data()?;
    let reads = data["reads"].as_array().context("reads")?;
    assert_that!(reads.len(), eq(5));
    let unique: std::collections::BTreeSet<_> = reads.iter().map(Value::to_string).collect();
    assert_that!(unique.len(), eq(5));
    Ok(())
}

#[googletest::test]
fn completion_failure_findings_and_closure_notify_once_per_transition() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("provider", json!("completed"))?;
    f.wake(100, "DONT_NOTIFY", "waiting")?;
    f.ci("completed", &json!("failure"))?;
    f.wake(101, "NOTIFY", "failure")?;
    f.wake(102, "DONT_NOTIFY", "failure")?;
    f.ci("completed", &json!("success"))?;
    f.set("findings", json!(true))?;
    f.wake(103, "NOTIFY", "review_findings")?;
    f.wake(104, "DONT_NOTIFY", "review_findings")?;
    f.set("findings", json!(false))?;
    f.wake(105, "NOTIFY", "external_work_complete")?;
    f.wake(106, "DONT_NOTIFY", "external_work_complete")?;
    f.set("state", json!("closed"))?;
    f.wake(107, "NOTIFY", "closed")?;
    f.wake(108, "DONT_NOTIFY", "closed")?;
    Ok(())
}

#[googletest::test]
fn failed_or_incomplete_reads_are_uncertain_and_never_trigger_requests() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("provider", Value::Null)?;
    f.ok("request", 100, HEAD)?;
    f.wake(101, "DONT_NOTIFY", "waiting")?;
    f.set("fail_read", json!(true))?;
    f.wake(4000, "NOTIFY", "uncertain")?;
    f.wake(4001, "DONT_NOTIFY", "uncertain")?;
    f.set("fail_read", json!(false))?;
    f.set("checks", json!([{"total_count":2,"check_runs":[]}]))?;
    f.wake(4002, "DONT_NOTIFY", "uncertain")?;
    assert_that!(f.sends()?, eq(1));
    f.ci("in_progress", &Value::Null)?;
    f.set("provider", json!("completed"))?;
    f.wake(4003, "NOTIFY", "waiting")?;
    Ok(())
}

#[googletest::test]
fn scheduled_timeout_uses_shared_request_history_and_strict_boundary() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("provider", Value::Null)?;
    f.ok("request", 100, HEAD)?;
    f.wake(3700, "DONT_NOTIFY", "waiting")?;
    f.wake(3701, "NOTIFY", "review_requested")?;
    f.wake(3702, "DONT_NOTIFY", "waiting")?;
    assert_that!(f.sends()?, eq(2));
    f.set("send_outcome", json!("lost_without_record"))?;
    f.wake(7302, "NOTIFY", "uncertain")?;
    f.wake(7303, "NOTIFY", "action_required")?;
    f.wake(7304, "DONT_NOTIFY", "action_required")?;
    assert_that!(f.sends()?, eq(2));
    Ok(())
}

#[googletest::test]
fn changed_head_and_missing_provider_require_inspection_without_releasing_work() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("head", json!(NEXT))?;
    f.wake(100, "NOTIFY", "head_changed")?;
    f.wake(101, "DONT_NOTIFY", "head_changed")?;
    f.set("head", json!(HEAD))?;
    f.set("provider", Value::Null)?;
    f.wake(102, "NOTIFY", "action_required")?;
    assert_that!(f.sends()?, eq(0));
    Ok(())
}

#[googletest::test]
fn incomplete_or_duplicate_pages_do_not_establish_completion() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("provider", json!("completed"))?;
    f.ci("completed", &json!("success"))?;
    let record = json!({"id":1,"name":"test","status":"completed","conclusion":"success"});
    f.set(
        "checks",
        json!([{"total_count":2,"check_runs":[record]},{"total_count":2,"check_runs":[record]}]),
    )?;
    f.wake(100, "NOTIFY", "uncertain")?;
    f.set("checks", json!([{"total_count":1,"check_runs":[]}]))?;
    f.wake(101, "DONT_NOTIFY", "uncertain")?;
    assert_that!(f.sends()?, eq(0));
    Ok(())
}

#[googletest::test]
fn receipt_changes_and_backwards_time_stop_before_remote_effects() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("provider", json!("completed"))?;
    f.wake(100, "DONT_NOTIFY", "waiting")?;
    assert_that!(f.run("wake", 99, HEAD)?.status.success(), eq(false));
    let path = f
        .directory
        .path()
        .join("acquisition/selected-assignment.json");
    let mut assignment: Value = serde_json::from_slice(&fs::read(&path)?)?;
    assignment["assignment_handle"] = json!("92f2c89c-8563-40c0-ac70-1ab05c022af3");
    fs::write(path, serde_json::to_vec(&assignment)?)?;
    assert_that!(f.run("wake", 101, HEAD)?.status.success(), eq(false));
    assert_that!(f.sends()?, eq(0));
    Ok(())
}

#[googletest::test]
fn a_new_reviewed_commit_has_a_separate_completion_transition() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("provider", json!("completed"))?;
    f.ci("completed", &json!("success"))?;
    f.wake(100, "NOTIFY", "external_work_complete")?;
    f.set("head", json!(NEXT))?;
    let value = f.ok("wake", 101, NEXT)?;
    assert_that!(value["status"].as_str(), eq(Some("external_work_complete")));
    assert_that!(value["decision"].as_str(), eq(Some("NOTIFY")));
    assert_that!(
        f.ok("wake", 102, NEXT)?["decision"].as_str(),
        eq(Some("DONT_NOTIFY"))
    );
    Ok(())
}

#[googletest::test]
fn a_cleared_failure_and_a_later_failure_each_notify_once() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("provider", json!("completed"))?;
    f.ci("completed", &json!("failure"))?;
    f.wake(100, "NOTIFY", "failure")?;
    f.ci("in_progress", &Value::Null)?;
    f.wake(101, "NOTIFY", "waiting")?;
    f.wake(102, "DONT_NOTIFY", "waiting")?;
    f.ci("completed", &json!("failure"))?;
    f.wake(103, "NOTIFY", "failure")?;
    Ok(())
}

#[googletest::test]
fn malformed_provider_status_cannot_authorize_an_initial_request() -> Result<()> {
    let f = Fixture::wake_fixture()?;
    f.set("omit_provider_status", json!(true))?;
    assert_that!(f.run("request", 100, HEAD)?.status.success(), eq(false));
    f.wake(101, "NOTIFY", "uncertain")?;
    assert_that!(f.sends()?, eq(0));
    Ok(())
}
