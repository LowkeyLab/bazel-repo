//! Command behavior uses real private files and a controlled gh process.
use anyhow::{Context, Result, ensure};
use googletest::{
    assert_that,
    matchers::{eq, ne},
};
use runfiles::{Runfiles, rlocation};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};
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
            serde_json::to_vec(&json!({"head":HEAD,"state":"open","provider":null,"comments":[]}))?,
        )?;
        Ok(f)
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
        self.set("now", json!(now))?;
        let mut command = Command::new(&self.tool);
        if action == "track" {
            command.args(["--request-comment", "50"]);
        }
        Ok(command
            .args([
                action,
                "--repo",
                "LowkeyLab/bazel-repo",
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
