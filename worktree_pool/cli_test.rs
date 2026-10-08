use std::{
    env,
    ffi::OsString,
    fs,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    },
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use googletest::{assert_that, matchers::eq};
use serde_json::Value;

struct Fixture {
    root: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().unwrap(),
        }
    }
    fn command(&self) -> Command {
        let binary = PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
            .join(env::var_os("TEST_WORKSPACE").unwrap())
            .join(env!("POOL_BINARY"));
        let mut command = Command::new(binary);
        command
            .env_clear()
            .env("PATH", env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.root.path())
            .env("XDG_STATE_HOME", self.root.path().join("state"))
            .env("XDG_DATA_HOME", self.root.path().join("data"))
            .env("XDG_CONFIG_HOME", self.root.path().join("config"))
            .arg("--json")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    fn run(&self, args: &[&str]) -> Output {
        let mut command = self.command();
        command.args(args);
        output(command)
    }
    fn json(&self, args: &[&str]) -> Value {
        let result = self.run(args);
        serde_json::from_slice(&result.stdout).unwrap()
    }
    fn database(&self) -> PathBuf {
        self.root.path().join("data/worktree-pool/catalog.redb")
    }
}
fn wait(mut child: Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            panic!(
                "CLI exceeded bounded watchdog: {:?}",
                child.wait_with_output().unwrap()
            );
        }
        thread::sleep(Duration::from_millis(5));
    }
}
fn output(mut command: Command) -> Output {
    wait(command.spawn().unwrap())
}

#[googletest::test]
fn catalog_is_explicit_and_reopens_with_the_same_identity() {
    let fixture = Fixture::new();
    assert_that!(fixture.run(&["catalog", "info"]).status.code(), eq(Some(2)));
    assert_that!(fixture.database().exists(), eq(false));
    let initial = fixture.json(&["catalog", "init"]);
    let info = fixture.json(&["catalog", "info"]);
    assert_that!(
        &info["context"]["catalog_id"],
        eq(&initial["context"]["catalog_id"])
    );
    assert_that!(info["data"]["revision"].as_u64(), eq(Some(1)));
    let before = fixture.json(&["events", "list"]);
    assert_that!(
        fixture.run(&["catalog", "check"]).status.code(),
        eq(Some(0))
    );
    assert_that!(fixture.run(&["catalog", "init"]).status.code(), eq(Some(2)));
    assert_that!(&fixture.json(&["events", "list"]), eq(&before));
    let event = &before["data"]["events"][0];
    assert_that!(event["specversion"].as_str(), eq(Some("1.0")));
    assert_that!(
        event["type"].as_str(),
        eq(Some("io.lowkeylab.worktreepool.catalog.initialized.v1"))
    );
    assert_that!(
        event["datacontenttype"].as_str(),
        eq(Some("application/json"))
    );
    assert_that!(event["operationid"].is_string(), eq(true));
}
#[googletest::test]
fn human_initialization_reports_the_catalog_identity_and_location() {
    let fixture = Fixture::new();
    let mut command = fixture.command();
    // Explicit false output setting after the shared helper's --json is not supported;
    // invoke a fresh command using the identical isolated environment.
    command = Command::new(command.get_program());
    command
        .env_clear()
        .env("HOME", fixture.root.path())
        .args(["catalog", "init"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let text = String::from_utf8(output(command).stdout).unwrap();
    assert_that!(text.contains("catalog_id"), eq(true));
    assert_that!(text.contains("catalog.redb"), eq(true));
}
#[googletest::test]
fn malformed_input_has_one_stable_sanitized_json_result() {
    let fixture = Fixture::new();
    let result = fixture.run(&["catalog", "secret-not-a-command"]);
    assert_that!(result.status.code(), eq(Some(2)));
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    insta::assert_yaml_snapshot!(value, @r###"
command: parse
context: {}
data:
  next_action: use --help to inspect supported arguments
outcome: rejected
reason_code: invalid_arguments
schema_version: 1
warnings: []
"###);
    assert_that!(
        String::from_utf8_lossy(&result.stderr).contains("secret-not-a-command"),
        eq(false)
    );
}
#[googletest::test]
fn missing_or_corrupt_active_storage_is_never_reinitialized() {
    for missing in [false, true] {
        let fixture = Fixture::new();
        fixture.json(&["catalog", "init"]);
        if missing {
            fs::remove_file(fixture.database()).unwrap();
        } else {
            fs::write(fixture.database(), b"protected corrupt catalog").unwrap();
        }
        assert_that!(
            fixture.run(&["catalog", "info"]).status.success(),
            eq(false)
        );
        assert_that!(
            fixture.run(&["catalog", "init"]).status.success(),
            eq(false)
        );
        if missing {
            assert_that!(fixture.database().exists(), eq(false));
        } else {
            assert_that!(
                fs::read(fixture.database()).unwrap().as_slice(),
                eq(b"protected corrupt catalog".as_slice())
            );
        }
    }
}
#[googletest::test]
fn catalog_paths_preserve_unix_bytes_and_private_permissions() {
    let fixture = Fixture::new();
    let directory = fixture
        .root
        .path()
        .join(OsString::from_vec(vec![b'p', 0xff]));
    let mut command = fixture.command();
    command
        .arg("--catalog-dir")
        .arg(&directory)
        .args(["catalog", "init"]);
    let initialized: Value = serde_json::from_slice(&output(command).stdout).unwrap();
    let expected = directory.join("catalog.redb");
    let bytes: Vec<u8> =
        serde_json::from_value(initialized["context"]["catalog_path"]["bytes"].clone()).unwrap();
    assert_that!(bytes.as_slice(), eq(expected.as_os_str().as_bytes()));
    let mut inspect = fixture.command();
    inspect
        .arg("--catalog-dir")
        .arg(&directory)
        .args(["catalog", "check"]);
    assert_that!(output(inspect).status.code(), eq(Some(0)));
    assert_that!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        eq(0o700)
    );
    assert_that!(
        fs::metadata(expected).unwrap().permissions().mode() & 0o777,
        eq(0o600)
    );
}
#[googletest::test]
fn location_precedence_never_creates_a_second_authority() {
    let fixture = Fixture::new();
    let config_dir = fixture.root.path().join("configured");
    let env_dir = fixture.root.path().join("environment");
    let flag_dir = fixture.root.path().join("flagged");
    let config = fixture.root.path().join("settings.toml");
    fs::write(
        &config,
        format!(
            "catalog_dir = {:?}\njson = true\n",
            config_dir.display().to_string()
        ),
    )
    .unwrap();
    let mut command = fixture.command();
    command
        .arg("--config")
        .arg(&config)
        .env("WORKTREE_POOL_CATALOG_DIR", &env_dir)
        .arg("--catalog-dir")
        .arg(&flag_dir)
        .args(["catalog", "init"]);
    assert_that!(output(command).status.code(), eq(Some(0)));
    assert_that!(flag_dir.join("catalog.redb").exists(), eq(true));
    assert_that!(env_dir.exists(), eq(false));
    assert_that!(config_dir.exists(), eq(false));
    let mut conflicting = fixture.command();
    conflicting
        .arg("--config")
        .arg(&config)
        .env("WORKTREE_POOL_CATALOG_DIR", &env_dir)
        .args(["catalog", "init"]);
    let result = output(conflicting);
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_that!(value["reason_code"].as_str(), eq(Some("catalog_conflict")));
    assert_that!(env_dir.exists(), eq(false));
}
#[googletest::test]
fn invalid_configuration_does_not_disclose_contents_or_create_state() {
    let fixture = Fixture::new();
    let file = fixture.root.path().join("bad.toml");
    fs::write(&file, "secret-access-token = [invalid").unwrap();
    let mut command = fixture.command();
    command.arg("--config").arg(file).args(["catalog", "init"]);
    let result = output(command);
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_that!(
        value["reason_code"].as_str(),
        eq(Some("invalid_configuration"))
    );
    assert_that!(
        String::from_utf8_lossy(&result.stdout).contains("secret-access-token"),
        eq(false)
    );
    assert_that!(
        String::from_utf8_lossy(&result.stderr).contains("secret-access-token"),
        eq(false)
    );
    assert_that!(fixture.database().exists(), eq(false));
}
#[googletest::test]
fn concurrent_initialization_publishes_one_catalog_identity() {
    let fixture = Fixture::new();
    let state = fixture.root.path().join("state/worktree-pool");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&state)
        .unwrap();
    let barrier = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(state.join("maintenance.lock"))
        .unwrap();
    barrier.lock().unwrap();
    let mut first = fixture.command();
    first.args(["catalog", "init"]);
    let first = first.spawn().unwrap();
    let mut second = fixture.command();
    second.args(["catalog", "init"]);
    let second = second.spawn().unwrap();
    barrier.unlock().unwrap();
    let mut statuses = [
        wait(first).status.code().unwrap(),
        wait(second).status.code().unwrap(),
    ];
    statuses.sort_unstable();
    assert_that!(statuses, eq([0, 2]));
    assert_that!(
        fixture.json(&["events", "list"])["data"]["events"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
}
#[googletest::test]
fn lost_stdout_does_not_repeat_committed_initialization() {
    let fixture = Fixture::new();
    let mut command = fixture.command();
    command.args(["catalog", "init"]).stdout(
        fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap(),
    );
    assert_that!(output(command).status.code(), eq(Some(4)));
    assert_that!(
        fixture.json(&["catalog", "info"])["data"]["revision"].as_u64(),
        eq(Some(1))
    );
    assert_that!(fixture.run(&["catalog", "init"]).status.code(), eq(Some(2)));
    assert_that!(
        fixture.json(&["events", "list"])["data"]["events"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
}

#[googletest::test]
fn process_death_preserves_initialization_intent_commit_and_publication() {
    use std::io::{BufRead, BufReader};
    for (checkpoint, expected_status, database_exists) in [
        ("intent", 3, false),
        ("store", 3, true),
        ("published", 0, true),
    ] {
        let fixture = Fixture::new();
        let binary = PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
            .join(env::var_os("TEST_WORKSPACE").unwrap())
            .join(env!("CRASH_BINARY"));
        let mut command = fixture.command();
        let environments: Vec<_> = command
            .get_envs()
            .map(|(name, value)| (name.to_owned(), value.map(std::ffi::OsStr::to_owned)))
            .collect();
        command = Command::new(binary);
        command.env_clear();
        for (name, value) in environments {
            if let Some(value) = value {
                command.env(name, value);
            }
        }
        command
            .env("CHECKPOINT", checkpoint)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let reader = thread::spawn(move || {
            let mut line = String::new();
            BufReader::new(stdout).read_line(&mut line).unwrap();
            let _ = sender.send(line);
        });
        let observed = receiver.recv_timeout(Duration::from_secs(15));
        child.kill().unwrap();
        let _ = child.wait_with_output().unwrap();
        reader.join().unwrap();
        assert_that!(
            observed.unwrap().as_str(),
            eq(format!("checkpoint:{checkpoint}\n").as_str())
        );
        assert_that!(fixture.database().exists(), eq(database_exists));
        let info = fixture.run(&["catalog", "info"]);
        assert_that!(info.status.code(), eq(Some(expected_status)));
        let info: Value = serde_json::from_slice(&info.stdout).unwrap();
        assert_that!(info["context"]["catalog_id"].is_string(), eq(true));
        if expected_status != 0 {
            assert_that!(info["data"]["phase"].as_str(), eq(Some("initializing")));
            assert_that!(
                info["data"]["last_checkpoint"].as_str(),
                eq(Some(if database_exists {
                    "store_committed"
                } else {
                    "intent_recorded"
                }))
            );
        }
        assert_that!(
            fixture.run(&["catalog", "init"]).status.code(),
            eq(Some(if expected_status == 0 { 2 } else { 3 }))
        );
        if expected_status == 0 {
            assert_that!(
                fixture.json(&["events", "list"])["data"]["events"]
                    .as_array()
                    .unwrap()
                    .len(),
                eq(1)
            );
        }
    }
}

impl Fixture {
    fn git(&self, path: &std::path::Path, args: &[&std::ffi::OsStr]) {
        let result = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .env_clear()
            .env("PATH", env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.root.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.test")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.test")
            .output()
            .unwrap();
        assert_that!(result.status.success(), eq(true));
    }
    fn repository(&self) -> PathBuf {
        let path = self.root.path().join("repository");
        fs::create_dir(&path).unwrap();
        self.git(&path, &["init".as_ref(), "--initial-branch=main".as_ref()]);
        fs::write(path.join("tracked"), b"protected checkout\n").unwrap();
        self.git(&path, &["add".as_ref(), ".".as_ref()]);
        self.git(
            &path,
            &["commit".as_ref(), "-m".as_ref(), "fixture".as_ref()],
        );
        path
    }
    fn path_json(&self, args: &[&str], path: &std::path::Path) -> Value {
        let mut command = self.command();
        command
            .env("PATH", env::var_os("PATH").unwrap())
            .args(args)
            .arg(path);
        serde_json::from_slice(&output(command).stdout).unwrap()
    }
}

#[googletest::test]
fn explicit_registration_separates_repository_and_managed_worktree_visibility() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let repository = fixture.repository();
    let registered = fixture.path_json(&["repo", "register"], &repository);
    assert_that!(registered["outcome"].as_str(), eq(Some("completed")));
    let repo_id = registered["context"]["repository_id"].as_str().unwrap();
    assert_that!(
        fixture.json(&["repo", "list"])["data"]["repositories"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
    assert_that!(
        fixture.json(&["worktree", "list"])["data"]["worktrees"]
            .as_array()
            .unwrap()
            .len(),
        eq(0)
    );
    let worktree = fixture.path_json(&["worktree", "register", "--repo", repo_id], &repository);
    assert_that!(worktree["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        &worktree["context"]["repository_id"],
        eq(&registered["context"]["repository_id"])
    );
    assert_that!(
        worktree["data"]["worktree"]["ownership"].as_str(),
        eq(Some("unassigned"))
    );
    assert_that!(
        fs::read(repository.join("tracked")).unwrap().as_slice(),
        eq(b"protected checkout\n".as_slice())
    );
}

#[googletest::test]
fn registered_repositories_maintain_independent_resource_stream_revisions() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let first = fixture.repository();
    let second = fixture.root.path().join("clone");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), first.as_os_str(), second.as_os_str()],
    );
    for path in [&first, &second] {
        let registered = fixture.path_json(&["repo", "register"], path);
        assert_that!(
            registered["data"]["repository"]["revision"].as_u64(),
            eq(Some(0))
        );
        let id = registered["context"]["repository_id"].as_str().unwrap();
        fixture.path_json(&["worktree", "register", "--repo", id], path);
        assert_that!(
            fixture.json(&["repo", "inspect", id])["data"]["repository"]["revision"].as_u64(),
            eq(Some(1))
        );
    }
    assert_that!(
        fixture.json(&["catalog", "info"])["data"]["revision"].as_u64(),
        eq(Some(5))
    );
}

#[googletest::test]
fn refresh_records_intent_and_resolves_fresh_origin_main_without_changing_checkout() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let source = fixture.repository();
    let remote = fixture.root.path().join("remote.git");
    fixture.git(
        fixture.root.path(),
        &[
            "clone".as_ref(),
            "--bare".as_ref(),
            source.as_os_str(),
            remote.as_os_str(),
        ],
    );
    let checkout = fixture.root.path().join("checkout");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), remote.as_os_str(), checkout.as_os_str()],
    );
    let registered = fixture.path_json(&["repo", "register"], &checkout);
    let id = registered["context"]["repository_id"].as_str().unwrap();
    fs::write(source.join("new-file"), b"new main\n").unwrap();
    fixture.git(&source, &["add".as_ref(), ".".as_ref()]);
    fixture.git(
        &source,
        &["commit".as_ref(), "-m".as_ref(), "new-main".as_ref()],
    );
    fixture.git(
        &source,
        &["push".as_ref(), remote.as_os_str(), "main".as_ref()],
    );
    let refreshed = fixture.json(&["repo", "refresh", "--repo", id]);
    assert_that!(refreshed["outcome"].as_str(), eq(Some("completed")));
    assert_that!(refreshed["context"]["operation_id"].is_string(), eq(true));
    assert_that!(
        refreshed["data"]["operation"]["resolved_commit"]
            .as_str()
            .unwrap()
            .len(),
        eq(40)
    );
    assert_that!(checkout.join("new-file").exists(), eq(false));
    let events = fixture.json(&["events", "list"]);
    let events = events["data"]["events"].as_array().unwrap();
    assert_that!(
        events[2]["type"].as_str(),
        eq(Some(
            "io.lowkeylab.worktreepool.repository.refresh.started.v1"
        ))
    );
    assert_that!(
        events[3]["type"].as_str(),
        eq(Some(
            "io.lowkeylab.worktreepool.repository.refresh.finished.v1"
        ))
    );
    assert_that!(
        &events[2]["operationid"],
        eq(&refreshed["context"]["operation_id"])
    );
    assert_that!(
        &events[3]["operationid"],
        eq(&refreshed["context"]["operation_id"])
    );
    assert_that!(&events[3]["causationid"], eq(&events[2]["id"]));
    assert_that!(
        events[1]["subject"].as_str(),
        eq(Some(format!("repositories/{id}").as_str()))
    );
}

#[googletest::test]
fn copied_git_pointer_cannot_enroll_an_unlisted_checkout() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let repository = fixture.repository();
    fixture.path_json(&["repo", "register"], &repository);
    let impostor = fixture.root.path().join("impostor");
    fs::create_dir(&impostor).unwrap();
    fs::write(
        impostor.join(".git"),
        format!("gitdir: {}\n", repository.join(".git").display()),
    )
    .unwrap();
    let before = fixture.json(&["events", "list"]);
    let result = fixture.path_json(&["worktree", "register"], &impostor);
    assert_that!(result["outcome"].as_str(), eq(Some("rejected")));
    assert_that!(&fixture.json(&["events", "list"]), eq(&before));
}

#[googletest::test]
fn linked_enrollment_preserves_byte_paths_ignored_state_and_capacity_after_removal() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let repository = fixture.repository();
    let registered = fixture.path_json(&["repo", "register"], &repository);
    let id = registered["context"]["repository_id"].as_str().unwrap();
    let mut paths = Vec::new();
    for number in 0..5 {
        let path = fixture.root.path().join(OsString::from_vec(vec![
            b'w',
            b'0' + number,
            0xff,
            b'\n',
            b' ',
        ]));
        fixture.git(
            &repository,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "--detach".as_ref(),
                path.as_os_str(),
                "HEAD".as_ref(),
            ],
        );
        paths.push(path);
    }
    assert_that!(
        fixture.path_json(&["worktree", "inspect"], &paths[4])["reason_code"].as_str(),
        eq(Some("resource_unregistered"))
    );
    for path in &paths[..4] {
        fs::write(path.join(".gitignore"), b"cache/\n").unwrap();
        fs::write(path.join("tracked"), b"unfinished tracked work\n").unwrap();
        fs::create_dir(path.join("cache")).unwrap();
        fs::write(path.join("cache/build-state"), b"warm bytes").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o751)).unwrap();
        let worktree = fixture.path_json(&["worktree", "register", "--repo", id], path);
        assert_that!(worktree["outcome"].as_str(), eq(Some("completed")));
        let bytes: Vec<u8> =
            serde_json::from_value(worktree["data"]["worktree"]["path"]["bytes"].clone()).unwrap();
        assert_that!(bytes.as_slice(), eq(path.as_os_str().as_bytes()));
        assert_that!(
            fs::read(path.join("tracked")).unwrap().as_slice(),
            eq(b"unfinished tracked work\n".as_slice())
        );
        assert_that!(
            fs::read(path.join("cache/build-state")).unwrap().as_slice(),
            eq(b"warm bytes".as_slice())
        );
        assert_that!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            eq(0o751)
        );
        assert_that!(
            &fixture.path_json(&["repo", "register"], path)["context"]["repository_id"],
            eq(&registered["context"]["repository_id"])
        );
    }
    let moved = fixture.root.path().join("moved-existing-worktree");
    fixture.git(
        &repository,
        &[
            "worktree".as_ref(),
            "move".as_ref(),
            paths[1].as_os_str(),
            moved.as_os_str(),
        ],
    );
    assert_that!(fixture.path_json(&["worktree", "inspect"], &paths[1])["data"]["worktree"]["registration_state"].as_str(), eq(Some("missing")));
    assert_that!(
        fixture.path_json(&["worktree", "inspect"], &moved)["reason_code"].as_str(),
        eq(Some("resource_unregistered"))
    );
    assert_that!(
        fixture.path_json(&["worktree", "register", "--repo", id], &moved)["reason_code"].as_str(),
        eq(Some("selector_conflict"))
    );
    fs::remove_dir_all(&paths[0]).unwrap();
    let missing = fixture.path_json(&["worktree", "inspect"], &paths[0]);
    assert_that!(
        missing["data"]["worktree"]["registration_state"].as_str(),
        eq(Some("missing"))
    );
    assert_that!(
        missing["data"]["worktree"]["availability"].as_str(),
        eq(Some("withheld"))
    );
    let full = fixture.path_json(&["worktree", "register", "--repo", id], &paths[4]);
    assert_that!(full["reason_code"].as_str(), eq(Some("capacity_exhausted")));
    assert_that!(
        fixture.json(&["repo", "inspect", id])["data"]["registered_count"].as_u64(),
        eq(Some(4))
    );
    assert_that!(
        fixture.json(&["worktree", "list"])["data"]["worktrees"]
            .as_array()
            .unwrap()
            .len(),
        eq(4)
    );
}

#[googletest::test]
fn failed_refresh_is_durable_and_repeated_requests_never_report_stale_success() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let repository = fixture.repository();
    fixture.git(
        &repository,
        &[
            "update-ref".as_ref(),
            "refs/remotes/origin/main".as_ref(),
            "HEAD".as_ref(),
        ],
    );
    let registered = fixture.path_json(&["repo", "register"], &repository);
    let id = registered["context"]["repository_id"].as_str().unwrap();
    let result = fixture.json(&["repo", "refresh", "--repo", id]);
    assert_that!(result["outcome"].as_str(), eq(Some("pending")));
    assert_that!(result["reason_code"].as_str(), eq(Some("refresh_failed")));
    assert_that!(
        result["data"]["operation"]["resolved_commit"].is_null(),
        eq(true)
    );
    assert_that!(
        result["data"]["operation"]["last_checkpoint"].as_str(),
        eq(Some("fetch_failed"))
    );
    let before = fixture.json(&["events", "list"]);
    let repeated = fixture.json(&["repo", "refresh", "--repo", id]);
    assert_that!(
        &repeated["context"]["operation_id"],
        eq(&result["context"]["operation_id"])
    );
    assert_that!(&fixture.json(&["events", "list"]), eq(&before));
    assert_that!(
        fixture
            .run(&["repo", "refresh", "--repo", id])
            .status
            .code(),
        eq(Some(3))
    );
    let inspected = fixture.json(&["repo", "inspect", id]);
    assert_that!(
        &inspected["data"]["operations"][0]["operation_id"],
        eq(&result["context"]["operation_id"])
    );
}

impl Fixture {
    fn remote_checkout(&self) -> (PathBuf, PathBuf, PathBuf) {
        let source = self.repository();
        let remote = self.root.path().join("origin.git");
        self.git(
            self.root.path(),
            &[
                "clone".as_ref(),
                "--bare".as_ref(),
                source.as_os_str(),
                remote.as_os_str(),
            ],
        );
        let checkout = self.root.path().join("pool-checkout");
        self.git(
            self.root.path(),
            &["clone".as_ref(), remote.as_os_str(), checkout.as_os_str()],
        );
        (source, remote, checkout)
    }
    fn advance_main(&self, source: &std::path::Path, remote: &std::path::Path) {
        fs::write(source.join("next"), b"next main\n").unwrap();
        self.git(source, &["add".as_ref(), ".".as_ref()]);
        self.git(
            source,
            &["commit".as_ref(), "-m".as_ref(), "advance".as_ref()],
        );
        self.git(
            source,
            &["push".as_ref(), remote.as_os_str(), "main".as_ref()],
        );
    }
    fn git_stdout(&self, path: &std::path::Path, args: &[&str]) -> Vec<u8> {
        let result = Command::new("git")
            .env_clear()
            .env("PATH", env::var_os("PATH").unwrap())
            .env("HOME", self.root.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert_that!(result.status.success(), eq(true));
        result.stdout
    }
}

#[googletest::test]
fn selectors_and_ambient_git_context_cannot_redirect_registered_effects() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let (source, remote, checkout) = fixture.remote_checkout();
    let other = fixture.root.path().join("other");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), remote.as_os_str(), other.as_os_str()],
    );
    let stale = fixture.git_stdout(&other, &["rev-parse", "refs/remotes/origin/main"]);
    let registered = fixture.path_json(&["repo", "register"], &checkout);
    let other_registered = fixture.path_json(&["repo", "register"], &other);
    let id = registered["context"]["repository_id"].as_str().unwrap();
    let other_id = other_registered["context"]["repository_id"]
        .as_str()
        .unwrap();
    assert_that!(id == other_id, eq(false));
    let before = fixture.json(&["events", "list"]);
    let conflict = fixture.json(&["repo", "refresh", id, "--repo", other_id]);
    assert_that!(
        conflict["reason_code"].as_str(),
        eq(Some("selector_conflict"))
    );
    assert_that!(
        fixture.path_json(&["worktree", "register", "--repo", other_id], &checkout)["reason_code"]
            .as_str(),
        eq(Some("selector_conflict"))
    );
    assert_that!(&fixture.json(&["events", "list"]), eq(&before));
    fixture.advance_main(&source, &remote);
    let expected = fixture.git_stdout(&source, &["rev-parse", "HEAD"]);
    let mut command = fixture.command();
    command
        .env("GIT_DIR", other.join(".git"))
        .env("GIT_COMMON_DIR", other.join(".git"))
        .env("GIT_WORK_TREE", &other)
        .env("GIT_INDEX_FILE", other.join(".git/index"))
        .args(["repo", "refresh", "--repo", id]);
    let result: Value = serde_json::from_slice(&output(command).stdout).unwrap();
    assert_that!(result["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        format!(
            "{}\n",
            result["data"]["operation"]["resolved_commit"]
                .as_str()
                .unwrap()
        )
        .as_bytes(),
        eq(expected.as_slice())
    );
    assert_that!(
        fixture
            .git_stdout(&other, &["rev-parse", "refs/remotes/origin/main"])
            .as_slice(),
        eq(stale.as_slice())
    );
}

impl Fixture {
    fn paused_refresh(&self, repository_id: &str, checkpoint: &str) -> Child {
        use std::io::{BufRead, BufReader};
        let binary = PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
            .join(env::var_os("TEST_WORKSPACE").unwrap())
            .join(env!("CRASH_BINARY"));
        let command = self.command();
        let mut process = Command::new(binary);
        process.env_clear();
        for (name, value) in command.get_envs() {
            if let Some(value) = value {
                process.env(name, value);
            }
        }
        let mut child = process
            .env("OPERATION", "refresh")
            .env("REPOSITORY_ID", repository_id)
            .env("CHECKPOINT", checkpoint)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let reader = thread::spawn(move || {
            let mut line = String::new();
            BufReader::new(stdout).read_line(&mut line).unwrap();
            let _ = sender.send(line);
        });
        let observed = receiver.recv_timeout(Duration::from_secs(15));
        if observed.as_ref().is_err()
            || observed
                .as_ref()
                .is_ok_and(|line| line != &format!("checkpoint:{checkpoint}\n"))
        {
            let _ = child.kill();
            panic!(
                "refresh checkpoint missing: {observed:?}, {:?}",
                child.wait_with_output().unwrap()
            );
        }
        reader.join().unwrap();
        child
    }
}

#[googletest::test]
fn refresh_process_death_keeps_intent_or_complete_result_without_blind_retry() {
    for checkpoint in ["intent", "fetch", "result"] {
        let fixture = Fixture::new();
        fixture.json(&["catalog", "init"]);
        let (source, remote, checkout) = fixture.remote_checkout();
        let registered = fixture.path_json(&["repo", "register"], &checkout);
        let id = registered["context"]["repository_id"].as_str().unwrap();
        fixture.advance_main(&source, &remote);
        let worktree = fixture.path_json(&["worktree", "register", "--repo", id], &checkout);
        let mut child = fixture.paused_refresh(id, checkpoint);
        child.kill().unwrap();
        let _ = child.wait_with_output().unwrap();
        let inspected = fixture.json(&["repo", "inspect", id]);
        let operation = &inspected["data"]["operations"][0];
        assert_that!(
            operation["state"].as_str(),
            eq(Some(if checkpoint == "result" {
                "completed"
            } else {
                "pending"
            }))
        );
        assert_that!(
            operation["last_checkpoint"].as_str(),
            eq(Some(if checkpoint == "result" {
                "result_committed"
            } else {
                "intent_recorded"
            }))
        );
        if checkpoint != "result" {
            let worktree_id = worktree["context"]["worktree_id"].as_str().unwrap();
            let inspected_worktree = fixture.json(&["worktree", "inspect", worktree_id]);
            assert_that!(
                inspected_worktree["data"]["worktree"]["ownership"].as_str(),
                eq(Some("unassigned"))
            );
            assert_that!(
                inspected_worktree["data"]["worktree"]["availability"].as_str(),
                eq(Some("withheld"))
            );
            assert_that!(
                &inspected_worktree["data"]["worktree"]["pending_work"][0]["operation_id"],
                eq(&operation["operation_id"])
            );
            let before = fixture.json(&["events", "list"]);
            let retry = fixture.json(&["repo", "refresh", "--repo", id]);
            assert_that!(retry["outcome"].as_str(), eq(Some("pending")));
            assert_that!(
                &retry["context"]["operation_id"],
                eq(&operation["operation_id"])
            );
            assert_that!(&fixture.json(&["events", "list"]), eq(&before));
        }
    }
}

#[googletest::test]
fn parallel_refreshes_serialize_one_repository_while_another_can_finish() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let (_, remote, checkout) = fixture.remote_checkout();
    let other = fixture.root.path().join("parallel-clone");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), remote.as_os_str(), other.as_os_str()],
    );
    let registered = fixture.path_json(&["repo", "register"], &checkout);
    let second = fixture.path_json(&["repo", "register"], &other);
    let first_id = registered["context"]["repository_id"].as_str().unwrap();
    let second_id = second["context"]["repository_id"].as_str().unwrap();
    let mut paused = fixture.paused_refresh(first_id, "intent");
    let mut waiting = fixture.command();
    waiting.args(["repo", "refresh", "--repo", first_id]);
    let waiting = waiting.spawn().unwrap();
    let independent = fixture.json(&["repo", "refresh", "--repo", second_id]);
    assert_that!(independent["outcome"].as_str(), eq(Some("completed")));
    paused.kill().unwrap();
    let _ = paused.wait_with_output().unwrap();
    let waiting = wait(waiting);
    assert_that!(waiting.status.code(), eq(Some(3)));
    let result: Value = serde_json::from_slice(&waiting.stdout).unwrap();
    assert_that!(
        result["reason_code"].as_str(),
        eq(Some("operation_pending"))
    );
    let inspected = fixture.json(&["repo", "inspect", first_id]);
    assert_that!(
        inspected["data"]["operations"].as_array().unwrap().len(),
        eq(1)
    );
    assert_that!(
        &inspected["data"]["operations"][0]["operation_id"],
        eq(&result["context"]["operation_id"])
    );
}

#[googletest::test]
fn death_during_real_git_transport_leaves_pending_and_children_do_not_inherit_locks() {
    struct ReleaseOnDrop(PathBuf);
    impl Drop for ReleaseOnDrop {
        fn drop(&mut self) {
            let _ = fs::write(&self.0, b"release");
        }
    }
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let (source, remote, checkout) = fixture.remote_checkout();
    fixture.advance_main(&source, &remote);
    let registered = fixture.path_json(&["repo", "register"], &checkout);
    let id = registered["context"]["repository_id"].as_str().unwrap();
    let ready = fixture.root.path().join("transport-ready");
    let release = fixture.root.path().join("transport-release");
    let done = fixture.root.path().join("transport-done");
    let cleanup = ReleaseOnDrop(release.clone());
    let script = fixture.root.path().join("upload-pack-barrier");
    fs::write(&script, format!("#!/bin/sh\nprintf ready > '{}'\nwhile [ ! -f '{}' ]; do sleep 0.01; done\nprintf done > '{}'\nexec git upload-pack \"$@\"\n", ready.display(), release.display(), done.display())).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    fixture.git(
        &checkout,
        &[
            "config".as_ref(),
            "remote.origin.uploadpack".as_ref(),
            script.as_os_str(),
        ],
    );
    let mut command = fixture.command();
    command.args(["repo", "refresh", "--repo", id]);
    let mut refreshing = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready.exists() {
        if Instant::now() >= deadline {
            let _ = refreshing.kill();
            panic!("real Git transport did not reach its barrier");
        }
        thread::sleep(Duration::from_millis(5));
    }
    refreshing.kill().unwrap();
    // The transport is still paused. A second command must acquire released pool locks now.
    let repeated = fixture.json(&["repo", "refresh", "--repo", id]);
    assert_that!(repeated["outcome"].as_str(), eq(Some("pending")));
    assert_that!(
        repeated["data"]["operation"]["last_checkpoint"].as_str(),
        eq(Some("intent_recorded"))
    );
    assert_that!(
        fixture.json(&["repo", "inspect", id])["data"]["operations"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
    drop(cleanup);
    while !done.exists() {
        assert_that!(Instant::now() < deadline, eq(true));
        thread::sleep(Duration::from_millis(5));
    }
    let _ = refreshing.wait_with_output().unwrap();
}

#[googletest::test]
fn refresh_missing_main_transport_and_ref_write_failures_preserve_existing_state() {
    for fault in ["missing-main", "transport", "ref-write"] {
        let fixture = Fixture::new();
        fixture.json(&["catalog", "init"]);
        let (source, remote, checkout) = fixture.remote_checkout();
        let old_ref = fixture.git_stdout(&checkout, &["rev-parse", "refs/remotes/origin/main"]);
        fixture.advance_main(&source, &remote);
        let lock = checkout.join(".git/refs/remotes/origin/main.lock");
        match fault {
            "missing-main" => fixture.git(
                &remote,
                &[
                    "update-ref".as_ref(),
                    "-d".as_ref(),
                    "refs/heads/main".as_ref(),
                ],
            ),
            "transport" => fixture.git(
                &checkout,
                &[
                    "remote".as_ref(),
                    "set-url".as_ref(),
                    "origin".as_ref(),
                    fixture.root.path().join("missing-remote").as_os_str(),
                ],
            ),
            _ => {
                fs::create_dir_all(lock.parent().unwrap()).unwrap();
                fs::write(&lock, b"external lock sentinel").unwrap();
            }
        }
        let registered = fixture.path_json(&["repo", "register"], &checkout);
        let id = registered["context"]["repository_id"].as_str().unwrap();
        let result = fixture.json(&["repo", "refresh", "--repo", id]);
        assert_that!(result["outcome"].as_str(), eq(Some("pending")));
        assert_that!(result["reason_code"].as_str(), eq(Some("refresh_failed")));
        assert_that!(
            fixture
                .git_stdout(&checkout, &["rev-parse", "refs/remotes/origin/main"])
                .as_slice(),
            eq(old_ref.as_slice())
        );
        assert_that!(checkout.join("next").exists(), eq(false));
        if fault == "ref-write" {
            assert_that!(
                fs::read(lock).unwrap().as_slice(),
                eq(b"external lock sentinel".as_slice())
            );
        }
    }
}

#[googletest::test]
fn lost_refresh_stdout_preserves_the_completed_operation_for_inspection() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let (_, _, checkout) = fixture.remote_checkout();
    let registered = fixture.path_json(&["repo", "register"], &checkout);
    let id = registered["context"]["repository_id"].as_str().unwrap();
    let mut command = fixture.command();
    command.args(["repo", "refresh", "--repo", id]).stdout(
        fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap(),
    );
    assert_that!(output(command).status.code(), eq(Some(4)));
    let inspected = fixture.json(&["repo", "inspect", id]);
    assert_that!(
        inspected["data"]["operations"].as_array().unwrap().len(),
        eq(1)
    );
    assert_that!(
        inspected["data"]["operations"][0]["state"].as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        inspected["data"]["operations"][0]["resolved_commit"].is_string(),
        eq(true)
    );
}

#[googletest::test]
fn replaced_common_directory_cannot_refresh_a_different_repository() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let (source, remote, checkout) = fixture.remote_checkout();
    let other = fixture.root.path().join("replacement-target");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), remote.as_os_str(), other.as_os_str()],
    );
    let stale = fixture.git_stdout(&other, &["rev-parse", "refs/remotes/origin/main"]);
    let registered = fixture.path_json(&["repo", "register"], &checkout);
    let id = registered["context"]["repository_id"].as_str().unwrap();
    fixture.advance_main(&source, &remote);
    fs::rename(checkout.join(".git"), checkout.join(".git.saved")).unwrap();
    std::os::unix::fs::symlink(other.join(".git"), checkout.join(".git")).unwrap();
    let result = fixture.json(&["repo", "refresh", "--repo", id]);
    assert_that!(result["outcome"].as_str(), eq(Some("pending")));
    assert_that!(
        fixture
            .git_stdout(&other, &["rev-parse", "refs/remotes/origin/main"])
            .as_slice(),
        eq(stale.as_slice())
    );
    assert_that!(checkout.join(".git.saved").is_dir(), eq(true));
}

#[googletest::test]
fn concurrent_registration_cannot_overflow_the_last_durable_capacity_place() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let repository = fixture.repository();
    let registered = fixture.path_json(&["repo", "register"], &repository);
    let id = registered["context"]["repository_id"].as_str().unwrap();
    let mut paths = Vec::new();
    for number in 0..5 {
        let path = fixture.root.path().join(format!("contender-{number}"));
        fixture.git(
            &repository,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "--detach".as_ref(),
                path.as_os_str(),
                "HEAD".as_ref(),
            ],
        );
        if number < 3 {
            assert_that!(
                fixture.path_json(&["worktree", "register", "--repo", id], &path)["outcome"]
                    .as_str(),
                eq(Some("completed"))
            );
        }
        paths.push(path);
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            fixture
                .root
                .path()
                .join(format!("state/worktree-pool/repository-{id}.lock")),
        )
        .unwrap();
    lock.lock().unwrap();
    let mut processes = Vec::new();
    for path in &paths[3..] {
        let mut command = fixture.command();
        command
            .args(["worktree", "register", "--repo", id])
            .arg(path);
        processes.push(command.spawn().unwrap());
    }
    lock.unlock().unwrap();
    let mut statuses: Vec<_> = processes
        .into_iter()
        .map(|child| wait(child).status.code().unwrap())
        .collect();
    statuses.sort_unstable();
    assert_that!(statuses.as_slice(), eq([0, 2].as_slice()));
    assert_that!(
        fixture.json(&["repo", "inspect", id])["data"]["registered_count"].as_u64(),
        eq(Some(4))
    );
    for path in paths {
        assert_that!(
            fs::read(path.join("tracked")).unwrap().as_slice(),
            eq(b"protected checkout\n".as_slice())
        );
    }
}
