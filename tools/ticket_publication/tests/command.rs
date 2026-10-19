//! Command tests use private state and a persistent fake GitHub process.
use anyhow::Context;
use googletest::{assert_that, matchers::eq};
use runfiles::{Runfiles, rlocation};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output},
};

struct Fixture {
    directory: tempfile::TempDir,
    tool: PathBuf,
}
impl Fixture {
    fn new() -> anyhow::Result<Self> {
        let files = Runfiles::create()?;
        let tool = rlocation!(files, std::env::var("PUBLICATION_TOOL")?)
            .context("publication tool runfile")?;
        let directory = tempfile::tempdir()?;
        fs::write(
            directory.path().join("plan.json"),
            serde_json::to_vec(&json!({
                "schema_version":1,"repository":"LowkeyLab/bazel-repo","parent":2401,
                "tickets":[{"key":"first","title":"Prepare a batch","body":"Describe the behavior.","labels":["ready-for-agent"],"blockers":[]}]
            }))?,
        )?;
        fs::create_dir(directory.path().join("bin"))?;
        let gh = directory.path().join("bin/gh");
        fs::write(&gh, "#!/bin/sh\necho called >> \"$REMOTE_STATE\"\nexit 1\n")?;
        fs::set_permissions(gh, fs::Permissions::from_mode(0o700))?;
        Ok(Self { directory, tool })
    }
    fn run(&self, args: &[&str]) -> anyhow::Result<Output> {
        Ok(Command::new(&self.tool)
            .args(args)
            .current_dir(self.directory.path())
            .env("PATH", self.directory.path().join("bin"))
            .env("REMOTE_STATE", self.directory.path().join("remote.log"))
            .output()?)
    }
    fn successful(&self, args: &[&str]) -> anyhow::Result<Value> {
        let output = self.run(args)?;
        anyhow::ensure!(
            output.status.success(),
            "command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    }
}

#[googletest::test]
fn prepared_batch_survives_restart_without_github_calls() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let prepared = f.successful(&["--state-dir", "state", "prepare", "--plan", "plan.json"])?;
    let batch = prepared["batch_id"].as_str().context("batch identity")?;
    let first = f.successful(&["--state-dir", "state", "status", "--batch", batch])?;
    let second = f.successful(&["--state-dir", "state", "status", "--batch", batch])?;
    assert_that!(first, eq(&second));
    assert_that!(second["status"].as_str(), eq(Some("prepared")));
    assert_that!(
        second["plan"]["tickets"][0]["title"].as_str(),
        eq(Some("Prepare a batch"))
    );
    assert_that!(f.directory.path().join("remote.log").exists(), eq(false));
    Ok(())
}

#[googletest::test]
fn state_inside_a_git_checkout_is_rejected() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    fs::create_dir(f.directory.path().join(".git"))?;
    let output = f.run(&["--state-dir", "state", "prepare", "--plan", "plan.json"])?;
    assert_that!(output.status.success(), eq(false));
    assert_that!(f.directory.path().join("state").exists(), eq(false));
    Ok(())
}

#[googletest::test]
fn invalid_plans_create_no_batch_or_remote_effects() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let original: Value = serde_json::from_slice(&fs::read(f.directory.path().join("plan.json"))?)?;
    let mut invalid = Vec::new();
    let mut plan = original.clone();
    plan["repository"] = json!("another/repository");
    invalid.push(plan);
    let mut plan = original.clone();
    plan["parent"] = json!(0);
    invalid.push(plan);
    let mut plan = original.clone();
    plan["schema_version"] = json!(2);
    invalid.push(plan);
    let mut plan = original.clone();
    plan["tickets"][0]["key"] = json!("../escape");
    invalid.push(plan);
    let mut plan = original.clone();
    plan["tickets"]
        .as_array_mut()
        .unwrap()
        .push(original["tickets"][0].clone());
    invalid.push(plan);
    let mut plan = original.clone();
    plan["tickets"][0]["blockers"] = json!([{"ticket":"missing"}]);
    invalid.push(plan);
    let mut plan = original.clone();
    plan["tickets"][0]["blockers"] = json!([{"ticket":"first"}]);
    invalid.push(plan);
    let mut plan = original.clone();
    plan["tickets"][0]["body"] = json!("<!-- ticket-publication:other:first -->");
    invalid.push(plan);
    let mut plan = original.clone();
    plan["extra"] = json!(true);
    invalid.push(plan);
    let mut plan = original;
    plan["tickets"][0]["title"] = json!(" ");
    invalid.push(plan);
    for plan in invalid {
        fs::write(
            f.directory.path().join("plan.json"),
            serde_json::to_vec(&plan)?,
        )?;
        let output = f.run(&["--state-dir", "state", "prepare", "--plan", "plan.json"])?;
        assert_that!(output.status.success(), eq(false));
        assert_that!(f.directory.path().join("state").exists(), eq(false));
    }
    let output = f.run(&["--state-dir", "state", "status", "--batch", "../../escape"])?;
    assert_that!(output.status.success(), eq(false));
    assert_that!(f.directory.path().join("remote.log").exists(), eq(false));
    Ok(())
}

#[googletest::test]
fn explicit_identity_preserves_the_original_plan_and_private_files() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let id = "84c6d08c-06c2-4d37-ac9d-d051a3e0786d";
    let args = [
        "--state-dir",
        "state",
        "prepare",
        "--plan",
        "plan.json",
        "--batch",
        id,
    ];
    let first = f.successful(&args)?;
    let file = f
        .directory
        .path()
        .join(first["plan_path"].as_str().context("plan path")?);
    let original = fs::read(&file)?;
    f.successful(&args)?;
    assert_that!(fs::read(&file)?, eq(&original));
    assert_that!(fs::metadata(&file)?.permissions().mode() & 0o077, eq(0));
    assert_that!(
        fs::metadata(file.parent().unwrap())?.permissions().mode() & 0o077,
        eq(0)
    );
    let mut plan: Value = serde_json::from_slice(&fs::read(f.directory.path().join("plan.json"))?)?;
    plan["tickets"][0]["title"] = json!("A different plan");
    fs::write(
        f.directory.path().join("plan.json"),
        serde_json::to_vec(&plan)?,
    )?;
    assert_that!(f.run(&args)?.status.success(), eq(false));
    assert_that!(fs::read(&file)?, eq(&original));
    let other = f.successful(&["--state-dir", "state", "prepare", "--plan", "plan.json"])?;
    assert_that!(other["batch_id"] != first["batch_id"], eq(true));
    assert_that!(f.directory.path().join("remote.log").exists(), eq(false));
    Ok(())
}

#[googletest::test]
fn corrupt_or_unsupported_snapshots_are_preserved() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let prepared = f.successful(&["--state-dir", "state", "prepare", "--plan", "plan.json"])?;
    let id = prepared["batch_id"].as_str().context("batch identity")?;
    let file = f
        .directory
        .path()
        .join(prepared["plan_path"].as_str().context("plan path")?);
    let original: Value = serde_json::from_slice(&fs::read(&file)?)?;
    let mut corrupt = vec![b"not json".to_vec()];
    let mut state = original.clone();
    state["schema_version"] = json!(2);
    corrupt.push(serde_json::to_vec(&state)?);
    let mut state = original.clone();
    state["plan"]["tickets"][0]["body"] = json!("Modified body");
    corrupt.push(serde_json::to_vec(&state)?);
    let mut state = original.clone();
    state["tickets"][0]["marker"] = json!("other marker");
    corrupt.push(serde_json::to_vec(&state)?);
    let mut state = original;
    state["tickets"][0]["status"] = json!("complete");
    corrupt.push(serde_json::to_vec(&state)?);
    for bytes in corrupt {
        fs::write(&file, &bytes)?;
        assert_that!(
            f.run(&["--state-dir", "state", "status", "--batch", id])?
                .status
                .success(),
            eq(false)
        );
        assert_that!(
            f.run(&[
                "--state-dir",
                "state",
                "prepare",
                "--plan",
                "plan.json",
                "--batch",
                id
            ])?
            .status
            .success(),
            eq(false)
        );
        assert_that!(fs::read(&file)?, eq(&bytes));
    }
    Ok(())
}

#[googletest::test]
fn durable_failure_and_incomplete_preparation_do_not_report_success() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let id = "84c6d08c-06c2-4d37-ac9d-d051a3e0786d";
    let directory = f.directory.path().join("state").join(id);
    fs::create_dir_all(&directory)?;
    fs::set_permissions(
        f.directory.path().join("state"),
        fs::Permissions::from_mode(0o700),
    )?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let incomplete = f.run(&["--state-dir", "state", "status", "--batch", id])?;
    assert_that!(incomplete.status.success(), eq(false));
    // A destination directory forces a real filesystem error and must survive.
    fs::create_dir(directory.join("batch.json"))?;
    assert_that!(
        f.run(&[
            "--state-dir",
            "state",
            "prepare",
            "--plan",
            "plan.json",
            "--batch",
            id
        ])?
        .status
        .success(),
        eq(false)
    );
    assert_that!(directory.join("batch.json").is_dir(), eq(true));
    fs::remove_dir(directory.join("batch.json"))?;
    f.successful(&[
        "--state-dir",
        "state",
        "prepare",
        "--plan",
        "plan.json",
        "--batch",
        id,
    ])?;
    f.successful(&["--state-dir", "state", "status", "--batch", id])?;
    assert_that!(f.directory.path().join("remote.log").exists(), eq(false));
    Ok(())
}

#[googletest::test]
fn an_existing_lock_excludes_competing_commands_and_releases_on_exit() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let prepared = f.successful(&["--state-dir", "state", "prepare", "--plan", "plan.json"])?;
    let id = prepared["batch_id"].as_str().context("batch identity")?;
    let file = f
        .directory
        .path()
        .join(prepared["plan_path"].as_str().context("plan path")?);
    let snapshot = fs::read(&file)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(file.parent().unwrap().join("lock"))?;
    lock.lock()?;
    assert_that!(
        f.run(&["--state-dir", "state", "status", "--batch", id])?
            .status
            .success(),
        eq(false)
    );
    assert_that!(
        f.run(&[
            "--state-dir",
            "state",
            "prepare",
            "--plan",
            "plan.json",
            "--batch",
            id
        ])?
        .status
        .success(),
        eq(false)
    );
    assert_that!(fs::read(&file)?, eq(&snapshot));
    drop(lock);
    f.successful(&["--state-dir", "state", "status", "--batch", id])?;
    Ok(())
}

#[googletest::test]
fn default_state_uses_the_platform_directory() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let output = Command::new(&f.tool)
        .args(["prepare", "--plan", "plan.json"])
        .current_dir(f.directory.path())
        .env("XDG_STATE_HOME", f.directory.path().join("platform-state"))
        .output()?;
    assert_that!(output.status.success(), eq(true));
    let prepared: Value = serde_json::from_slice(&output.stdout)?;
    assert_that!(
        PathBuf::from(prepared["plan_path"].as_str().context("plan path")?)
            .starts_with(f.directory.path().join("platform-state/ticket-publication")),
        eq(true)
    );
    Ok(())
}

#[googletest::test]
fn interrupted_snapshot_write_can_resume_with_the_original_identity() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let id = "84c6d08c-06c2-4d37-ac9d-d051a3e0786d";
    let mut plan: Value = serde_json::from_slice(&fs::read(f.directory.path().join("plan.json"))?)?;
    plan["tickets"][0]["body"] = json!("Approved behavior. ".repeat(4096));
    fs::write(
        f.directory.path().join("plan.json"),
        serde_json::to_vec(&plan)?,
    )?;
    // The kernel interrupts the real command during its snapshot write.
    let interrupted = Command::new("/bin/sh")
        .args(["-c", "ulimit -f 1; exec \"$@\"", "limit"])
        .arg(&f.tool)
        .args([
            "--state-dir",
            "state",
            "prepare",
            "--plan",
            "plan.json",
            "--batch",
            id,
        ])
        .current_dir(f.directory.path())
        .env("PATH", f.directory.path().join("bin"))
        .env("REMOTE_STATE", f.directory.path().join("remote.log"))
        .output()?;
    assert_that!(interrupted.status.success(), eq(false));
    let directory = f.directory.path().join("state").join(id);
    assert_that!(directory.join("lock").exists(), eq(true));
    assert_that!(directory.join("batch.json").exists(), eq(false));
    assert_that!(
        f.run(&["--state-dir", "state", "status", "--batch", id])?
            .status
            .success(),
        eq(false)
    );
    f.successful(&[
        "--state-dir",
        "state",
        "prepare",
        "--plan",
        "plan.json",
        "--batch",
        id,
    ])?;
    let status = f.successful(&["--state-dir", "state", "status", "--batch", id])?;
    assert_that!(status["plan"], eq(&plan));
    assert_that!(f.directory.path().join("remote.log").exists(), eq(false));
    Ok(())
}

#[googletest::test]
fn ordered_tickets_keep_batch_and_external_blockers() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let mut plan: Value = serde_json::from_slice(&fs::read(f.directory.path().join("plan.json"))?)?;
    let mut dependent = plan["tickets"][0].clone();
    dependent["key"] = json!("second");
    dependent["blockers"] = json!([{"ticket":"first"},{"issue":2392}]);
    plan["tickets"].as_array_mut().unwrap().insert(0, dependent);
    fs::write(
        f.directory.path().join("plan.json"),
        serde_json::to_vec(&plan)?,
    )?;
    let prepared = f.successful(&["--state-dir", "state", "prepare", "--plan", "plan.json"])?;
    let id = prepared["batch_id"].as_str().context("batch identity")?;
    let status = f.successful(&["--state-dir", "state", "status", "--batch", id])?;
    assert_that!(status["plan"], eq(&plan));
    assert_that!(status["tickets"][0]["key"].as_str(), eq(Some("second")));
    assert_that!(
        status["tickets"][0]["marker"].as_str(),
        eq(Some(
            format!("<!-- ticket-publication:{id}:second -->").as_str()
        ))
    );
    Ok(())
}
