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
        let fake = rlocation!(files, std::env::var("FAKE_GH")?).context("fake gh")?;
        std::os::unix::fs::symlink(fake, gh)?;
        Ok(Self { directory, tool })
    }
    fn run(&self, args: &[&str]) -> anyhow::Result<Output> {
        Ok(self.command(args).output()?)
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.tool);
        command
            .args(args)
            .current_dir(self.directory.path())
            .env("PATH", self.directory.path().join("bin"))
            .env("REMOTE_STATE", self.directory.path().join("remote.log"));
        command
    }
    fn successful(&self, args: &[&str]) -> anyhow::Result<Value> {
        let output = self.run(args)?;
        anyhow::ensure!(
            output.status.success(),
            "command failed: {} {}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
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

#[googletest::test]
fn failed_directory_sync_is_retried_before_preparation_succeeds() -> anyhow::Result<()> {
    let files = Runfiles::create()?;
    let fault =
        rlocation!(files, std::env::var("SYNC_FAULT_LIBRARY")?).context("sync fault runfile")?;
    for fail_on_root in [true, false] {
        let f = Fixture::new()?;
        let id = "84c6d08c-06c2-4d37-ac9d-d051a3e0786d";
        let state = f.directory.path().join("state");
        let target = if fail_on_root {
            state.clone()
        } else {
            f.directory.path().to_path_buf()
        };
        let receipt = f.directory.path().join("sync-receipt");
        let run = || {
            Command::new(&f.tool)
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
                .env("LD_PRELOAD", &fault)
                .env("SYNC_FAULT", &target)
                .env("SYNC_RECEIPT", &receipt)
                .env("PATH", f.directory.path().join("bin"))
                .env("REMOTE_STATE", f.directory.path().join("remote.log"))
                .output()
        };
        let failed = run()?;
        assert_that!(failed.status.success(), eq(false));
        assert_that!(receipt.exists(), eq(true));
        assert_that!(state.join(id).join("batch.json").exists(), eq(false));
        assert_that!(fs::read_to_string(&receipt)?, eq(""));
        let retried = run()?;
        anyhow::ensure!(
            retried.status.success(),
            "retry failed: {}",
            String::from_utf8_lossy(&retried.stderr)
        );
        assert_that!(
            fs::read_to_string(&receipt)?.contains("synchronized"),
            eq(true)
        );
        f.successful(&["--state-dir", "state", "status", "--batch", id])?;
        assert_that!(f.directory.path().join("remote.log").exists(), eq(false));
    }
    Ok(())
}

#[googletest::test]
fn non_utf8_state_paths_fail_before_creating_any_batch() -> anyhow::Result<()> {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    for source in ["argument", "XDG_STATE_HOME", "HOME"] {
        let f = Fixture::new()?;
        let mut bytes = f.directory.path().as_os_str().as_encoded_bytes().to_vec();
        bytes.extend_from_slice(b"/non-utf8-\xff");
        let invalid = PathBuf::from(OsString::from_vec(bytes));
        let mut command = Command::new(&f.tool);
        command.current_dir(f.directory.path());
        if source == "argument" {
            command.arg("--state-dir").arg(&invalid);
        } else {
            command.env_remove("XDG_STATE_HOME").env(source, &invalid);
        }
        let output = command.args(["prepare", "--plan", "plan.json"]).output()?;
        assert_that!(output.status.code(), eq(Some(1)));
        assert_that!(invalid.exists(), eq(false));
    }
    Ok(())
}

#[googletest::test]
fn preparation_below_a_traverse_only_ancestor_succeeds() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let files = Runfiles::create()?;
    let fault = rlocation!(files, std::env::var("SYNC_FAULT_LIBRARY")?).context("fault runfile")?;
    let ancestor = f.directory.path().join("traverse-only");
    let home = ancestor.join("user");
    fs::create_dir_all(&home)?;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(&ancestor, fs::Permissions::from_mode(0o111))?;
    let state = home.join("state");
    let output = Command::new(&f.tool)
        .arg("--state-dir")
        .arg(&state)
        .args(["prepare", "--plan", "plan.json"])
        .current_dir(f.directory.path())
        .env("LD_PRELOAD", &fault)
        .env("DENY_READ_DIRECTORY", &ancestor)
        .output()?;
    fs::set_permissions(&ancestor, fs::Permissions::from_mode(0o700))?;
    anyhow::ensure!(
        output.status.success(),
        "preparation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let prepared: Value = serde_json::from_slice(&output.stdout)?;
    let id = prepared["batch_id"].as_str().context("batch identity")?;
    f.successful(&[
        "--state-dir",
        state.to_str().context("state path")?,
        "status",
        "--batch",
        id,
    ])?;
    Ok(())
}

#[googletest::test]
fn invalid_creation_markers_stop_inspection_without_changing_the_snapshot() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let prepared = f.successful(&["--state-dir", "state", "prepare", "--plan", "plan.json"])?;
    let id = prepared["batch_id"].as_str().context("batch identity")?;
    let file = f
        .directory
        .path()
        .join(prepared["plan_path"].as_str().context("plan path")?);
    let snapshot = fs::read(&file)?;
    for directory in [
        f.directory.path().join("state"),
        file.parent().context("batch directory")?.to_path_buf(),
    ] {
        let marker = directory.join(".ticket-publication-directory");
        let original = fs::read(&marker)?;
        fs::write(&marker, b"unsupported marker version")?;
        assert_that!(
            f.run(&["--state-dir", "state", "status", "--batch", id])?
                .status
                .success(),
            eq(false)
        );
        assert_that!(fs::read(&file)?, eq(&snapshot));
        assert_that!(
            fs::read(&marker)?,
            eq(&b"unsupported marker version".to_vec())
        );
        fs::write(marker, original)?;
    }
    Ok(())
}

#[googletest::test]
fn publishing_creates_once_with_original_identity_and_reports_partial_graph() -> anyhow::Result<()>
{
    let f = Fixture::new()?;
    let files = Runfiles::create()?;
    let fake = rlocation!(files, std::env::var("FAKE_GH")?).context("fake gh")?;
    fs::remove_file(f.directory.path().join("bin/gh"))?;
    std::os::unix::fs::symlink(fake, f.directory.path().join("bin/gh"))?;
    let prepared = f.successful(&["--state-dir", "state", "prepare", "--plan", "plan.json"])?;
    let id = prepared["batch_id"].as_str().context("batch identity")?;
    let result = f.successful(&["--state-dir", "state", "publish", "--batch", id])?;
    assert_that!(result["status"].as_str(), eq(Some("partial")));
    assert_that!(result["phase"].as_str(), eq(Some("issues_created")));
    f.successful(&["--state-dir", "state", "publish", "--batch", id])?;
    let remote: Value = serde_json::from_slice(&fs::read(f.directory.path().join("remote.log"))?)?;
    assert_that!(remote["creations"].as_u64(), eq(Some(1)));
    assert_that!(
        remote["issues"][0]["title"].as_str(),
        eq(Some("Prepare a batch"))
    );
    assert_that!(remote["issues"][0]["labels"], eq(&json!([])));
    assert_that!(
        remote["issues"][0]["body"].as_str(),
        eq(Some(
            format!("Describe the behavior.\n\n<!-- ticket-publication:{id}:first -->").as_str()
        ))
    );
    Ok(())
}

impl Fixture {
    fn with_github() -> anyhow::Result<Self> {
        let f = Self::new()?;
        let files = Runfiles::create()?;
        let fake = rlocation!(files, std::env::var("FAKE_GH")?).context("fake gh")?;
        fs::remove_file(f.directory.path().join("bin/gh"))?;
        std::os::unix::fs::symlink(fake, f.directory.path().join("bin/gh"))?;
        Ok(f)
    }
    fn remote(&self) -> anyhow::Result<Value> {
        Ok(serde_json::from_slice(&fs::read(
            self.directory.path().join("remote.log"),
        )?)?)
    }
    fn save_remote(&self, state: &Value) -> anyhow::Result<()> {
        Ok(fs::write(
            self.directory.path().join("remote.log"),
            serde_json::to_vec(state)?,
        )?)
    }
    fn prepare(&self) -> anyhow::Result<String> {
        Ok(self.successful(&["--state-dir", "state", "prepare", "--plan", "plan.json"])?["batch_id"].as_str().context("batch identity")?.to_owned())
    }
}

#[googletest::test]
fn accepted_creation_without_a_response_is_recovered_after_restart() -> anyhow::Result<()> {
    let f = Fixture::with_github()?;
    f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
    let id = f.prepare()?;
    assert_that!(
        f.run(&["--state-dir", "state", "publish", "--batch", &id])?
            .status
            .success(),
        eq(false)
    );
    assert_that!(f.remote()?["creations"].as_u64(), eq(Some(1)));
    let result = f.successful(&["--state-dir", "state", "publish", "--batch", &id])?;
    assert_that!(result["phase"].as_str(), eq(Some("issues_created")));
    assert_that!(f.remote()?["creations"].as_u64(), eq(Some(1)));
    Ok(())
}

#[googletest::test]
fn unresolved_or_conflicting_creation_never_retries_a_remote_write() -> anyhow::Result<()> {
    for mode in ["absent", "multiple", "permission", "incomplete", "content"] {
        let f = Fixture::with_github()?;
        f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
        let id = f.prepare()?;
        assert_that!(
            f.run(&["--state-dir", "state", "publish", "--batch", &id])?
                .status
                .success(),
            eq(false)
        );
        let mut state = f.remote()?;
        match mode {
            "absent" => state["issues"] = json!([]),
            "multiple" => {
                let mut duplicate = state["issues"][0].clone();
                duplicate["number"] = json!(3001);
                duplicate["id"] = json!(13001);
                duplicate["node_id"] = json!("I_3001");
                state["issues"].as_array_mut().unwrap().push(duplicate);
            }
            "content" => state["issues"][0]["title"] = json!("External change"),
            _ => state["mode"] = json!(mode),
        }
        f.save_remote(&state)?;
        let output = f.run(&["--state-dir", "state", "publish", "--batch", &id])?;
        assert_that!(output.status.success(), eq(false));
        let report: Value = serde_json::from_slice(&output.stdout)?;
        assert_that!(
            report["status"].as_str(),
            eq(Some(if mode == "multiple" || mode == "content" {
                "conflict"
            } else {
                "uncertain"
            }))
        );
        assert_that!(f.remote()?, eq(&state));
    }
    Ok(())
}

#[googletest::test]
fn dependency_order_and_saved_mappings_survive_restart_without_labels() -> anyhow::Result<()> {
    let f = Fixture::with_github()?;
    let mut plan: Value = serde_json::from_slice(&fs::read(f.directory.path().join("plan.json"))?)?;
    let mut dependent = plan["tickets"][0].clone();
    dependent["key"] = json!("second");
    dependent["title"] = json!("Dependent work");
    dependent["blockers"] = json!([{"ticket":"first"},{"issue":2392}]);
    plan["tickets"].as_array_mut().unwrap().insert(0, dependent);
    fs::write(
        f.directory.path().join("plan.json"),
        serde_json::to_vec(&plan)?,
    )?;
    let id = f.prepare()?;
    f.successful(&["--state-dir", "state", "publish", "--batch", &id])?;
    let state = f.remote()?;
    assert_that!(state["creations"].as_u64(), eq(Some(2)));
    assert_that!(
        state["issues"][0]["title"].as_str(),
        eq(Some("Prepare a batch"))
    );
    assert_that!(
        state["issues"][1]["title"].as_str(),
        eq(Some("Dependent work"))
    );
    f.successful(&["--state-dir", "state", "publish", "--batch", &id])?;
    assert_that!(f.remote()?, eq(&state));
    let status = f.successful(&["--state-dir", "state", "status", "--batch", &id])?;
    assert_that!(status["status"].as_str(), eq(Some("partial")));
    assert_that!(
        status["tickets"][0]["status"]["created"]["number"].as_u64(),
        eq(Some(3001))
    );
    assert_that!(
        status["tickets"][0]["status"]["created"]["id"].as_u64(),
        eq(Some(13001))
    );
    assert_that!(
        status["tickets"][0]["status"]["created"]["node_id"].as_str(),
        eq(Some("I_3001"))
    );
    Ok(())
}

#[googletest::test]
fn closed_match_on_a_later_page_is_reused_and_pull_requests_are_excluded() -> anyhow::Result<()> {
    let f = Fixture::with_github()?;
    f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
    let id = f.prepare()?;
    assert_that!(
        f.run(&["--state-dir", "state", "publish", "--batch", &id])?
            .status
            .success(),
        eq(false)
    );
    let mut state = f.remote()?;
    let mut issue = state["issues"][0].clone();
    issue["state"] = json!("closed");
    let mut issues = Vec::new();
    for number in 4000..4100 {
        let mut pr = issue.clone();
        pr["number"] = json!(number);
        pr["pull_request"] = json!({"url":"example"});
        issues.push(pr);
    }
    let mut similar = issue.clone();
    similar["number"] = json!(5000);
    similar["node_id"] = json!("I_5000");
    similar["id"] = json!(15000);
    similar["body"] = json!(
        issue["body"]
            .as_str()
            .unwrap()
            .replace(":first -->", ":first-extra -->")
    );
    issues.push(similar);
    issues.push(issue);
    state["issues"] = json!(issues);
    f.save_remote(&state)?;
    f.successful(&["--state-dir", "state", "publish", "--batch", &id])?;
    assert_that!(f.remote()?, eq(&state));
    Ok(())
}

#[googletest::test]
fn saved_mappings_stop_on_external_changes_without_overwriting_remote_state() -> anyhow::Result<()>
{
    for field in [
        "title",
        "body",
        "id",
        "node_id",
        "repository_url",
        "pull_request",
    ] {
        let f = Fixture::with_github()?;
        let id = f.prepare()?;
        f.successful(&["--state-dir", "state", "publish", "--batch", &id])?;
        let mut state = f.remote()?;
        state["issues"][0][field] = match field {
            "id" => json!(99999),
            "node_id" => json!("I_changed"),
            "pull_request" => json!({"url":"example"}),
            _ => json!("External change"),
        };
        f.save_remote(&state)?;
        let output = f.run(&["--state-dir", "state", "publish", "--batch", &id])?;
        assert_that!(output.status.success(), eq(false));
        let report: Value = serde_json::from_slice(&output.stdout)?;
        assert_that!(report["status"].as_str(), eq(Some("conflict")));
        assert_that!(f.remote()?, eq(&state));
    }
    Ok(())
}

#[googletest::test]
fn interrupted_progress_writes_resume_only_when_creation_is_safe() -> anyhow::Result<()> {
    let files = Runfiles::create()?;
    let fault = rlocation!(files, std::env::var("SYNC_FAULT_LIBRARY")?).context("sync fault")?;
    for phase in [
        "intent-file",
        "intent-directory",
        "mapping-file",
        "mapping-directory",
    ] {
        for effect in ["exit", "error"] {
            let f = Fixture::with_github()?;
            let mut plan: Value =
                serde_json::from_slice(&fs::read(f.directory.path().join("plan.json"))?)?;
            let mut dependent = plan["tickets"][0].clone();
            dependent["key"] = json!("second");
            dependent["blockers"] = json!([{"ticket":"first"}]);
            plan["tickets"].as_array_mut().unwrap().push(dependent);
            fs::write(
                f.directory.path().join("plan.json"),
                serde_json::to_vec(&plan)?,
            )?;
            let id = f.prepare()?;
            let receipt = f.directory.path().join("progress-fault-receipt");
            let output = f
                .command(&["--state-dir", "state", "publish", "--batch", &id])
                .env("LD_PRELOAD", &fault)
                .env("PROGRESS_FAULT", phase)
                .env("PROGRESS_EFFECT", effect)
                .env("PROGRESS_BATCH", f.directory.path().join("state").join(&id))
                .output()?;
            assert_that!(output.status.success(), eq(false));
            assert_that!(receipt.exists(), eq(true));
            if effect == "error" {
                let report: Value = serde_json::from_slice(&output.stdout)?;
                assert_that!(report["status"].as_str(), eq(Some("uncertain")));
            }
            let before = if f.directory.path().join("remote.log").exists() {
                f.remote()?["creations"].as_u64().unwrap()
            } else {
                0
            };
            assert_that!(before, eq(u64::from(!phase.starts_with("intent"))));
            let resumed = f.run(&["--state-dir", "state", "publish", "--batch", &id])?;
            if phase == "intent-directory" {
                assert_that!(resumed.status.success(), eq(false));
                assert_that!(f.directory.path().join("remote.log").exists(), eq(false));
                let report: Value = serde_json::from_slice(&resumed.stdout)?;
                assert_that!(report["status"].as_str(), eq(Some("uncertain")));
            } else {
                anyhow::ensure!(
                    resumed.status.success(),
                    "restart failed: {}",
                    String::from_utf8_lossy(&resumed.stdout)
                );
                assert_that!(f.remote()?["creations"].as_u64(), eq(Some(2)));
            }
        }
    }
    Ok(())
}

#[googletest::test]
fn repeated_preparation_reports_preserved_publication_progress() -> anyhow::Result<()> {
    let f = Fixture::with_github()?;
    f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
    let id = f.prepare()?;
    assert_that!(
        f.run(&["--state-dir", "state", "publish", "--batch", &id])?
            .status
            .success(),
        eq(false)
    );
    for complete_creation in [false, true] {
        if complete_creation {
            f.successful(&["--state-dir", "state", "publish", "--batch", &id])?;
        }
        let remote = f.remote()?;
        let snapshot = fs::read(
            f.directory
                .path()
                .join("state")
                .join(&id)
                .join("batch.json"),
        )?;
        let result = f.successful(&[
            "--state-dir",
            "state",
            "prepare",
            "--plan",
            "plan.json",
            "--batch",
            &id,
        ])?;
        assert_that!(result["status"].as_str(), eq(Some("partial")));
        let status = f.successful(&["--state-dir", "state", "status", "--batch", &id])?;
        assert_that!(status["status"].as_str(), eq(Some("partial")));
        assert_that!(
            fs::read(
                f.directory
                    .path()
                    .join("state")
                    .join(&id)
                    .join("batch.json")
            )?,
            eq(&snapshot)
        );
        assert_that!(f.remote()?, eq(&remote));
    }
    Ok(())
}

#[googletest::test]
fn explicit_adoption_checks_the_issue_and_never_publishes() -> anyhow::Result<()> {
    let f = Fixture::with_github()?;
    f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
    let id = f.prepare()?;
    assert_that!(
        f.run(&["--state-dir", "state", "publish", "--batch", &id])?
            .status
            .success(),
        eq(false)
    );
    let remote = f.remote()?;
    let result = f.successful(&[
        "--state-dir",
        "state",
        "recover",
        "adopt",
        "--batch",
        &id,
        "--ticket",
        "first",
        "--issue",
        "3000",
    ])?;
    assert_that!(
        result["tickets"][0]["status"]["created"]["number"].as_u64(),
        eq(Some(3000))
    );
    assert_that!(f.remote()?, eq(&remote));
    f.successful(&["--state-dir", "state", "publish", "--batch", &id])?;
    assert_that!(f.remote()?, eq(&remote));
    Ok(())
}

#[googletest::test]
fn uncertain_retry_requires_durable_explicit_authorization_and_fresh_reads() -> anyhow::Result<()> {
    for remote_appears in [false, true] {
        let f = Fixture::with_github()?;
        f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
        let id = f.prepare()?;
        assert_that!(
            f.run(&["--state-dir", "state", "publish", "--batch", &id])?
                .status
                .success(),
            eq(false)
        );
        let original = f.remote()?;
        f.save_remote(&json!({"creations":0,"issues":[]}))?;
        let snapshot = f
            .directory
            .path()
            .join("state")
            .join(&id)
            .join("batch.json");
        let before = fs::read(&snapshot)?;
        assert_that!(
            f.run(&[
                "--state-dir",
                "state",
                "recover",
                "authorize-retry",
                "--batch",
                &id,
                "--ticket",
                "first"
            ])?
            .status
            .success(),
            eq(false)
        );
        assert_that!(fs::read(&snapshot)?, eq(&before));
        f.successful(&[
            "--state-dir",
            "state",
            "recover",
            "authorize-retry",
            "--batch",
            &id,
            "--ticket",
            "first",
            "--acknowledge-duplicate-risk",
        ])?;
        assert_that!(f.remote()?["creations"].as_u64(), eq(Some(0)));
        // An issue can become visible after authorization. Publication must reuse it.
        if remote_appears {
            f.save_remote(&original)?;
        }
        f.successful(&["--state-dir", "state", "publish", "--batch", &id])?;
        assert_that!(f.remote()?["creations"].as_u64(), eq(Some(1)));
    }
    Ok(())
}

#[googletest::test]
fn reconstruction_preserves_original_identities_and_requires_a_stopped_publisher()
-> anyhow::Result<()> {
    for use_export in [false, true] {
        let f = Fixture::with_github()?;
        let id = f.prepare()?;
        let exported = f.successful(&["--state-dir", "state", "export", "--batch", &id])?;
        fs::write(
            f.directory.path().join("export.json"),
            serde_json::to_vec(&exported)?,
        )?;
        f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
        assert_that!(
            f.run(&["--state-dir", "state", "publish", "--batch", &id])?
                .status
                .success(),
            eq(false)
        );
        let remote = f.remote()?;
        let source = if use_export { "--export" } else { "--plan" };
        let file = if use_export {
            "export.json"
        } else {
            "plan.json"
        };
        let args = [
            "--state-dir",
            "restored",
            "recover",
            "reconstruct",
            "--batch",
            &id,
            source,
            file,
        ];
        assert_that!(f.run(&args)?.status.success(), eq(false));
        assert_that!(f.directory.path().join("restored").exists(), eq(false));
        let mut authorized = args.to_vec();
        authorized.push("--publisher-stopped");
        let result = f.successful(&authorized)?;
        assert_that!(result["batch_id"].as_str(), eq(Some(id.as_str())));
        assert_that!(
            result["tickets"][0]["status"]["created"]["number"].as_u64(),
            eq(Some(3000))
        );
        assert_that!(f.remote()?, eq(&remote));
        f.successful(&["--state-dir", "restored", "publish", "--batch", &id])?;
        assert_that!(f.remote()?, eq(&remote));
        // Existing state is evidence. Reconstruction must never replace it.
        let saved = f
            .directory
            .path()
            .join("restored")
            .join(&id)
            .join("batch.json");
        fs::write(&saved, b"corrupt evidence")?;
        assert_that!(f.run(&authorized)?.status.success(), eq(false));
        assert_that!(fs::read(saved)?, eq(b"corrupt evidence"));
    }
    Ok(())
}

#[googletest::test]
fn status_checks_remote_conflicts_and_uncertainty_without_saving_or_publishing()
-> anyhow::Result<()> {
    let f = Fixture::with_github()?;
    f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
    let id = f.prepare()?;
    assert_that!(
        f.run(&["--state-dir", "state", "publish", "--batch", &id])?
            .status
            .success(),
        eq(false)
    );
    let original = f.remote()?;
    let snapshot = f
        .directory
        .path()
        .join("state")
        .join(&id)
        .join("batch.json");
    let before = fs::read(&snapshot)?;
    for mode in ["absent", "content", "permission", "incomplete"] {
        let mut remote = original.clone();
        match mode {
            "absent" => remote["issues"] = json!([]),
            "content" => remote["issues"][0]["title"] = json!("External change"),
            _ => remote["mode"] = json!(mode),
        }
        f.save_remote(&remote)?;
        let output = f.run(&["--state-dir", "state", "status", "--batch", &id])?;
        let result: Value = serde_json::from_slice(&output.stdout)?;
        assert_that!(
            result["status"].as_str(),
            eq(Some(if mode == "content" {
                "conflict"
            } else if mode == "absent" {
                "uncertain"
            } else {
                "unverified"
            }))
        );
        assert_that!(output.status.success(), eq(mode == "absent"));
        assert_that!(fs::read(&snapshot)?, eq(&before));
        assert_that!(f.remote()?, eq(&remote));
    }
    Ok(())
}

#[googletest::test]
fn rejected_adoptions_preserve_saved_and_remote_evidence() -> anyhow::Result<()> {
    for field in ["title", "body", "repository_url", "pull_request"] {
        let f = Fixture::with_github()?;
        f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
        let id = f.prepare()?;
        assert_that!(
            f.run(&["--state-dir", "state", "publish", "--batch", &id])?
                .status
                .success(),
            eq(false)
        );
        let mut remote = f.remote()?;
        remote["issues"][0][field] = if field == "pull_request" {
            json!({"url":"example"})
        } else {
            json!("External change")
        };
        f.save_remote(&remote)?;
        let snapshot = f
            .directory
            .path()
            .join("state")
            .join(&id)
            .join("batch.json");
        let before = fs::read(&snapshot)?;
        assert_that!(
            f.run(&[
                "--state-dir",
                "state",
                "recover",
                "adopt",
                "--batch",
                &id,
                "--ticket",
                "first",
                "--issue",
                "3000"
            ])?
            .status
            .success(),
            eq(false)
        );
        assert_that!(fs::read(snapshot)?, eq(&before));
        assert_that!(f.remote()?, eq(&remote));
    }
    Ok(())
}

#[googletest::test]
fn explicit_adoption_resolves_multiple_matches_but_cannot_replace_a_saved_mapping()
-> anyhow::Result<()> {
    let f = Fixture::with_github()?;
    f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
    let id = f.prepare()?;
    assert_that!(
        f.run(&["--state-dir", "state", "publish", "--batch", &id])?
            .status
            .success(),
        eq(false)
    );
    let mut remote = f.remote()?;
    let mut duplicate = remote["issues"][0].clone();
    duplicate["number"] = json!(3001);
    duplicate["id"] = json!(13001);
    duplicate["node_id"] = json!("I_3001");
    remote["issues"].as_array_mut().unwrap().push(duplicate);
    f.save_remote(&remote)?;
    assert_that!(
        f.run(&["--state-dir", "state", "publish", "--batch", &id])?
            .status
            .success(),
        eq(false)
    );
    f.successful(&[
        "--state-dir",
        "state",
        "recover",
        "adopt",
        "--batch",
        &id,
        "--ticket",
        "first",
        "--issue",
        "3001",
    ])?;
    f.successful(&["--state-dir", "state", "publish", "--batch", &id])?;
    let snapshot = f
        .directory
        .path()
        .join("state")
        .join(&id)
        .join("batch.json");
    let before = fs::read(&snapshot)?;
    assert_that!(
        f.run(&[
            "--state-dir",
            "state",
            "recover",
            "adopt",
            "--batch",
            &id,
            "--ticket",
            "first",
            "--issue",
            "3000"
        ])?
        .status
        .success(),
        eq(false)
    );
    assert_that!(fs::read(snapshot)?, eq(&before));
    assert_that!(f.remote()?, eq(&remote));
    Ok(())
}

#[googletest::test]
fn reconstructed_absence_never_authorizes_creation_and_invalid_inputs_preserve_evidence()
-> anyhow::Result<()> {
    let f = Fixture::with_github()?;
    let id = f.prepare()?;
    let exported = f.successful(&["--state-dir", "state", "export", "--batch", &id])?;
    fs::write(
        f.directory.path().join("export.json"),
        serde_json::to_vec(&exported)?,
    )?;
    assert_that!(
        f.run(&[
            "--state-dir",
            "missing",
            "recover",
            "reconstruct",
            "--batch",
            &id,
            "--publisher-stopped"
        ])?
        .status
        .success(),
        eq(false)
    );
    assert_that!(f.directory.path().join("missing").exists(), eq(false));
    f.successful(&[
        "--state-dir",
        "restored",
        "recover",
        "reconstruct",
        "--batch",
        &id,
        "--export",
        "export.json",
        "--publisher-stopped",
    ])?;
    let output = f.run(&["--state-dir", "restored", "publish", "--batch", &id])?;
    assert_that!(output.status.success(), eq(false));
    let result: Value = serde_json::from_slice(&output.stdout)?;
    assert_that!(result["status"].as_str(), eq(Some("uncertain")));
    assert_that!(f.directory.path().join("remote.log").exists(), eq(false));
    let mut changed = exported;
    changed["plan"]["tickets"][0]["title"] = json!("Changed plan");
    let changed_bytes = serde_json::to_vec(&changed)?;
    fs::write(f.directory.path().join("export.json"), &changed_bytes)?;
    assert_that!(
        f.run(&[
            "--state-dir",
            "changed",
            "recover",
            "reconstruct",
            "--batch",
            &id,
            "--export",
            "export.json",
            "--publisher-stopped"
        ])?
        .status
        .success(),
        eq(false)
    );
    assert_that!(f.directory.path().join("changed").exists(), eq(false));
    assert_that!(
        fs::read(f.directory.path().join("export.json"))?,
        eq(&changed_bytes)
    );
    Ok(())
}

#[googletest::test]
fn authorized_publication_stops_on_new_conflicting_or_incomplete_evidence() -> anyhow::Result<()> {
    for mode in ["multiple", "content", "permission", "incomplete"] {
        let f = Fixture::with_github()?;
        f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
        let id = f.prepare()?;
        assert_that!(
            f.run(&["--state-dir", "state", "publish", "--batch", &id])?
                .status
                .success(),
            eq(false)
        );
        let mut remote = f.remote()?;
        f.successful(&[
            "--state-dir",
            "state",
            "recover",
            "authorize-retry",
            "--batch",
            &id,
            "--ticket",
            "first",
            "--acknowledge-duplicate-risk",
        ])?;
        match mode {
            "multiple" => {
                let mut duplicate = remote["issues"][0].clone();
                duplicate["number"] = json!(3001);
                remote["issues"].as_array_mut().unwrap().push(duplicate);
            }
            "content" => remote["issues"][0]["title"] = json!("External change"),
            _ => remote["mode"] = json!(mode),
        }
        f.save_remote(&remote)?;
        assert_that!(
            f.run(&["--state-dir", "state", "publish", "--batch", &id])?
                .status
                .success(),
            eq(false)
        );
        assert_that!(f.remote()?, eq(&remote));
    }
    Ok(())
}

#[googletest::test]
fn interrupted_authorization_writes_never_publish_and_can_resume_explicitly() -> anyhow::Result<()>
{
    let files = Runfiles::create()?;
    let fault = rlocation!(files, std::env::var("SYNC_FAULT_LIBRARY")?).context("sync fault")?;
    for phase in ["authorization-file", "authorization-directory"] {
        for effect in ["error", "exit"] {
            let f = Fixture::with_github()?;
            f.save_remote(&json!({"creations":0,"issues":[],"mode":"lose-response"}))?;
            let id = f.prepare()?;
            assert_that!(
                f.run(&["--state-dir", "state", "publish", "--batch", &id])?
                    .status
                    .success(),
                eq(false)
            );
            let remote = json!({"creations":0,"issues":[]});
            f.save_remote(&remote)?;
            let args = [
                "--state-dir",
                "state",
                "recover",
                "authorize-retry",
                "--batch",
                &id,
                "--ticket",
                "first",
                "--acknowledge-duplicate-risk",
            ];
            let failed = f
                .command(&args)
                .env("LD_PRELOAD", &fault)
                .env("PROGRESS_FAULT", phase)
                .env("PROGRESS_EFFECT", effect)
                .env("PROGRESS_BATCH", f.directory.path().join("state").join(&id))
                .output()?;
            assert_that!(failed.status.success(), eq(false));
            assert_that!(
                f.directory.path().join("progress-fault-receipt").exists(),
                eq(true)
            );
            assert_that!(f.remote()?, eq(&remote));
            if phase == "authorization-file" {
                assert_that!(
                    f.run(&["--state-dir", "state", "publish", "--batch", &id])?
                        .status
                        .success(),
                    eq(false)
                );
                assert_that!(f.remote()?, eq(&remote));
            }
            f.successful(&args)?;
            assert_that!(f.remote()?, eq(&remote));
            f.successful(&["--state-dir", "state", "publish", "--batch", &id])?;
            assert_that!(f.remote()?["creations"].as_u64(), eq(Some(1)));
        }
    }
    Ok(())
}
