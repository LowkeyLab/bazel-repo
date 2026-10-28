//! Exercise the real wrapper with a controlled process and private filesystem.
use std::{
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::PathBuf,
    process::Command,
};

use anyhow::{Context, Result, ensure};
use googletest::{assert_that, matchers::eq};
use runfiles::{Runfiles, rlocation};
use serde_json::{Value, json};
const HANDLE: &str = "11111111-1111-1111-1111-111111111111";
const OTHER: &str = "22222222-2222-2222-2222-222222222222";
const OP: &str = "33333333-3333-3333-3333-333333333333";
const REPO: &str = "44444444-4444-4444-4444-444444444444";
const WORKTREE: &str = "55555555-5555-5555-5555-555555555555";
const CATALOG: &str = "66666666-6666-6666-6666-666666666666";
fn assignment(handle: &str, state: &str) -> Value {
    json!({"assignment_handle":handle,"operation_id":OP,"repository_id":REPO,"worktree_id":WORKTREE,"state":state,"path":{"bytes":[47,116,109,112,47,255,97],"display":"/tmp/�a","encoding":"unix-bytes"}})
}
fn catalog(outcome: &str) -> Value {
    json!({"schema_version":1,"command":"catalog info","outcome":outcome,"reason_code":"controlled","context":{"catalog_id":CATALOG},"data":{"authority":{"catalog_id":CATALOG,"phase":"active"},"catalog_mutations_available":true,"operations":[],"assignments":[assignment(HANDLE,"released"),assignment(OTHER,"active")]}})
}
struct Fixture {
    directory: tempfile::TempDir,
    tool: PathBuf,
    receipt_root: PathBuf,
}
impl Fixture {
    fn new(value: &Value, exit: i32) -> Result<Self> {
        let files = Runfiles::create()?;
        let tool = rlocation!(files, std::env::var("POOL_WRAPPER")?).context("wrapper")?;
        let fake = rlocation!(files, std::env::var("POOL_FAKE")?).context("pool")?;
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("bin"))?;
        std::os::unix::fs::symlink(fake, directory.path().join("bin/worktree-pool"))?;
        let receipt_root = directory.path().join("receipts");
        let f = Self {
            directory,
            tool,
            receipt_root,
        };
        f.response(&serde_json::to_vec(value)?, b"", exit, false)?;
        Ok(f)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }
    fn response(&self, out: &[u8], err: &[u8], exit: i32, damage: bool) -> Result<()> {
        fs::write(
            self.path("response.json"),
            serde_json::to_vec(
                &json!({"stdout":out,"stderr":err,"exit":exit,"damage_status":damage}),
            )?,
        )?;
        Ok(())
    }
    fn run(&self, selected: Option<&str>, args: &[OsString]) -> Result<(Value, i32)> {
        let mut command = Command::new(&self.tool);
        command
            .args(["--task", "issue-2778", "--receipt-dir"])
            .arg(&self.receipt_root);
        if let Some(handle) = selected {
            command.args(["--assignment", handle]);
        }
        let path = std::env::join_paths(std::iter::once(self.path("bin")).chain(
            std::env::split_paths(&std::env::var_os("PATH").context("PATH")?),
        ))?;
        let output = command
            .arg("--")
            .args(args)
            .env("PATH", path)
            .env("POOL_TEST_RESPONSE", self.path("response.json"))
            .current_dir(self.directory.path())
            .output()?;
        assert_that!(output.stdout.len() <= 8192, eq(true));
        let value: Value = serde_json::from_slice(&output.stdout).with_context(|| {
            format!(
                "wrapper output: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
        let code = output.status.code().context("wrapper status")?;
        assert_that!(value["wrapper_exit"].as_i64(), eq(Some(i64::from(code))));
        Ok((value, code))
    }
    fn observe(&self, selected: Option<&str>) -> Result<(Value, i32)> {
        self.run(selected, &["catalog".into(), "info".into()])
    }
    fn receipt(value: &Value) -> Result<PathBuf> {
        Ok(PathBuf::from(
            value["receipt_directory"]
                .as_str()
                .context("receipt directory")?,
        ))
    }
    fn calls(&self) -> Result<usize> {
        Ok(if self.path("calls").exists() {
            fs::read_to_string(self.path("calls"))?.lines().count()
        } else {
            0
        })
    }
}
#[googletest::test]
fn large_catalog_output_is_bounded_and_never_selects_the_latest_assignment() -> Result<()> {
    let mut data = catalog("completed");
    data["data"]["noise"] = json!("sensitive-catalog-data".repeat(60_000));
    let f = Fixture::new(&data, 0)?;
    let (value, code) = f.observe(None)?;
    assert_that!(code, eq(0));
    assert_that!(value["assignment"].is_null(), eq(true));
    assert_that!(
        value["selection_status"].as_str(),
        eq(Some("not_requested"))
    );
    let receipt = Fixture::receipt(&value)?;
    assert_that!(
        fs::read(receipt.join("stdout"))?,
        eq(&serde_json::to_vec(&data)?)
    );
    assert_that!(
        value.to_string().contains("sensitive-catalog-data"),
        eq(false)
    );
    assert_that!(f.calls()?, eq(1));
    Ok(())
}
#[googletest::test]
fn selected_identity_and_non_utf8_path_bytes_remain_exact() -> Result<()> {
    let mut data = catalog("completed");
    data["context"]["operation_id"] = json!(OTHER);
    let f = Fixture::new(&data, 0)?;
    let (value, code) = f.observe(Some(HANDLE))?;
    assert_that!(value["assignment"]["operation_id"].as_str(), eq(Some(OP)));
    assert_that!(
        value["assignment"]["repository_id"].as_str(),
        eq(Some(REPO))
    );
    assert_that!(
        value["assignment"]["worktree_id"].as_str(),
        eq(Some(WORKTREE))
    );
    assert_that!(
        value["command_identity"]["operation_id"].as_str(),
        eq(Some(OTHER))
    );
    assert_that!(code, eq(0));
    assert_that!(
        value["assignment"]["assignment_handle"].as_str(),
        eq(Some(HANDLE))
    );
    assert_that!(value["assignment"]["state"].as_str(), eq(Some("released")));
    let receipt = Fixture::receipt(&value)?;
    let selected: Value =
        serde_json::from_slice(&fs::read(receipt.join("selected-assignment.json"))?)?;
    assert_that!(selected, eq(&assignment(HANDLE, "released")));
    assert_that!(
        value["assignment"]["path"]["display_informational"].as_str(),
        eq(Some("/tmp/�a"))
    );
    let meta: Value = serde_json::from_slice(&fs::read(receipt.join("invocation.json"))?)?;
    assert_that!(meta["task"].as_str(), eq(Some("issue-2778")));
    assert_that!(meta["selected_assignment"].as_str(), eq(Some(HANDLE)));
    Ok(())
}
#[googletest::test]
fn every_pool_outcome_keeps_its_exit_and_follow_up_requirements() -> Result<()> {
    for (outcome, exit) in [
        ("completed", 0),
        ("rejected", 2),
        ("pending", 3),
        ("unknown", 4),
    ] {
        let f = Fixture::new(&catalog(outcome), exit)?;
        let (value, code) = f.observe(None)?;
        assert_that!(code, eq(exit));
        assert_that!(value["pool_exit"].as_i64(), eq(Some(i64::from(exit))));
        assert_that!(value["pool_outcome"].as_str(), eq(Some(outcome)));
        let action = value["follow_up"].as_str().context("follow-up")?;
        if exit >= 3 {
            assert_that!(
                action.contains("Do not retry uncertain mutations"),
                eq(true)
            );
        }
        assert_that!(f.calls()?, eq(1));
    }
    Ok(())
}
#[googletest::test]
fn partial_bytes_and_stderr_are_retained_before_parse_failure() -> Result<()> {
    let f = Fixture::new(&catalog("pending"), 3)?;
    f.response(b"{partial\xff", b"private-stderr\x00\xff", 3, false)?;
    let (value, code) = f.observe(None)?;
    assert_that!(code, eq(4));
    assert_that!(value["pool_exit"].as_i64(), eq(Some(3)));
    let receipt = Fixture::receipt(&value)?;
    assert_that!(
        fs::read(receipt.join("stdout"))?,
        eq(&b"{partial\xff".to_vec())
    );
    assert_that!(
        fs::read(receipt.join("stderr"))?,
        eq(&b"private-stderr\x00\xff".to_vec())
    );
    let exit: Value = serde_json::from_slice(&fs::read(receipt.join("exit-status.json"))?)?;
    assert_that!(exit["code"].as_i64(), eq(Some(3)));
    assert_that!(f.calls()?, eq(1));
    Ok(())
}
#[googletest::test]
fn receipt_permissions_and_child_umask_are_private() -> Result<()> {
    let f = Fixture::new(&catalog("completed"), 0)?;
    let (value, _) = f.observe(Some(HANDLE))?;
    let receipt = Fixture::receipt(&value)?;
    assert_that!(
        fs::metadata(&receipt)?.permissions().mode() & 0o777,
        eq(0o700)
    );
    for name in [
        "invocation.json",
        "stdout",
        "stderr",
        "exit-status.json",
        "selected-assignment.json",
    ] {
        assert_that!(
            fs::metadata(receipt.join(name))?.permissions().mode() & 0o777,
            eq(0o600)
        );
    }
    assert_that!(
        fs::metadata(f.path("mask-marker"))?.permissions().mode() & 0o777,
        eq(0o600)
    );
    assert_that!(
        fs::metadata(f.path("mask-directory"))?.permissions().mode() & 0o777,
        eq(0o700)
    );
    Ok(())
}
#[googletest::test]
fn pending_inventory_is_bounded_and_explicit_about_omitted_records() -> Result<()> {
    let mut data = catalog("completed");
    data["data"]["operations"]=json!((1..=500).map(|i|json!({"operation_id":format!("00000000-0000-0000-0000-{i:012x}"),"state":"preparing"})).collect::<Vec<_>>());
    let f = Fixture::new(&data, 0)?;
    let (value, code) = f.observe(None)?;
    assert_that!(code, eq(0));
    assert_that!(value["catalog"]["pending_count"].as_u64(), eq(Some(500)));
    assert_that!(
        value["catalog"]["pending_operations"]
            .as_array()
            .context("pending")?
            .len(),
        eq(8)
    );
    assert_that!(
        value["catalog"]["pending_summary_truncated"].as_bool(),
        eq(Some(true))
    );
    assert_that!(value["known_catalog_blocker"].as_bool(), eq(Some(true)));
    assert_that!(f.calls()?, eq(1));
    Ok(())
}
#[googletest::test]
fn capture_failure_before_start_prevents_the_pool_command() -> Result<()> {
    let f = Fixture::new(&catalog("completed"), 0)?;
    fs::write(f.path("receipts"), b"not a directory")?;
    let (value, code) = f.observe(None)?;
    assert_that!(code, eq(4));
    assert_that!(value["pool_started"].as_bool(), eq(Some(false)));
    assert_that!(f.calls()?, eq(0));
    Ok(())
}
#[googletest::test]
fn failed_status_capture_never_retries_the_completed_command() -> Result<()> {
    let data = catalog("completed");
    let f = Fixture::new(&data, 0)?;
    f.response(&serde_json::to_vec(&data)?, b"stderr", 0, true)?;
    let (value, code) = f.observe(None)?;
    assert_that!(code, eq(4));
    assert_that!(value["capture_complete"].as_bool(), eq(Some(false)));
    assert_that!(value["pool_started"].as_bool(), eq(Some(true)));
    assert_that!(value["pool_exit"].as_i64(), eq(Some(0)));
    assert_that!(f.calls()?, eq(1));
    let receipt = Fixture::receipt(&value)?;
    assert_that!(
        fs::read(receipt.join("stdout"))?,
        eq(&serde_json::to_vec(&data)?)
    );
    Ok(())
}
#[googletest::test]
fn missing_or_conflicting_identity_cannot_establish_ownership() -> Result<()> {
    for data in [catalog("completed"), {
        let mut v = catalog("completed");
        v["data"]["assignments"] =
            json!([assignment(HANDLE, "active"), assignment(HANDLE, "released")]);
        v
    }] {
        let f = Fixture::new(&data, 0)?;
        let handle = if data["data"]["assignments"][1]["assignment_handle"] == HANDLE {
            HANDLE
        } else {
            OP
        };
        let (value, code) = f.observe(Some(handle))?;
        assert_that!(code, eq(4));
        assert_that!(
            value["interpretation_failure"]
                .as_str()
                .context("reason")?
                .contains("missing or ambiguous"),
            eq(true)
        );
        assert_that!(f.calls()?, eq(1));
    }
    Ok(())
}
#[googletest::test]
fn lossless_arguments_and_rejected_missing_authority_are_preserved() -> Result<()> {
    let mut data = catalog("rejected");
    data["context"] = json!({});
    data["data"] = json!({});
    let f = Fixture::new(&data, 2)?;
    let argument = OsString::from_vec(vec![47, 116, 109, 112, 47, 255]);
    let (value, code) = f.run(None, &["repo".into(), "register".into(), argument])?;
    assert_that!(code, eq(2));
    assert_that!(
        value["catalog"]["identity_missing"].as_bool(),
        eq(Some(true))
    );
    let meta: Value = serde_json::from_slice(&fs::read(
        Fixture::receipt(&value)?.join("invocation.json"),
    )?)?;
    assert_that!(
        meta["arguments"][3]["bytes"],
        eq(&json!([47, 116, 109, 112, 47, 255]))
    );
    Ok(())
}
#[googletest::test]
fn inconsistent_outcome_and_missing_command_identity_stop_acceptance() -> Result<()> {
    let f = Fixture::new(&catalog("completed"), 3)?;
    let (value, code) = f.observe(None)?;
    assert_that!(code, eq(4));
    assert_that!(value["pool_exit"].as_i64(), eq(Some(3)));
    let f = Fixture::new(&catalog("completed"), 0)?;
    let (_, code) = f.run(None, &["acquire".into(), "--repo".into(), REPO.into()])?;
    assert_that!(code, eq(4));
    assert_that!(f.calls()?, eq(1));
    Ok(())
}
#[googletest::test]
fn a_git_checkout_cannot_be_used_for_receipts() -> Result<()> {
    let f = Fixture::new(&catalog("completed"), 0)?;
    ensure!(
        Command::new("git")
            .args(["init", "--quiet"])
            .arg(f.directory.path())
            .status()?
            .success(),
        "fixture repository"
    );
    let (_, code) = f.observe(None)?;
    assert_that!(code, eq(4));
    assert_that!(f.path("receipts").exists(), eq(false));
    assert_that!(f.calls()?, eq(0));
    Ok(())
}

#[googletest::test]
fn parent_traversal_cannot_create_receipts_inside_a_repository() -> Result<()> {
    let mut f = Fixture::new(&catalog("completed"), 0)?;
    let repo = f.path("repo");
    fs::create_dir(&repo)?;
    ensure!(
        Command::new("git")
            .args(["init", "--quiet"])
            .arg(&repo)
            .status()?
            .success(),
        "fixture repository"
    );
    f.receipt_root = f.path("new/../repo/receipts");
    let (_, code) = f.observe(None)?;
    assert_that!(code, eq(4));
    assert_that!(f.path("new").exists(), eq(false));
    assert_that!(repo.join("receipts").exists(), eq(false));
    assert_that!(f.calls()?, eq(0));
    Ok(())
}

#[googletest::test]
fn repeated_explicit_reads_create_distinct_receipts() -> Result<()> {
    let f = Fixture::new(&catalog("completed"), 0)?;
    let (first, first_code) = f.observe(None)?;
    let (second, second_code) = f.observe(None)?;
    assert_that!(first_code, eq(0));
    assert_that!(second_code, eq(0));
    assert_that!(
        Fixture::receipt(&first)? == Fixture::receipt(&second)?,
        eq(false)
    );
    assert_that!(f.calls()?, eq(2));
    Ok(())
}
#[googletest::test]
fn a_successful_acquisition_reports_only_its_returned_assignment() -> Result<()> {
    let mut data = catalog("completed");
    data["command"] = json!("acquire");
    data["data"] = json!({"assignment":assignment(HANDLE,"active")});
    let f = Fixture::new(&data, 0)?;
    let (value, code) = f.run(None, &["acquire".into(), "--repo".into(), REPO.into()])?;
    assert_that!(code, eq(0));
    assert_that!(
        value["selection_status"].as_str(),
        eq(Some("command_result"))
    );
    assert_that!(
        value["assignment"]["assignment_handle"].as_str(),
        eq(Some(HANDLE))
    );
    assert_that!(
        value["catalog"]["pending_inventory_returned"].as_bool(),
        eq(Some(false))
    );
    assert_that!(value["catalog"]["pending_count"].is_null(), eq(true));
    assert_that!(f.calls()?, eq(1));
    Ok(())
}
#[googletest::test]
fn conflicting_catalog_ids_and_missing_path_bytes_stop_interpretation() -> Result<()> {
    let mut data = catalog("completed");
    data["data"]["catalog_id"] = json!(OTHER);
    let f = Fixture::new(&data, 0)?;
    let (_, code) = f.observe(None)?;
    assert_that!(code, eq(4));
    let mut data = catalog("completed");
    data["data"]["assignments"][0]["path"]["bytes"] = Value::Null;
    let f = Fixture::new(&data, 0)?;
    let (value, code) = f.observe(Some(HANDLE))?;
    assert_that!(code, eq(4));
    assert_that!(
        value["interpretation_failure"]
            .as_str()
            .context("reason")?
            .contains("path bytes missing"),
        eq(true)
    );
    Ok(())
}
#[googletest::test]
fn summary_overflow_is_an_explicit_unknown_result() -> Result<()> {
    let mut data = catalog("completed");
    data["data"]["operations"]=json!((1..=8).map(|i|json!({"operation_id":format!("00000000-0000-0000-0000-{i:012x}"),"state":"\u{0000}".repeat(64),"last_checkpoint":"\u{0000}".repeat(80)})).collect::<Vec<_>>());
    let f = Fixture::new(&data, 0)?;
    let (value, code) = f.observe(None)?;
    assert_that!(code, eq(4));
    assert_that!(value["summary_overflow"].as_bool(), eq(Some(true)));
    assert_that!(value["pool_exit"].as_i64(), eq(Some(0)));
    assert_that!(f.calls()?, eq(1));
    Ok(())
}
#[googletest::test]
fn an_oversized_response_remains_complete_without_interpretation() -> Result<()> {
    let f = Fixture::new(&catalog("completed"), 0)?;
    let large = f.path("large-output");
    fs::write(&large, vec![b' '; 17 * 1024 * 1024])?;
    fs::write(
        f.path("response.json"),
        serde_json::to_vec(&json!({"stdout_file":large,"stderr":[],"exit":0}))?,
    )?;
    let (value, code) = f.observe(None)?;
    assert_that!(code, eq(4));
    assert_that!(
        value["interpretation_failure"]
            .as_str()
            .context("reason")?
            .contains("16 MiB"),
        eq(true)
    );
    assert_that!(
        fs::metadata(Fixture::receipt(&value)?.join("stdout"))?.len(),
        eq(17 * 1024 * 1024)
    );
    assert_that!(f.calls()?, eq(1));
    Ok(())
}

#[googletest::test]
fn a_pending_report_remains_visible_when_its_identity_is_incomplete() -> Result<()> {
    let mut data = catalog("pending");
    data["data"]["assignments"][0]["path"]["bytes"] = Value::Null;
    let f = Fixture::new(&data, 3)?;
    let (value, code) = f.observe(Some(HANDLE))?;
    assert_that!(code, eq(4));
    assert_that!(value["pool_exit"].as_i64(), eq(Some(3)));
    assert_that!(value["pool_outcome"].as_str(), eq(Some("pending")));
    assert_that!(value["capture_complete"].as_bool(), eq(Some(true)));
    assert_that!(f.calls()?, eq(1));
    Ok(())
}

#[googletest::test]
fn wrapper_argument_errors_and_help_never_emit_unbounded_stderr() -> Result<()> {
    let f = Fixture::new(&catalog("completed"), 0)?;
    for (argument, expected) in [
        (format!("--{}", "x".repeat(20_000)), 4),
        ("--help".to_owned(), 0),
    ] {
        let output = Command::new(&f.tool)
            .arg(argument)
            .env("PATH", f.path("bin"))
            .output()?;
        assert_that!(output.status.code(), eq(Some(expected)));
        assert_that!(output.stdout.len() <= 8192, eq(true));
        assert_that!(output.stderr.is_empty(), eq(true));
        let value: Value = serde_json::from_slice(&output.stdout)?;
        assert_that!(value["pool_started"].as_bool(), eq(Some(false)));
        if expected == 4 {
            assert_that!(
                value["argument_failure"].as_str(),
                eq(Some("UnknownArgument"))
            );
        } else {
            assert_that!(
                value["usage_informational"]
                    .as_str()
                    .context("help")?
                    .contains("Usage"),
                eq(true)
            );
        }
    }
    assert_that!(f.calls()?, eq(0));
    Ok(())
}
