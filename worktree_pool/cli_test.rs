use std::{
    env,
    ffi::OsString,
    fs,
    io::Read,
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
        // Installation acceptance supplies the real Cargo-installed artifact.
        // Ordinary Bazel tests retain their existing binary and real adapters.
        let binary = env::var_os("POOL_INSTALLED_BINARY").map_or_else(
            || {
                PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
                    .join(env::var_os("TEST_WORKSPACE").unwrap())
                    .join(env!("POOL_BINARY"))
            },
            PathBuf::from,
        );
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
    // Drain both real pipes during execution; large histories must not fill a pipe
    // while the watchdog waits for exit.
    let stdout = child.stdout.take();
    let stdout = thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = stdout {
            pipe.read_to_end(&mut bytes).unwrap();
        }
        bytes
    });
    let stderr = child.stderr.take();
    let stderr = thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = stderr {
            pipe.read_to_end(&mut bytes).unwrap();
        }
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break (status, false);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break (child.wait().unwrap(), true);
        }
        thread::sleep(Duration::from_millis(5));
    };
    let result = Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    };
    assert!(!timed_out, "CLI exceeded bounded watchdog: {result:?}");
    result
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
            eq(Some(0))
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

#[googletest::test]
fn acquire_publishes_exclusive_detached_assignment_at_fresh_main() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let source = fixture.repository();
    let checkout = fixture.root.path().join("acquire-checkout");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), source.as_os_str(), checkout.as_os_str()],
    );
    let registered = fixture.path_json(&["repo", "register"], &checkout);
    let id = registered["context"]["repository_id"].as_str().unwrap();
    fixture.path_json(&["worktree", "register", "--repo", id], &checkout);
    fixture.json(&["pool", "configure", "--repo", id, "--max-worktrees", "1"]);
    fs::write(source.join("new-main"), b"fresh bytes").unwrap();
    fixture.git(&source, &["add".as_ref(), ".".as_ref()]);
    fixture.git(
        &source,
        &["commit".as_ref(), "-m".as_ref(), "fresh".as_ref()],
    );
    let acquired = fixture.json(&["acquire", "--repo", id]);
    assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        acquired["data"]["assignment"]["assignment_handle"].is_string(),
        eq(true)
    );
    assert_that!(
        fs::read(checkout.join("new-main")).unwrap().as_slice(),
        eq(b"fresh bytes".as_slice())
    );
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("active"))
    );
    assert_that!(
        fixture.json(&["acquire", "--repo", id])["outcome"].as_str(),
        eq(Some("rejected"))
    );
    assert_that!(
        fixture.json(&["assignment", "list"])["data"]["assignments"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
}

impl Fixture {
    fn acquisition(&self) -> (PathBuf, PathBuf, String) {
        self.json(&["catalog", "init"]);
        let source = self.repository();
        let checkout = self.root.path().join("checkout");
        self.git(
            self.root.path(),
            &["clone".as_ref(), source.as_os_str(), checkout.as_os_str()],
        );
        let registered = self.path_json(&["repo", "register"], &checkout);
        let id = registered["context"]["repository_id"]
            .as_str()
            .unwrap()
            .to_owned();
        self.path_json(&["worktree", "register", "--repo", &id], &checkout);
        (source, checkout, id)
    }
    fn git_text(&self, path: &std::path::Path, args: &[&str]) -> String {
        let result = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .env_clear()
            .env("PATH", env::var_os("PATH").unwrap())
            .env("HOME", self.root.path())
            .output()
            .unwrap();
        assert_that!(result.status.success(), eq(true));
        String::from_utf8(result.stdout)
            .unwrap()
            .trim_end_matches('\n')
            .to_owned()
    }
}
#[googletest::test]
fn acquisition_preserves_old_detached_tip_with_a_durable_recorded_reference() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    fixture.git(&checkout, &["checkout".as_ref(), "--detach".as_ref()]);
    fixture.git(
        &checkout,
        &[
            "commit".as_ref(),
            "--allow-empty".as_ref(),
            "-m".as_ref(),
            "private-detached-work".as_ref(),
        ],
    );
    let old = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let result = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(result["outcome"].as_str(), eq(Some("completed")));
    let operation = result["context"]["operation_id"].as_str().unwrap();
    let reference = format!("refs/worktree-pool/{operation}");
    assert_that!(
        fixture
            .git_text(&checkout, &["show-ref", "--verify", "--hash", &reference])
            .as_str(),
        eq(old.as_str())
    );
    let inspected = fixture.json(&["operation", "inspect", operation]);
    assert_that!(
        inspected["data"]["operation"]["preservation_reference"].as_str(),
        eq(Some(reference.as_str()))
    );
    assert_that!(
        fixture
            .git_text(&checkout, &["branch", "--show-current"])
            .as_str(),
        eq("")
    );
}

#[googletest::test]
fn acquisition_withholds_ignored_empty_directory_collision_and_tries_safe_registration() {
    let fixture = Fixture::new();
    let (source, checkout, id) = fixture.acquisition();
    fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "2"]);
    fs::write(checkout.join(".git/info/exclude"), b"retained\n").unwrap();
    fs::create_dir(checkout.join("retained")).unwrap();
    let fallback = fixture.root.path().join("fallback");
    fixture.git(
        &checkout,
        &[
            "worktree".as_ref(),
            "add".as_ref(),
            "--detach".as_ref(),
            fallback.as_os_str(),
            "HEAD".as_ref(),
        ],
    );
    let fallback_registered =
        fixture.path_json(&["worktree", "register", "--repo", &id], &fallback);
    let expected_id = fallback_registered["context"]["worktree_id"]
        .as_str()
        .unwrap();
    // Keep fallback dirty initially so the first attempt must inspect and withhold the collision.
    fs::write(fallback.join("tracked"), b"protected unfinished fallback").unwrap();
    fs::write(source.join("retained"), b"target tracked file").unwrap();
    fixture.git(&source, &["add".as_ref(), ".".as_ref()]);
    fixture.git(
        &source,
        &[
            "commit".as_ref(),
            "-m".as_ref(),
            "target collision".as_ref(),
        ],
    );
    let before = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let rejected = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(rejected["outcome"].as_str(), eq(Some("rejected")));
    assert_that!(checkout.join("retained").is_dir(), eq(true));
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
        eq(before.as_str())
    );
    let worktrees = fixture.json(&["worktree", "list", "--repo", &id]);
    let collision = worktrees["data"]["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["worktree_id"].as_str() != Some(expected_id))
        .unwrap();
    assert_that!(collision["availability"].as_str(), eq(Some("withheld")));
    assert_that!(
        collision["withheld_reason"].as_str(),
        eq(Some("retained_file_collision"))
    );
}

#[googletest::test]
fn retained_collisions_report_their_cause_and_fall_back_without_touching_files() {
    use std::os::unix::fs::symlink;
    for kind in ["file", "directory", "symlink", "prefix"] {
        let fixture = Fixture::new();
        let (source, checkout, id) = fixture.acquisition();
        fs::write(checkout.join(".git/info/exclude"), b"retained\n").unwrap();
        let sentinel = fixture.root.path().join("external");
        fs::write(&sentinel, b"external protected bytes").unwrap();
        match kind {
            "directory" => fs::create_dir(checkout.join("retained")).unwrap(),
            "symlink" => symlink(&sentinel, checkout.join("retained")).unwrap(),
            _ => fs::write(checkout.join("retained"), b"retained protected bytes").unwrap(),
        }
        if kind == "prefix" {
            fs::create_dir(source.join("retained")).unwrap();
            fs::write(source.join("retained/child"), b"target").unwrap();
        } else {
            fs::write(source.join("retained"), b"target").unwrap();
        }
        fixture.git(&source, &["add".as_ref(), ".".as_ref()]);
        fixture.git(
            &source,
            &["commit".as_ref(), "-m".as_ref(), "target".as_ref()],
        );
        let safe = fixture.root.path().join("safe");
        fixture.git(
            &checkout,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "--detach".as_ref(),
                safe.as_os_str(),
                "HEAD".as_ref(),
            ],
        );
        let registered = fixture.path_json(&["worktree", "register", "--repo", &id], &safe);
        let before = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let acquired = fixture.json(&["acquire", "--repo", &id]);
        assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            &acquired["context"]["worktree_id"],
            eq(&registered["context"]["worktree_id"])
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
            eq(before.as_str())
        );
        assert_that!(
            fs::read(&sentinel).unwrap().as_slice(),
            eq(b"external protected bytes".as_slice())
        );
        match kind {
            "directory" => assert_that!(checkout.join("retained").is_dir(), eq(true)),
            "symlink" => assert_that!(
                fs::read_link(checkout.join("retained")).unwrap().as_path(),
                eq(sentinel.as_path())
            ),
            _ => assert_that!(
                fs::read(checkout.join("retained")).unwrap().as_slice(),
                eq(b"retained protected bytes".as_slice())
            ),
        }
        let inspected = fixture.path_json(&["worktree", "inspect"], &checkout);
        assert_that!(
            inspected["data"]["worktree"]["withheld_reason"].as_str(),
            eq(Some("retained_file_collision"))
        );
    }
}

#[googletest::test]
fn acquisition_withholds_hidden_operations_indexes_and_unfinished_work_without_modifying_them() {
    for kind in [
        "merge",
        "sequencer",
        "revert",
        "index-lock",
        "assume-unchanged",
        "skip-worktree",
        "sparse",
        "staged",
        "untracked",
    ] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "1"]);
        match kind {
            "merge" => {
                fixture.git(
                    &checkout,
                    &["checkout".as_ref(), "-b".as_ref(), "side".as_ref()],
                );
                fixture.git(
                    &checkout,
                    &[
                        "commit".as_ref(),
                        "--allow-empty".as_ref(),
                        "-m".as_ref(),
                        "side".as_ref(),
                    ],
                );
                fixture.git(&checkout, &["checkout".as_ref(), "main".as_ref()]);
                fixture.git(
                    &checkout,
                    &[
                        "commit".as_ref(),
                        "--allow-empty".as_ref(),
                        "-m".as_ref(),
                        "main".as_ref(),
                    ],
                );
                fixture.git(
                    &checkout,
                    &[
                        "merge".as_ref(),
                        "--no-ff".as_ref(),
                        "--no-commit".as_ref(),
                        "side".as_ref(),
                    ],
                );
                assert_that!(
                    fixture
                        .git_text(&checkout, &["status", "--porcelain"])
                        .as_str(),
                    eq("")
                );
            }
            "sequencer" => fs::create_dir(checkout.join(".git/sequencer")).unwrap(),
            "revert" => {
                fs::write(
                    checkout.join(".git/REVERT_HEAD"),
                    b"unknown operation state",
                )
                .unwrap();
            }
            "index-lock" => {
                fs::write(checkout.join(".git/index.lock"), b"protected lock").unwrap();
            }
            "assume-unchanged" | "skip-worktree" => {
                fixture.git(
                    &checkout,
                    &[
                        "update-index".as_ref(),
                        format!("--{kind}").as_ref(),
                        "tracked".as_ref(),
                    ],
                );
                fs::write(
                    checkout.join("tracked"),
                    b"hidden protected unfinished bytes",
                )
                .unwrap();
            }
            "sparse" => fixture.git(
                &checkout,
                &[
                    "config".as_ref(),
                    "core.sparseCheckout".as_ref(),
                    "true".as_ref(),
                ],
            ),
            "staged" => {
                fs::write(checkout.join("tracked"), b"staged unfinished bytes").unwrap();
                fixture.git(&checkout, &["add".as_ref(), "tracked".as_ref()]);
            }
            "untracked" => {
                fs::write(checkout.join("unfinished"), b"untracked protected").unwrap();
            }
            _ => unreachable!(),
        }
        let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let tracked = fs::read(checkout.join("tracked")).unwrap();
        let result = fixture.json(&["acquire", "--repo", &id]);
        assert_that!(result["outcome"].as_str(), eq(Some("rejected")));
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
            eq(head.as_str())
        );
        assert_that!(
            fs::read(checkout.join("tracked")).unwrap().as_slice(),
            eq(tracked.as_slice())
        );
        assert_that!(
            fixture.json(&["assignment", "list"])["data"]["assignments"]
                .as_array()
                .unwrap()
                .is_empty(),
            eq(true)
        );
        let view = fixture.path_json(&["worktree", "inspect"], &checkout);
        let expected = match kind {
            "merge" | "sequencer" | "revert" | "index-lock" => "git_operation_in_progress",
            "assume-unchanged" | "skip-worktree" | "sparse" => "unsupported_index_state",
            _ => "unfinished_work",
        };
        assert_that!(
            view["data"]["worktree"]["withheld_reason"].as_str(),
            eq(Some(expected))
        );
    }
}

#[googletest::test]
fn cached_git_observations_cannot_hide_protected_content_or_mode_changes() {
    use std::fs::{File, FileTimes};
    for kind in ["weak-stat", "fsmonitor", "file-mode"] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "1"]);
        let tracked = checkout.join("tracked");
        File::open(&tracked)
            .unwrap()
            .set_times(
                FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_000_000_000)),
            )
            .unwrap();
        fixture.git(&checkout, &["update-index".as_ref(), "--refresh".as_ref()]);
        let original = fs::metadata(&tracked).unwrap();
        let marker = fixture.root.path().join("monitor-called");
        let hook = fixture.root.path().join("monitor");
        match kind {
            "fsmonitor" => {
                fs::write(
                    &hook,
                    format!(
                        "#!/bin/sh\nprintf called >> '{}'\nprintf 'fixture-token\\000'\n",
                        marker.display()
                    ),
                )
                .unwrap();
                fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
                fixture.git(
                    &checkout,
                    &[
                        "config".as_ref(),
                        "core.fsmonitor".as_ref(),
                        hook.as_os_str(),
                    ],
                );
                fixture.git(
                    &checkout,
                    &[
                        "config".as_ref(),
                        "core.fsmonitorHookVersion".as_ref(),
                        "2".as_ref(),
                    ],
                );
                fixture.git(
                    &checkout,
                    &[
                        "update-index".as_ref(),
                        "--fsmonitor".as_ref(),
                        "--fsmonitor-valid".as_ref(),
                        "tracked".as_ref(),
                    ],
                );
                fs::write(
                    &tracked,
                    b"caller protected unfinished content larger than before\n",
                )
                .unwrap();
            }
            "file-mode" => {
                fixture.git(
                    &checkout,
                    &[
                        "config".as_ref(),
                        "core.fileMode".as_ref(),
                        "false".as_ref(),
                    ],
                );
                fs::set_permissions(&tracked, fs::Permissions::from_mode(0o755)).unwrap();
            }
            _ => {
                fixture.git(
                    &checkout,
                    &[
                        "config".as_ref(),
                        "core.trustctime".as_ref(),
                        "false".as_ref(),
                    ],
                );
                fixture.git(
                    &checkout,
                    &[
                        "config".as_ref(),
                        "core.checkStat".as_ref(),
                        "minimal".as_ref(),
                    ],
                );
                let replacement = fixture.root.path().join("replacement");
                fs::write(
                    &replacement,
                    vec![b'x'; usize::try_from(original.len()).unwrap()],
                )
                .unwrap();
                fs::set_permissions(&replacement, original.permissions()).unwrap();
                File::open(&replacement)
                    .unwrap()
                    .set_times(FileTimes::new().set_modified(original.modified().unwrap()))
                    .unwrap();
                fs::rename(replacement, &tracked).unwrap();
            }
        }
        assert_that!(
            fixture
                .git_text(&checkout, &["status", "--porcelain"])
                .as_str(),
            eq("")
        );
        if marker.exists() {
            fs::remove_file(&marker).unwrap();
        }
        let before = fs::read(&tracked).unwrap();
        let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let mode = fs::metadata(&tracked).unwrap().permissions().mode();
        let result = fixture.json(&["acquire", "--repo", &id]);
        assert_that!(result["outcome"].as_str(), eq(Some("rejected")));
        assert_that!(
            fs::read(&tracked).unwrap().as_slice(),
            eq(before.as_slice())
        );
        assert_that!(
            fs::metadata(&tracked).unwrap().permissions().mode(),
            eq(mode)
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
            eq(head.as_str())
        );
        assert_that!(marker.exists(), eq(false));
    }
}

impl Fixture {
    fn paused_acquire(&self, id: &str, checkpoint: &str, continuable: bool) -> Child {
        use std::io::{BufRead, BufReader};
        let binary = PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
            .join(env::var_os("TEST_WORKSPACE").unwrap())
            .join(env!("CRASH_BINARY"));
        let environment = self.command();
        let mut command = Command::new(binary);
        command.env_clear();
        for (name, value) in environment.get_envs() {
            if let Some(value) = value {
                command.env(name, value);
            }
        }
        let mut child = command
            .env("OPERATION", "acquire")
            .env("REPOSITORY_ID", id)
            .env("CHECKPOINT", checkpoint)
            .env("CONTINUABLE", if continuable { "1" } else { "0" })
            .stdin(Stdio::piped())
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
                .is_ok_and(|s| s != &format!("checkpoint:{checkpoint}\n"))
        {
            let _ = child.kill();
            panic!(
                "acquire checkpoint missing: {observed:?}; {:?}",
                child.wait_with_output().unwrap()
            );
        }
        reader.join().unwrap();
        child
    }
}
#[googletest::test]
fn process_death_keeps_acquisition_reservations_and_effect_checkpoints_inspectable() {
    for checkpoint in [
        "reservation",
        "preservation-intent",
        "preservation-effect",
        "preserved",
        "checkout-intent",
        "checkout-effect",
        "result",
    ] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "1"]);
        fixture.git(&checkout, &["checkout".as_ref(), "--detach".as_ref()]);
        fixture.git(
            &checkout,
            &[
                "commit".as_ref(),
                "--allow-empty".as_ref(),
                "-m".as_ref(),
                "protected detached tip".as_ref(),
            ],
        );
        let tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let mut child = fixture.paused_acquire(&id, checkpoint, false);
        let recorded = fixture.json(&["assignment", "list"]);
        let assignments = recorded["data"]["assignments"].as_array().unwrap();
        assert_that!(assignments.len(), eq(1));
        let handle = assignments[0]["assignment_handle"]
            .as_str()
            .unwrap()
            .to_owned();
        let op = assignments[0]["operation_id"].as_str().unwrap().to_owned();
        child.kill().unwrap();
        let _ = child.wait().unwrap();
        let inspected = fixture.json(&["assignment", "inspect", &handle]);
        assert_that!(
            inspected["data"]["assignment"]["state"].as_str(),
            eq(Some(if checkpoint == "result" {
                "active"
            } else {
                "preparing"
            }))
        );
        let operation = fixture.json(&["operation", "inspect", &op]);
        assert_that!(
            operation["data"]["operation"]["last_checkpoint"].is_string(),
            eq(true)
        );
        if checkpoint != "result" {
            let worktree = fixture.path_json(&["worktree", "inspect"], &checkout);
            assert_that!(
                worktree["data"]["worktree"]["pending_work"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|o| o["operation_id"].as_str() == Some(op.as_str())),
                eq(true)
            );
        }
        let retried = fixture.json(&["acquire", "--repo", &id]);
        assert_that!(
            retried["outcome"].as_str(),
            eq(Some(if checkpoint == "result" {
                "rejected"
            } else {
                "pending"
            }))
        );
        assert_that!(
            fixture.json(&["assignment", "list"])["data"]["assignments"]
                .as_array()
                .unwrap()
                .len(),
            eq(1)
        );
        if [
            "preservation-effect",
            "preserved",
            "checkout-intent",
            "checkout-effect",
            "result",
        ]
        .contains(&checkpoint)
        {
            assert_that!(
                fixture
                    .git_text(
                        &checkout,
                        &[
                            "show-ref",
                            "--verify",
                            "--hash",
                            &format!("refs/worktree-pool/{op}")
                        ]
                    )
                    .as_str(),
                eq(tip.as_str())
            );
        }
    }
}

#[googletest::test]
fn default_acquisition_holds_the_committed_refresh_result_when_tracking_ref_changes() {
    use std::io::Write;
    let fixture = Fixture::new();
    let (source, checkout, id) = fixture.acquisition();
    fs::write(source.join("fresh"), b"fresh commit bytes").unwrap();
    fixture.git(&source, &["add".as_ref(), ".".as_ref()]);
    fixture.git(
        &source,
        &["commit".as_ref(), "-m".as_ref(), "advance main".as_ref()],
    );
    let expected = fixture.git_text(&source, &["rev-parse", "HEAD"]);
    let mut child = fixture.paused_acquire(&id, "refreshed", true);
    let refreshed = fixture.json(&["repo", "inspect", &id]);
    assert_that!(
        refreshed["data"]["operations"][0]["resolved_commit"].as_str(),
        eq(Some(expected.as_str()))
    );
    fixture.git(
        &checkout,
        &[
            "update-ref".as_ref(),
            "refs/remotes/origin/main".as_ref(),
            "HEAD".as_ref(),
        ],
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    assert_that!(wait(child).status.success(), eq(true));
    let recorded = fixture.json(&["assignment", "list"]);
    let assignment = &recorded["data"]["assignments"][0];
    assert_that!(
        assignment["resolved_commit"].as_str(),
        eq(Some(expected.as_str()))
    );
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
        eq(expected.as_str())
    );
}

#[googletest::test]
fn competing_acquisitions_serialize_while_independent_repositories_prepare() {
    let fixture = Fixture::new();
    let (source, checkout, id) = fixture.acquisition();
    let other = fixture.root.path().join("independent");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), source.as_os_str(), other.as_os_str()],
    );
    let registered = fixture.path_json(&["repo", "register"], &other);
    let other_id = registered["context"]["repository_id"].as_str().unwrap();
    fixture.path_json(&["worktree", "register", "--repo", other_id], &other);
    let mut paused = fixture.paused_acquire(&id, "checkout-intent", false);
    let mut competing = fixture.command();
    competing.args(["acquire", "--repo", &id]);
    let mut competing = competing.spawn().unwrap();
    let independent = fixture.json(&["acquire", "--repo", other_id]);
    assert_that!(independent["outcome"].as_str(), eq(Some("completed")));
    assert_that!(competing.try_wait().unwrap().is_none(), eq(true));
    let before = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    paused.kill().unwrap();
    let _ = paused.wait().unwrap();
    let rejected: Value = serde_json::from_slice(&wait(competing).stdout).unwrap();
    assert_that!(rejected["outcome"].as_str(), eq(Some("pending")));
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
        eq(before.as_str())
    );
    assert_that!(
        fixture.json(&["assignment", "list", "--repo", &id])["data"]["assignments"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
}
#[googletest::test]
fn simultaneous_callers_receive_distinct_registered_worktrees_and_handles() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    let linked = fixture.root.path().join("linked");
    fixture.git(
        &checkout,
        &[
            "worktree".as_ref(),
            "add".as_ref(),
            "--detach".as_ref(),
            linked.as_os_str(),
            "HEAD".as_ref(),
        ],
    );
    fixture.path_json(&["worktree", "register", "--repo", &id], &linked);
    let barrier = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(
            fixture
                .root
                .path()
                .join(format!("state/worktree-pool/repository-{id}.lock")),
        )
        .unwrap();
    barrier.lock().unwrap();
    let mut first = fixture.command();
    first.args(["acquire", "--repo", &id]);
    let first = first.spawn().unwrap();
    let mut second = fixture.command();
    second.args(["acquire", "--repo", &id]);
    let second = second.spawn().unwrap();
    barrier.unlock().unwrap();
    let a: Value = serde_json::from_slice(&wait(first).stdout).unwrap();
    let b: Value = serde_json::from_slice(&wait(second).stdout).unwrap();
    assert_that!(a["outcome"].as_str(), eq(Some("completed")));
    assert_that!(b["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        a["context"]["worktree_id"] != b["context"]["worktree_id"],
        eq(true)
    );
    assert_that!(
        a["context"]["assignment_handle"] != b["context"]["assignment_handle"],
        eq(true)
    );
    assert_that!(
        &a["data"]["assignment"]["resolved_commit"],
        eq(&b["data"]["assignment"]["resolved_commit"])
    );
}
#[googletest::test]
fn explicit_refs_use_local_commits_without_refreshing_origin_main() {
    for reference in ["HEAD", "main", "local-tag", "origin/main"] {
        let fixture = Fixture::new();
        let (source, checkout, id) = fixture.acquisition();
        let old = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        fixture.git(&checkout, &["tag".as_ref(), "local-tag".as_ref()]);
        fixture.git(
            &source,
            &[
                "commit".as_ref(),
                "--allow-empty".as_ref(),
                "-m".as_ref(),
                "remote advanced".as_ref(),
            ],
        );
        let result = fixture.json(&["acquire", "--repo", &id, reference]);
        assert_that!(result["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            result["data"]["assignment"]["resolved_commit"].as_str(),
            eq(Some(old.as_str()))
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&old)
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "refs/remotes/origin/main"]),
            eq(&old)
        );
        assert_that!(
            fixture.json(&["catalog", "info"])["data"]["operations"]
                .as_array()
                .unwrap()
                .is_empty(),
            eq(true)
        );
    }
}
#[googletest::test]
fn explicit_refs_check_out_local_commits_without_a_usable_origin_or_main() {
    for fault in ["broken-origin", "no-origin", "no-main"] {
        let fixture = Fixture::new();
        let (source, checkout, id) = fixture.acquisition();
        fs::write(checkout.join("requested"), b"local task content\n").unwrap();
        fixture.git(&checkout, &["add".as_ref(), ".".as_ref()]);
        fixture.git(
            &checkout,
            &["commit".as_ref(), "-m".as_ref(), "local task".as_ref()],
        );
        let target = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        fixture.git(&checkout, &["tag".as_ref(), "local-task".as_ref()]);
        fixture.git(
            &checkout,
            &["checkout".as_ref(), "--detach".as_ref(), "HEAD^".as_ref()],
        );
        match fault {
            "broken-origin" => fixture.git(
                &checkout,
                &[
                    "remote".as_ref(),
                    "set-url".as_ref(),
                    "origin".as_ref(),
                    fixture.root.path().join("missing-remote").as_os_str(),
                ],
            ),
            "no-origin" => fixture.git(
                &checkout,
                &["remote".as_ref(), "remove".as_ref(), "origin".as_ref()],
            ),
            _ => {
                fixture.git(
                    &source,
                    &[
                        "update-ref".as_ref(),
                        "-d".as_ref(),
                        "refs/heads/main".as_ref(),
                    ],
                );
                fixture.git(
                    &checkout,
                    &[
                        "update-ref".as_ref(),
                        "-d".as_ref(),
                        "refs/remotes/origin/main".as_ref(),
                    ],
                );
            }
        }
        let result = fixture.json(&[
            "acquire",
            "--repo",
            &id,
            if fault == "no-origin" {
                &target
            } else {
                "local-task"
            },
        ]);
        assert_that!(result["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            result["data"]["assignment"]["resolved_commit"].as_str(),
            eq(Some(target.as_str()))
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&target)
        );
        assert_that!(
            fixture.git_text(&checkout, &["branch", "--show-current"]),
            eq("")
        );
        assert_that!(
            fs::read(checkout.join("requested")).unwrap().as_slice(),
            eq(b"local task content\n".as_slice())
        );
        assert_that!(
            fixture.json(&["catalog", "info"])["data"]["operations"]
                .as_array()
                .unwrap()
                .is_empty(),
            eq(true)
        );
    }
}
#[googletest::test]
fn explicit_refs_reject_replaced_common_directories_before_acquisition_effects() {
    let fixture = Fixture::new();
    let (source, checkout, id) = fixture.acquisition();
    let other = fixture.root.path().join("replacement-target");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), source.as_os_str(), other.as_os_str()],
    );
    let worktrees = fixture.git_text(&other, &["worktree", "list", "--porcelain"]);
    fs::rename(checkout.join(".git"), checkout.join(".git.saved")).unwrap();
    std::os::unix::fs::symlink(other.join(".git"), checkout.join(".git")).unwrap();
    let before = fixture.json(&["events", "list"]);
    let result = fixture.run(&["acquire", "--repo", &id, "HEAD"]);
    assert_that!(result.status.code(), eq(Some(2)));
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_that!(result["reason_code"].as_str(), eq(Some("git_failed")));
    assert_that!(fixture.json(&["events", "list"]), eq(&before));
    assert_that!(
        fixture.git_text(&other, &["worktree", "list", "--porcelain"]),
        eq(&worktrees)
    );
    assert_that!(checkout.join(".git.saved").is_dir(), eq(true));
}
#[googletest::test]
fn missing_explicit_ref_rejects_without_refresh_or_assignment() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    fixture.git(
        &checkout,
        &["remote".as_ref(), "remove".as_ref(), "origin".as_ref()],
    );
    let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let before = fixture.json(&["events", "list"]);
    let result = fixture.run(&["acquire", "--repo", &id, "unknown-ref"]);
    assert_that!(result.status.code(), eq(Some(2)));
    let result: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_that!(result["reason_code"].as_str(), eq(Some("git_failed")));
    assert_that!(fixture.json(&["events", "list"]), eq(&before));
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
        eq(&head)
    );
    assert_that!(
        fixture.json(&["assignment", "list"])["data"]["assignments"]
            .as_array()
            .unwrap()
            .is_empty(),
        eq(true)
    );
}
#[googletest::test]
fn explicit_refs_respect_pending_refreshes_without_changing_history_or_checkout() {
    for fault in ["interrupted", "failed"] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        if fault == "interrupted" {
            let mut child = fixture.paused_refresh(&id, "intent");
            child.kill().unwrap();
            child.wait().unwrap();
        } else {
            fixture.git(
                &checkout,
                &["remote".as_ref(), "remove".as_ref(), "origin".as_ref()],
            );
            assert_that!(
                fixture.json(&["repo", "refresh", "--repo", &id])["outcome"].as_str(),
                eq(Some("pending"))
            );
        }
        let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let before = fixture.json(&["events", "list"]);
        let result = fixture.run(&["acquire", "--repo", &id, "HEAD"]);
        assert_that!(result.status.code(), eq(Some(3)));
        let result: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_that!(
            result["reason_code"].as_str(),
            eq(Some("operation_pending"))
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&before));
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&head)
        );
        assert_that!(
            fixture.json(&["assignment", "list"])["data"]["assignments"]
                .as_array()
                .unwrap()
                .is_empty(),
            eq(true)
        );
    }
}
#[googletest::test]
fn default_acquisition_never_assigns_stale_main_after_fetch_failure() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    fixture.git(
        &checkout,
        &[
            "remote".as_ref(),
            "set-url".as_ref(),
            "origin".as_ref(),
            fixture.root.path().join("missing-remote").as_os_str(),
        ],
    );
    let result = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(result["outcome"].as_str(), eq(Some("pending")));
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
        eq(&head)
    );
    assert_that!(
        fixture.json(&["assignment", "list"])["data"]["assignments"]
            .as_array()
            .unwrap()
            .is_empty(),
        eq(true)
    );
}
#[googletest::test]
fn lost_acquisition_stdout_keeps_the_committed_assignment_and_never_reacquires_it() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "1"]);
    let mut command = fixture.command();
    command.args(["acquire", "--repo", &id]).stdout(
        fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap(),
    );
    assert_that!(output(command).status.code(), eq(Some(4)));
    let assignments = fixture.json(&["assignment", "list"]);
    let a = &assignments["data"]["assignments"][0];
    let handle = a["assignment_handle"].as_str().unwrap();
    let op = a["operation_id"].as_str().unwrap();
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("active"))
    );
    assert_that!(
        fixture.json(&["operation", "inspect", op])["data"]["operation"]["state"].as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        fixture.json(&["acquire", "--repo", &id])["outcome"].as_str(),
        eq(Some("rejected"))
    );
    assert_that!(
        fixture.json(&["assignment", "list"])["data"]["assignments"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
    assert_that!(
        fixture
            .git_text(&checkout, &["branch", "--show-current"])
            .as_str(),
        eq("")
    );
}
#[googletest::test]
fn changed_ignore_rules_retain_files_and_withhold_preparation_after_checkout() {
    let fixture = Fixture::new();
    let (source, checkout, id) = fixture.acquisition();
    fs::write(source.join(".gitignore"), b"cache/\n").unwrap();
    fixture.git(&source, &["add".as_ref(), ".".as_ref()]);
    fixture.git(
        &source,
        &["commit".as_ref(), "-m".as_ref(), "ignore cache".as_ref()],
    );
    fixture.git(&checkout, &["fetch".as_ref(), "origin".as_ref()]);
    fixture.git(
        &checkout,
        &[
            "checkout".as_ref(),
            "--detach".as_ref(),
            "origin/main".as_ref(),
        ],
    );
    fs::create_dir(checkout.join("cache")).unwrap();
    fs::write(checkout.join("cache/retained"), b"warm protected bytes").unwrap();
    fs::remove_file(source.join(".gitignore")).unwrap();
    fixture.git(&source, &["add".as_ref(), ".".as_ref()]);
    fixture.git(
        &source,
        &[
            "commit".as_ref(),
            "-m".as_ref(),
            "stop ignoring cache".as_ref(),
        ],
    );
    let result = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(result["outcome"].as_str(), eq(Some("pending")));
    assert_that!(
        fs::read(checkout.join("cache/retained"))
            .unwrap()
            .as_slice(),
        eq(b"warm protected bytes".as_slice())
    );
    let handle = result["context"]["assignment_handle"].as_str().unwrap();
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("preparing"))
    );
}

#[googletest::test]
fn acquisition_prefers_a_safe_matching_commit_over_the_lowest_worktree_id() {
    let fixture = Fixture::new();
    let (source, checkout, id) = fixture.acquisition();
    let mut registrations = vec![(
        fixture.path_json(&["worktree", "inspect"], &checkout),
        checkout.clone(),
    )];
    for name in ["candidate-one", "candidate-two"] {
        let path = fixture.root.path().join(name);
        fixture.git(
            &checkout,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "--detach".as_ref(),
                path.as_os_str(),
                "HEAD".as_ref(),
            ],
        );
        registrations.push((
            fixture.path_json(&["worktree", "register", "--repo", &id], &path),
            path,
        ));
    }
    registrations.sort_by_key(|(r, _)| {
        r["data"]["worktree"]["worktree_id"]
            .as_str()
            .unwrap()
            .to_owned()
    });
    fs::write(source.join("new-main"), b"target").unwrap();
    fixture.git(&source, &["add".as_ref(), ".".as_ref()]);
    fixture.git(
        &source,
        &["commit".as_ref(), "-m".as_ref(), "new target".as_ref()],
    );
    fixture.git(&checkout, &["fetch".as_ref(), "origin".as_ref()]);
    let (matching, path) = registrations.last().unwrap();
    fixture.git(
        path,
        &[
            "checkout".as_ref(),
            "--detach".as_ref(),
            "refs/remotes/origin/main".as_ref(),
        ],
    );
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        &acquired["context"]["worktree_id"],
        eq(&matching["data"]["worktree"]["worktree_id"])
    );
}

#[googletest::test]
fn clean_release_retains_ignored_files_and_reacquires_with_a_fresh_handle() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    fs::write(checkout.join(".git/info/exclude"), b"build-cache\n").unwrap();
    fs::write(checkout.join("build-cache"), b"retained build state").unwrap();
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let released = fixture.json(&["release", handle]);
    assert_that!(released["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        released["data"]["assignment"]["state"].as_str(),
        eq(Some("released"))
    );
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("released"))
    );
    let worktree = acquired["context"]["worktree_id"].as_str().unwrap();
    let inspected = fixture.json(&["worktree", "inspect", worktree]);
    assert_that!(
        inspected["data"]["worktree"]["ownership"].as_str(),
        eq(Some("unassigned"))
    );
    assert_that!(
        inspected["data"]["worktree"]["last_release_position"].is_u64(),
        eq(true)
    );
    let next = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(next["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        next["context"]["assignment_handle"] != acquired["context"]["assignment_handle"],
        eq(true)
    );
    assert_that!(
        fs::read(checkout.join("build-cache")).unwrap().as_slice(),
        eq(b"retained build state".as_slice())
    );
}

impl Fixture {
    fn paused_release(&self, handle: &str, checkpoint: &str, continuable: bool) -> Child {
        use std::io::{BufRead, BufReader};
        let binary = PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
            .join(env::var_os("TEST_WORKSPACE").unwrap())
            .join(env!("CRASH_BINARY"));
        let environment = self.command();
        let mut command = Command::new(binary);
        command.env_clear();
        for (name, value) in environment.get_envs() {
            if let Some(value) = value {
                command.env(name, value);
            }
        }
        let mut child = command
            .env("OPERATION", "release")
            .env("ASSIGNMENT_HANDLE", handle)
            .env("CHECKPOINT", checkpoint)
            .env("CONTINUABLE", if continuable { "1" } else { "0" })
            .stdin(Stdio::piped())
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
                .is_ok_and(|s| s != &format!("checkpoint:{checkpoint}\n"))
        {
            let _ = child.kill();
            panic!(
                "release checkpoint missing: {observed:?}; {:?}",
                child.wait_with_output().unwrap()
            );
        }
        reader.join().unwrap();
        child
    }
}
#[googletest::test]
fn release_process_death_keeps_ownership_until_the_durable_release_commit() {
    for checkpoint in ["intent", "preservation-effect", "preserved", "result"] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        let acquired = fixture.json(&["acquire", "--repo", &id]);
        let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
        let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let mut child = fixture.paused_release(handle, checkpoint, false);
        child.kill().unwrap();
        child.wait().unwrap();
        let assignment = fixture.json(&["assignment", "inspect", handle]);
        assert_that!(
            assignment["data"]["assignment"]["state"].as_str(),
            eq(Some(if checkpoint == "result" {
                "released"
            } else {
                "active"
            }))
        );
        let operations = fixture.json(&["operation", "list", "--repo", &id]);
        let operation = operations["data"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| {
                o["last_checkpoint"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("release_") || s == "preservation_committed")
            })
            .unwrap();
        let operation_id = operation["operation_id"].as_str().unwrap();
        let before = fixture.json(&["events", "list"]);
        let repeated = fixture.json(&["release", handle]);
        assert_that!(
            repeated["context"]["operation_id"].as_str(),
            eq(Some(operation_id))
        );
        assert_that!(
            repeated["outcome"].as_str(),
            eq(Some(if checkpoint == "result" {
                "completed"
            } else {
                "pending"
            }))
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&before));
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
            eq(head.as_str())
        );
        if checkpoint != "intent" {
            let reference = operation["preservation_reference"].as_str().unwrap();
            assert_that!(
                fixture
                    .git_text(&checkout, &["show-ref", "--verify", "--hash", reference])
                    .as_str(),
                eq(head.as_str())
            );
        }
        if checkpoint != "result" {
            assert_that!(
                fixture.json(&["acquire", "--repo", &id])["outcome"].as_str(),
                eq(Some("pending"))
            );
        }
    }
}

#[googletest::test]
fn release_rechecks_the_preservation_reference_before_ending_ownership() {
    use std::io::Write;
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    fixture.git(
        &checkout,
        &[
            "commit".as_ref(),
            "--allow-empty".as_ref(),
            "-m".as_ref(),
            "private tip".as_ref(),
        ],
    );
    let mut child = fixture.paused_release(handle, "preserved", true);
    let operations = fixture.json(&["operation", "list"]);
    let operation = operations["data"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["last_checkpoint"] == "preservation_committed")
        .unwrap();
    let reference = operation["preservation_reference"].as_str().unwrap();
    fixture.git(
        &checkout,
        &["update-ref".as_ref(), reference.as_ref(), "HEAD^".as_ref()],
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    assert_that!(wait(child).status.success(), eq(true));
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("active"))
    );
    assert_that!(
        fixture.json(&["release", handle])["outcome"].as_str(),
        eq(Some("pending"))
    );
    assert_that!(
        fixture
            .git_text(&checkout, &["show-ref", "--verify", "--hash", reference])
            .as_str(),
        eq(fixture
            .git_text(&checkout, &["rev-parse", "HEAD^"])
            .as_str())
    );
}

#[googletest::test]
fn unsafe_release_rejects_without_changing_ownership_files_index_or_history() {
    for state in [
        "tracked",
        "staged",
        "untracked",
        "assume-unchanged",
        "skip-worktree",
        "sparse",
        "index.lock",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-apply",
        "rebase-merge",
        "sequencer",
        "BISECT_LOG",
        "hidden-mode",
        "broken-index",
        "missing-checkout",
    ] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        let acquired = fixture.json(&["acquire", "--repo", &id]);
        let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
        let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let mut actual = checkout.clone();
        match state {
            "tracked" => fs::write(checkout.join("tracked"), b"unfinished tracked work").unwrap(),
            "staged" => {
                fs::write(checkout.join("tracked"), b"unfinished indexed work").unwrap();
                fixture.git(&checkout, &["add".as_ref(), "tracked".as_ref()]);
            }
            "untracked" => {
                fs::write(checkout.join("private-notes"), b"nonignored private work").unwrap();
            }
            "assume-unchanged" | "skip-worktree" => {
                fixture.git(
                    &checkout,
                    &[
                        "update-index".as_ref(),
                        format!("--{state}").as_ref(),
                        "tracked".as_ref(),
                    ],
                );
                fs::write(checkout.join("tracked"), b"hidden unfinished work").unwrap();
            }
            "sparse" => fixture.git(
                &checkout,
                &[
                    "config".as_ref(),
                    "core.sparseCheckout".as_ref(),
                    "true".as_ref(),
                ],
            ),
            "hidden-mode" => {
                fixture.git(
                    &checkout,
                    &[
                        "config".as_ref(),
                        "core.filemode".as_ref(),
                        "false".as_ref(),
                    ],
                );
                fs::set_permissions(checkout.join("tracked"), fs::Permissions::from_mode(0o755))
                    .unwrap();
            }
            "broken-index" => {
                fs::write(checkout.join(".git/index"), b"uncertain index bytes").unwrap();
            }
            "missing-checkout" => {
                actual = fixture.root.path().join("moved-checkout");
                fs::rename(&checkout, &actual).unwrap();
            }
            "rebase-apply" | "rebase-merge" | "sequencer" => {
                fs::create_dir(checkout.join(".git").join(state)).unwrap();
            }
            marker => fs::write(
                checkout.join(".git").join(marker),
                b"unfinished operation artifact\n",
            )
            .unwrap(),
        }
        let bytes = fs::read(actual.join("tracked")).unwrap();
        let mode = fs::metadata(actual.join("tracked"))
            .unwrap()
            .permissions()
            .mode();
        let index = fs::read(actual.join(".git/index")).unwrap();
        let history = fixture.json(&["events", "list"]);
        let released = fixture.run(&["release", handle]);
        assert_that!(released.status.code(), eq(Some(2)));
        assert_that!(
            fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"]
                .as_str(),
            eq(Some("active"))
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&history));
        assert_that!(fs::read(actual.join("tracked")).unwrap(), eq(&bytes));
        assert_that!(
            fs::metadata(actual.join("tracked"))
                .unwrap()
                .permissions()
                .mode(),
            eq(mode)
        );
        assert_that!(fs::read(actual.join(".git/index")).unwrap(), eq(&index));
        if !matches!(state, "broken-index") {
            assert_that!(
                fixture.git_text(&actual, &["rev-parse", "HEAD"]).as_str(),
                eq(head.as_str())
            );
        }
        if state == "untracked" {
            assert_that!(
                fs::read(actual.join("private-notes")).unwrap().as_slice(),
                eq(b"nonignored private work".as_slice())
            );
        }
        if matches!(
            state,
            "index.lock"
                | "MERGE_HEAD"
                | "CHERRY_PICK_HEAD"
                | "REVERT_HEAD"
                | "rebase-apply"
                | "rebase-merge"
                | "sequencer"
                | "BISECT_LOG"
        ) {
            assert_that!(actual.join(".git").join(state).exists(), eq(true));
        }
    }
}

#[googletest::test]
fn released_and_unknown_handles_cannot_release_a_newer_assignment_or_claim_availability() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    let first = fixture.json(&["acquire", "--repo", &id]);
    let old = first["context"]["assignment_handle"].as_str().unwrap();
    let completed = fixture.json(&["release", old]);
    assert_that!(completed["outcome"].as_str(), eq(Some("completed")));
    let second = fixture.json(&["acquire", "--repo", &id]);
    let current = second["context"]["assignment_handle"].as_str().unwrap();
    let history = fixture.json(&["events", "list"]);
    let repeated = fixture.json(&["release", old]);
    assert_that!(
        repeated["reason_code"].as_str(),
        eq(Some("already_released"))
    );
    assert_that!(
        repeated["context"]["operation_id"],
        eq(&completed["context"]["operation_id"])
    );
    assert_that!(
        repeated["data"]["assignment"]["state"].as_str(),
        eq(Some("released"))
    );
    assert_that!(repeated["data"]["current_availability"].is_null(), eq(true));
    for unknown in [
        "00000000-0000-4000-8000-000000000099",
        checkout.to_str().unwrap(),
    ] {
        let result = fixture.json(&["release", unknown]);
        assert_that!(result["outcome"].as_str(), eq(Some("rejected")));
        assert_that!(
            result["reason_code"].as_str(),
            eq(Some("assignment_unknown"))
        );
    }
    assert_that!(
        fixture.json(&["assignment", "inspect", current])["data"]["assignment"]["state"].as_str(),
        eq(Some("active"))
    );
    assert_that!(
        fixture.json(&[
            "worktree",
            "inspect",
            second["context"]["worktree_id"].as_str().unwrap()
        ])["data"]["worktree"]["assignment_handle"]
            .as_str(),
        eq(Some(current))
    );
    assert_that!(fixture.json(&["events", "list"]), eq(&history));
}

#[googletest::test]
fn clean_branch_release_keeps_the_branch_and_does_not_veto_a_live_caller_process() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    fixture.git(
        &checkout,
        &["checkout".as_ref(), "-b".as_ref(), "caller-work".as_ref()],
    );
    fixture.git(
        &checkout,
        &[
            "commit".as_ref(),
            "--allow-empty".as_ref(),
            "-m".as_ref(),
            "committed caller work".as_ref(),
        ],
    );
    let tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let mut caller = Command::new("sleep")
        .arg("60")
        .current_dir(&checkout)
        .spawn()
        .unwrap();
    let released = fixture.json(&["release", handle]);
    let still_live = caller.try_wait().unwrap().is_none();
    caller.kill().unwrap();
    caller.wait().unwrap();
    assert_that!(still_live, eq(true));
    assert_that!(released["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        released["data"]["operation"]["branch"].as_str(),
        eq(Some("refs/heads/caller-work"))
    );
    assert_that!(
        released["data"]["operation"]["preservation_reference"].is_null(),
        eq(true)
    );
    assert_that!(
        fixture
            .git_text(&checkout, &["symbolic-ref", "HEAD"])
            .as_str(),
        eq("refs/heads/caller-work")
    );
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
        eq(tip.as_str())
    );
    assert_that!(
        fixture
            .git_text(&checkout, &["rev-parse", "refs/heads/caller-work"])
            .as_str(),
        eq(tip.as_str())
    );
}

#[googletest::test]
fn preservation_write_failure_retains_ownership_and_never_blindly_retries() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    fixture.git(
        &checkout,
        &[
            "commit".as_ref(),
            "--allow-empty".as_ref(),
            "-m".as_ref(),
            "private detached tip".as_ref(),
        ],
    );
    let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let obstruction = checkout.join(".git/refs/worktree-pool");
    fs::write(&obstruction, b"protected reference namespace obstruction").unwrap();
    let release = fixture.json(&["release", handle]);
    assert_that!(release["outcome"].as_str(), eq(Some("pending")));
    assert_that!(
        release["data"]["operation"]["last_checkpoint"].as_str(),
        eq(Some("preservation_failed"))
    );
    assert_that!(
        release["data"]["assignment"]["state"].as_str(),
        eq(Some("active"))
    );
    assert_that!(
        fs::read(&obstruction).unwrap().as_slice(),
        eq(b"protected reference namespace obstruction".as_slice())
    );
    let history = fixture.json(&["events", "list"]);
    fs::rename(
        &obstruction,
        fixture.root.path().join("retained-obstruction"),
    )
    .unwrap();
    let repeat = fixture.json(&["release", handle]);
    assert_that!(repeat["outcome"].as_str(), eq(Some("pending")));
    assert_that!(
        repeat["context"]["operation_id"],
        eq(&release["context"]["operation_id"])
    );
    assert_that!(obstruction.exists(), eq(false));
    assert_that!(fixture.json(&["events", "list"]), eq(&history));
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
        eq(head.as_str())
    );
}

#[googletest::test]
fn public_reacquisition_uses_release_recency_and_withholds_retained_checkout_collisions() {
    let fixture = Fixture::new();
    let (source, checkout, id) = fixture.acquisition();
    fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "2"]);
    let linked = fixture.root.path().join("second-checkout");
    fixture.git(
        &checkout,
        &[
            "worktree".as_ref(),
            "add".as_ref(),
            "--detach".as_ref(),
            linked.as_os_str(),
            "HEAD".as_ref(),
        ],
    );
    fixture.path_json(&["worktree", "register", "--repo", &id], &linked);
    fs::write(checkout.join(".git/info/exclude"), b"retained\n").unwrap();
    for path in [&checkout, &linked] {
        fs::create_dir(path.join("retained")).unwrap();
        fs::write(path.join("retained/cache"), b"persistent build bytes").unwrap();
    }
    let first = fixture.json(&["acquire", "--repo", &id]);
    let second = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(
        first["context"]["worktree_id"] != second["context"]["worktree_id"],
        eq(true)
    );
    let first_handle = first["context"]["assignment_handle"].as_str().unwrap();
    let second_handle = second["context"]["assignment_handle"].as_str().unwrap();
    assert_that!(
        fixture.json(&["release", first_handle])["outcome"].as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        fixture.json(&["release", second_handle])["outcome"].as_str(),
        eq(Some("completed"))
    );
    let next = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(
        next["context"]["worktree_id"],
        eq(&second["context"]["worktree_id"])
    );
    assert_that!(
        fixture.json(&[
            "release",
            next["context"]["assignment_handle"].as_str().unwrap()
        ])["outcome"]
            .as_str(),
        eq(Some("completed"))
    );
    fs::write(
        source.join("retained"),
        b"tracked destination conflicts with retained directory",
    )
    .unwrap();
    fixture.git(&source, &["add".as_ref(), "retained".as_ref()]);
    fixture.git(
        &source,
        &[
            "commit".as_ref(),
            "-m".as_ref(),
            "collision target".as_ref(),
        ],
    );
    let collided = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(collided["outcome"].as_str(), eq(Some("rejected")));
    let records = fixture.json(&["worktree", "list", "--repo", &id]);
    for record in records["data"]["worktrees"].as_array().unwrap() {
        assert_that!(record["ownership"].as_str(), eq(Some("unassigned")));
        assert_that!(
            record["withheld_reason"].as_str(),
            eq(Some("retained_file_collision"))
        );
    }
    for path in [&checkout, &linked] {
        assert_that!(
            fs::read(path.join("retained/cache")).unwrap().as_slice(),
            eq(b"persistent build bytes".as_slice())
        );
    }
}

#[googletest::test]
fn lost_release_stdout_preserves_the_committed_release_and_known_handle_result() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    fixture.git(
        &checkout,
        &[
            "commit".as_ref(),
            "--allow-empty".as_ref(),
            "-m".as_ref(),
            "detached result".as_ref(),
        ],
    );
    let tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let mut command = fixture.command();
    command.args(["release", handle]).stdout(
        fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap(),
    );
    let result = output(command);
    assert_that!(result.status.code(), eq(Some(4)));
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("released"))
    );
    let history = fixture.json(&["events", "list"]);
    let repeat = fixture.json(&["release", handle]);
    assert_that!(repeat["reason_code"].as_str(), eq(Some("already_released")));
    assert_that!(
        repeat["data"]["operation"]["tip"].as_str(),
        eq(Some(tip.as_str()))
    );
    let reference = repeat["data"]["operation"]["preservation_reference"]
        .as_str()
        .unwrap();
    assert_that!(
        fixture
            .git_text(&checkout, &["show-ref", "--verify", "--hash", reference])
            .as_str(),
        eq(tip.as_str())
    );
    assert_that!(fixture.json(&["events", "list"]), eq(&history));
}

#[googletest::test]
fn competing_release_and_acquire_serialize_while_an_independent_repository_can_finish() {
    use std::io::Write;
    let fixture = Fixture::new();
    let (source, _, id) = fixture.acquisition();
    let other = fixture.root.path().join("independent-checkout");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), source.as_os_str(), other.as_os_str()],
    );
    let registered = fixture.path_json(&["repo", "register"], &other);
    let other_id = registered["context"]["repository_id"].as_str().unwrap();
    fixture.path_json(&["worktree", "register", "--repo", other_id], &other);
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let mut paused = fixture.paused_release(handle, "preserved", true);
    let mut competing = fixture.command();
    competing.args(["acquire", "--repo", &id]);
    let mut competing = competing.spawn().unwrap();
    let mut repeated = fixture.command();
    repeated.args(["release", handle]);
    let mut repeated = repeated.spawn().unwrap();
    let independent = fixture.json(&["acquire", "--repo", other_id]);
    let acquire_waiting = competing.try_wait().unwrap().is_none();
    let release_waiting = repeated.try_wait().unwrap().is_none();
    paused
        .stdin
        .take()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    let finished = wait(paused);
    let competitor = wait(competing);
    let repeat = wait(repeated);
    assert_that!(independent["outcome"].as_str(), eq(Some("completed")));
    assert_that!(acquire_waiting, eq(true));
    assert_that!(release_waiting, eq(true));
    assert_that!(finished.status.success(), eq(true));
    assert_that!(competitor.status.success(), eq(true));
    assert_that!(repeat.status.success(), eq(true));
    let next: Value = serde_json::from_slice(&competitor.stdout).unwrap();
    let repeat: Value = serde_json::from_slice(&repeat.stdout).unwrap();
    assert_that!(repeat["reason_code"].as_str(), eq(Some("already_released")));
    assert_that!(
        next["context"]["assignment_handle"] != acquired["context"]["assignment_handle"],
        eq(true)
    );
    assert_that!(
        next["context"]["worktree_id"],
        eq(&acquired["context"]["worktree_id"])
    );
    assert_that!(
        fixture.json(&[
            "assignment",
            "inspect",
            next["context"]["assignment_handle"].as_str().unwrap()
        ])["data"]["assignment"]["state"]
            .as_str(),
        eq(Some("active"))
    );
}

#[googletest::test]
fn release_revalidates_the_recorded_checkout_binding_before_ending_ownership() {
    use std::io::Write;
    let fixture = Fixture::new();
    let (source, checkout, id) = fixture.acquisition();
    let other = fixture.root.path().join("unregistered-substitute");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), source.as_os_str(), other.as_os_str()],
    );
    fixture.git(
        &other,
        &["checkout".as_ref(), "-b".as_ref(), "caller-work".as_ref()],
    );
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    fixture.git(
        &checkout,
        &["checkout".as_ref(), "-b".as_ref(), "caller-work".as_ref()],
    );
    let tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let mut child = fixture.paused_release(handle, "intent", true);
    let saved = fixture.root.path().join("saved-original-git-metadata");
    fs::rename(checkout.join(".git"), &saved).unwrap();
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", other.join(".git").display()),
    )
    .unwrap();
    let routing = fs::read(checkout.join(".git")).unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    assert_that!(wait(child).status.success(), eq(true));
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("active"))
    );
    assert_that!(
        fixture.json(&["release", handle])["outcome"].as_str(),
        eq(Some("pending"))
    );
    assert_that!(fs::read(checkout.join(".git")).unwrap(), eq(&routing));
    assert_that!(saved.join("HEAD").exists(), eq(true));
    assert_that!(
        fixture.git_text(&other, &["rev-parse", "HEAD"]).as_str(),
        eq(tip.as_str())
    );
    assert_that!(
        fs::read(checkout.join("tracked")).unwrap().as_slice(),
        eq(b"protected checkout\n".as_slice())
    );
}

#[googletest::test]
fn capacity_configuration_is_durable_and_cannot_hide_registered_worktrees() {
    let fixture = Fixture::new();
    let (_, _, id) = fixture.acquisition();
    let initial = fixture.json(&["repo", "inspect", &id]);
    assert_that!(
        initial["data"]["repository"]["capacity"].as_u64(),
        eq(Some(4))
    );
    let configured = fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "2"]);
    assert_that!(configured["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        fixture.json(&["repo", "inspect", &id])["data"]["repository"]["capacity"].as_u64(),
        eq(Some(2))
    );
    let mut command = fixture.command();
    command
        .env("WORKTREE_POOL_MAX_WORKTREES", "99")
        .args(["repo", "inspect", &id]);
    let inspected: Value = serde_json::from_slice(&output(command).stdout).unwrap();
    assert_that!(
        inspected["data"]["repository"]["capacity"].as_u64(),
        eq(Some(2))
    );
    let before = fixture.json(&["events", "list"]);
    let rejected = fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "0"]);
    assert_that!(
        rejected["reason_code"].as_str(),
        eq(Some("capacity_below_count"))
    );
    assert_that!(&fixture.json(&["events", "list"]), eq(&before));
}

impl Fixture {
    fn empty_pool(&self) -> (PathBuf, PathBuf, String) {
        self.json(&["catalog", "init"]);
        let source = self.repository();
        let checkout = self.root.path().join("checkout");
        self.git(
            self.root.path(),
            &["clone".as_ref(), source.as_os_str(), checkout.as_os_str()],
        );
        let registered = self.path_json(&["repo", "register"], &checkout);
        (
            source,
            checkout,
            registered["context"]["repository_id"]
                .as_str()
                .unwrap()
                .to_owned(),
        )
    }
}
#[googletest::test]
fn acquire_creates_detached_registered_worktree_and_reuses_retained_state() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.empty_pool();
    let original = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
    let worktree_id = acquired["context"]["worktree_id"].as_str().unwrap();
    let path = fixture
        .database()
        .parent()
        .unwrap()
        .join("worktrees")
        .join(&id)
        .join(worktree_id);
    assert_that!(
        &acquired["data"]["assignment"]["path"]["bytes"],
        eq(&serde_json::json!(path.as_os_str().as_bytes()))
    );
    assert_that!(
        fixture.git_text(&path, &["rev-parse", "HEAD"]).as_str(),
        eq(original.as_str())
    );
    assert_that!(
        fixture
            .git_text(&path, &["rev-parse", "--abbrev-ref", "HEAD"])
            .as_str(),
        eq("HEAD")
    );
    assert_that!(
        fixture
            .git_text(&checkout, &["branch", "--show-current"])
            .as_str(),
        eq("main")
    );
    let common = checkout.join(".git");
    fs::write(common.join("info/exclude"), b"build-state\n").unwrap();
    fs::write(path.join("build-state"), b"retained build artifact").unwrap();
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    assert_that!(
        fixture.json(&["release", handle])["outcome"].as_str(),
        eq(Some("completed"))
    );
    let again = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(
        &again["context"]["worktree_id"],
        eq(&acquired["context"]["worktree_id"])
    );
    assert_that!(
        again["context"]["assignment_handle"] != acquired["context"]["assignment_handle"],
        eq(true)
    );
    assert_that!(
        fs::read(path.join("build-state")).unwrap().as_slice(),
        eq(b"retained build artifact".as_slice())
    );
    assert_that!(
        fixture.json(&["repo", "inspect", &id])["data"]["registered_count"].as_u64(),
        eq(Some(1))
    );
}

#[googletest::test]
fn default_capacity_counts_assignments_and_reports_immediate_exhaustion() {
    let fixture = Fixture::new();
    let (_, _, id) = fixture.empty_pool();
    let mut handles = std::collections::HashSet::new();
    let mut worktrees = std::collections::HashSet::new();
    for _ in 0..4 {
        let acquired = fixture.json(&["acquire", "--repo", &id]);
        assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            handles.insert(
                acquired["context"]["assignment_handle"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            ),
            eq(true)
        );
        assert_that!(
            worktrees.insert(
                acquired["context"]["worktree_id"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            ),
            eq(true)
        );
    }
    let exhausted = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(
        exhausted["reason_code"].as_str(),
        eq(Some("capacity_all_assigned"))
    );
    assert_that!(exhausted["data"]["registered_count"].as_u64(), eq(Some(4)));
    assert_that!(exhausted["data"]["maximum"].as_u64(), eq(Some(4)));
    assert_that!(
        fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "3"])["reason_code"]
            .as_str(),
        eq(Some("capacity_below_count"))
    );
    assert_that!(
        fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "5"])["outcome"]
            .as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        fixture.json(&["acquire", "--repo", &id])["outcome"].as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        fixture.json(&["repo", "inspect", &id])["data"]["registered_count"].as_u64(),
        eq(Some(5))
    );
}

#[googletest::test]
fn creation_crashes_remain_counted_owned_and_explicitly_inspectable() {
    for checkpoint in [
        "creation-intent",
        "creation-path-effect",
        "creation-path",
        "checkout-intent",
        "creation-effect",
        "result",
    ] {
        let fixture = Fixture::new();
        let (_, _, id) = fixture.empty_pool();
        fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "1"]);
        let mut child = fixture.paused_acquire(&id, checkpoint, false);
        child.kill().unwrap();
        child.wait().unwrap();
        let inspected = fixture.json(&["repo", "inspect", &id]);
        assert_that!(inspected["data"]["registered_count"].as_u64(), eq(Some(1)));
        let worktrees = fixture.json(&["worktree", "list", "--repo", &id]);
        let worktree = &worktrees["data"]["worktrees"][0];
        assert_that!(worktree["creation"]["state"].is_string(), eq(true));
        let operation = worktree["creation"]["operation_id"].as_str().unwrap();
        assert_that!(
            fixture.json(&["operation", "inspect", operation])["outcome"].as_str(),
            eq(Some("completed"))
        );
        let assignment = &fixture.json(&["assignment", "list"])["data"]["assignments"][0];
        assert_that!(
            assignment["state"].as_str(),
            eq(Some(if checkpoint == "result" {
                "active"
            } else {
                "preparing"
            }))
        );
        let before = fixture.json(&["events", "list"]);
        let retry = fixture.json(&["acquire", "--repo", &id]);
        assert_that!(
            retry["reason_code"].as_str(),
            eq(Some(if checkpoint == "result" {
                "capacity_all_assigned"
            } else {
                "operation_pending"
            }))
        );
        if checkpoint != "result" {
            assert_that!(&fixture.json(&["events", "list"]), eq(&before));
        }
        assert_that!(fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "0"])["reason_code"].as_str(), eq(Some("capacity_below_count")));
    }
}

#[googletest::test]
fn creation_never_prepares_preexisting_or_substituted_destinations() {
    use std::io::Write;
    use std::os::unix::fs::symlink;
    for kind in ["preexisting", "substituted", "replaced-directory"] {
        let fixture = Fixture::new();
        let (_, _, id) = fixture.empty_pool();
        let checkpoint = if kind == "preexisting" {
            "creation-intent"
        } else {
            "checkout-intent"
        };
        let mut child = fixture.paused_acquire(&id, checkpoint, true);
        let listed = fixture.json(&["worktree", "list", "--repo", &id]);
        let worktree = &listed["data"]["worktrees"][0];
        let path = PathBuf::from(OsString::from_vec(
            serde_json::from_value(worktree["path"]["bytes"].clone()).unwrap(),
        ));
        let protected = fixture.root.path().join("protected-external");
        fs::create_dir(&protected).unwrap();
        if kind == "preexisting" {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&path)
                .unwrap();
            fs::write(path.join("protected"), b"existing unfinished work").unwrap();
        } else if kind == "substituted" {
            fs::remove_dir(&path).unwrap();
            symlink(&protected, &path).unwrap();
        } else {
            fs::rename(
                &path,
                fixture.root.path().join("retained-prepared-directory"),
            )
            .unwrap();
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        }
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"continue\n")
            .unwrap();
        assert_that!(wait(child).status.success(), eq(true));
        assert_that!(fs::read_dir(&protected).unwrap().count(), eq(0));
        if kind == "replaced-directory" {
            assert_that!(fs::read_dir(&path).unwrap().count(), eq(0));
        }
        if kind == "preexisting" {
            assert_that!(
                fs::read(path.join("protected")).unwrap().as_slice(),
                eq(b"existing unfinished work".as_slice())
            );
        }
        let inspected = fixture.json(&["worktree", "list", "--repo", &id]);
        assert_that!(
            inspected["data"]["worktrees"][0]["creation"]["state"].as_str(),
            eq(Some("needs_reconciliation"))
        );
        assert_that!(
            inspected["data"]["worktrees"][0]["ownership"].as_str(),
            eq(Some("preparing"))
        );
        assert_that!(
            fixture.json(&["repo", "inspect", &id])["data"]["registered_count"].as_u64(),
            eq(Some(1))
        );
    }
}

#[googletest::test]
fn simultaneous_creation_respects_capacity_and_unique_assignment_ownership() {
    for maximum in [1, 2] {
        let fixture = Fixture::new();
        let (_, _, id) = fixture.empty_pool();
        fixture.json(&[
            "pool",
            "configure",
            "--repo",
            &id,
            "--max-worktrees",
            &maximum.to_string(),
        ]);
        let barrier = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(
                fixture
                    .root
                    .path()
                    .join(format!("state/worktree-pool/repository-{id}.lock")),
            )
            .unwrap();
        barrier.lock().unwrap();
        let mut first = fixture.command();
        first.args(["acquire", "--repo", &id]);
        let first = first.spawn().unwrap();
        let mut second = fixture.command();
        second.args(["acquire", "--repo", &id]);
        let second = second.spawn().unwrap();
        barrier.unlock().unwrap();
        let results: Vec<Value> = [first, second]
            .into_iter()
            .map(|p| serde_json::from_slice(&wait(p).stdout).unwrap())
            .collect();
        assert_that!(
            results
                .iter()
                .filter(|r| r["outcome"] == "completed")
                .count(),
            eq(maximum)
        );
        if maximum == 1 {
            assert_that!(
                results
                    .iter()
                    .any(|r| r["reason_code"] == "capacity_all_assigned"),
                eq(true)
            );
        }
        let assignments = fixture.json(&["assignment", "list", "--repo", &id]);
        let rows = assignments["data"]["assignments"].as_array().unwrap();
        assert_that!(rows.len(), eq(maximum));
        let handles: std::collections::HashSet<_> = rows
            .iter()
            .map(|a| a["assignment_handle"].as_str().unwrap())
            .collect();
        let worktrees: std::collections::HashSet<_> = rows
            .iter()
            .map(|a| a["worktree_id"].as_str().unwrap())
            .collect();
        assert_that!(handles.len(), eq(maximum));
        assert_that!(worktrees.len(), eq(maximum));
        assert_that!(
            fixture.json(&["repo", "inspect", &id])["data"]["registered_count"].as_u64(),
            eq(Some(maximum as u64))
        );
    }
}

#[googletest::test]
fn zero_capacity_and_missing_or_withheld_records_never_allocate_overflow() {
    let fixture = Fixture::new();
    let (_, _, id) = fixture.empty_pool();
    assert_that!(
        fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "0"])["outcome"]
            .as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        fixture.json(&["acquire", "--repo", &id])["reason_code"].as_str(),
        eq(Some("capacity_zero"))
    );
    fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "2"]);
    let first = fixture.json(&["acquire", "--repo", &id]);
    let second = fixture.json(&["acquire", "--repo", &id]);
    let first_handle = first["context"]["assignment_handle"].as_str().unwrap();
    fixture.json(&["release", first_handle]);
    let path = PathBuf::from(OsString::from_vec(
        serde_json::from_value(first["data"]["assignment"]["path"]["bytes"].clone()).unwrap(),
    ));
    fs::remove_dir_all(&path).unwrap(); // Explicit human removal in the private fixture.
    let rejected = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(
        rejected["reason_code"].as_str(),
        eq(Some("capacity_no_safe_worktree"))
    );
    assert_that!(rejected["data"]["registered_count"].as_u64(), eq(Some(2)));
    let missing = fixture.json(&[
        "worktree",
        "inspect",
        first["context"]["worktree_id"].as_str().unwrap(),
    ]);
    assert_that!(
        missing["data"]["worktree"]["registration_state"].as_str(),
        eq(Some("missing"))
    );
    assert_that!(
        missing["data"]["worktree"]["availability"].as_str(),
        eq(Some("withheld"))
    );
    assert_that!(
        fixture.json(&[
            "assignment",
            "inspect",
            second["context"]["assignment_handle"].as_str().unwrap()
        ])["data"]["assignment"]["state"]
            .as_str(),
        eq(Some("active"))
    );
    assert_that!(
        fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "1"])["reason_code"]
            .as_str(),
        eq(Some("capacity_below_count"))
    );
}

#[googletest::test]
fn failed_git_creation_stays_counted_and_never_retries_after_obstruction_is_removed() {
    let fixture = Fixture::new();
    let (source, checkout, id) = fixture.empty_pool();
    fs::write(source.join(".gitattributes"), b"tracked filter=blocked\n").unwrap();
    fixture.git(&source, &["add".as_ref(), ".gitattributes".as_ref()]);
    fixture.git(
        &source,
        &[
            "commit".as_ref(),
            "-m".as_ref(),
            "required failing filter".as_ref(),
        ],
    );
    fixture.git(
        &checkout,
        &[
            "config".as_ref(),
            "filter.blocked.smudge".as_ref(),
            "false".as_ref(),
        ],
    );
    fixture.git(
        &checkout,
        &[
            "config".as_ref(),
            "filter.blocked.required".as_ref(),
            "true".as_ref(),
        ],
    );
    let pending = fixture.json(&["acquire", "--repo", &id]);
    assert_that!(pending["outcome"].as_str(), eq(Some("pending")));
    let worktree = fixture.json(&[
        "worktree",
        "inspect",
        pending["context"]["worktree_id"].as_str().unwrap(),
    ]);
    assert_that!(
        worktree["data"]["worktree"]["creation"]["state"].as_str(),
        eq(Some("needs_reconciliation"))
    );
    assert_that!(
        worktree["data"]["worktree"]["ownership"].as_str(),
        eq(Some("preparing"))
    );
    let before = fixture.json(&["events", "list"]);
    fixture.git(
        &checkout,
        &[
            "config".as_ref(),
            "--unset".as_ref(),
            "filter.blocked.required".as_ref(),
        ],
    );
    fixture.git(
        &checkout,
        &[
            "config".as_ref(),
            "--unset".as_ref(),
            "filter.blocked.smudge".as_ref(),
        ],
    );
    assert_that!(
        fixture.json(&["acquire", "--repo", &id])["reason_code"].as_str(),
        eq(Some("operation_pending"))
    );
    assert_that!(&fixture.json(&["events", "list"]), eq(&before));
    assert_that!(
        fixture.json(&["repo", "inspect", &id])["data"]["registered_count"].as_u64(),
        eq(Some(1))
    );
    assert_that!(
        fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "0"])["reason_code"]
            .as_str(),
        eq(Some("capacity_below_count"))
    );
    assert_that!(
        fixture
            .git_text(&checkout, &["branch", "--show-current"])
            .as_str(),
        eq("main")
    );
}

#[googletest::test]
fn lost_creation_stdout_is_inspectable_without_duplicate_allocation() {
    let fixture = Fixture::new();
    let (_, _, id) = fixture.empty_pool();
    fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "1"]);
    let mut command = fixture.command();
    command.args(["acquire", "--repo", &id]).stdout(
        fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap(),
    );
    assert_that!(output(command).status.code(), eq(Some(4)));
    let listed = fixture.json(&["assignment", "list", "--repo", &id]);
    let assignments = listed["data"]["assignments"].as_array().unwrap();
    assert_that!(assignments.len(), eq(1));
    let handle = assignments[0]["assignment_handle"].as_str().unwrap();
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("active"))
    );
    let worktrees = fixture.json(&["worktree", "list", "--repo", &id]);
    assert_that!(
        worktrees["data"]["worktrees"][0]["creation"]["state"].as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        fixture.json(&["acquire", "--repo", &id])["reason_code"].as_str(),
        eq(Some("capacity_all_assigned"))
    );
    assert_that!(
        fixture.json(&["repo", "inspect", &id])["data"]["registered_count"].as_u64(),
        eq(Some(1))
    );
}

#[googletest::test]
fn created_worktree_paths_preserve_catalog_bytes_and_user_only_permissions() {
    let fixture = Fixture::new();
    let directory = fixture
        .root
        .path()
        .join(OsString::from_vec(b"catalog-\xff\n ".to_vec()));
    let invoke = |args: &[&str]| -> Value {
        let mut command = fixture.command();
        command.arg("--catalog-dir").arg(&directory).args(args);
        serde_json::from_slice(&output(command).stdout).unwrap()
    };
    invoke(&["catalog", "init"]);
    let source = fixture.repository();
    let checkout = fixture.root.path().join("context");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), source.as_os_str(), checkout.as_os_str()],
    );
    let original_mode = fs::metadata(&checkout).unwrap().permissions().mode();
    let registered = invoke(&["repo", "register", checkout.to_str().unwrap()]);
    let id = registered["context"]["repository_id"].as_str().unwrap();
    let acquired = invoke(&["acquire", "--repo", id]);
    assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
    let path = directory
        .join("worktrees")
        .join(id)
        .join(acquired["context"]["worktree_id"].as_str().unwrap());
    assert_that!(
        &acquired["data"]["assignment"]["path"]["bytes"],
        eq(&serde_json::json!(path.as_os_str().as_bytes()))
    );
    assert_that!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        eq(0o700)
    );
    assert_that!(
        fs::metadata(&checkout).unwrap().permissions().mode(),
        eq(original_mode)
    );
    let inspected = invoke(&[
        "worktree",
        "inspect",
        acquired["context"]["worktree_id"].as_str().unwrap(),
    ]);
    assert_that!(
        inspected["data"]["worktree"]["registration_state"].as_str(),
        eq(Some("registered"))
    );
    assert_that!(
        fixture
            .git_text(&path, &["rev-parse", "--abbrev-ref", "HEAD"])
            .as_str(),
        eq("HEAD")
    );
}

#[googletest::test]
fn recovery_preview_observes_known_active_ownership_without_changing_protected_state() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let before = fixture.json(&["events", "list"]);
    let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let index_path = fixture.git_text(
        &checkout,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    );
    let index = fs::read(&index_path).unwrap();
    let preview = fixture.json(&["recover", "preview", "--assignment", handle]);
    assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
    assert_that!(preview["data"]["ownership"].as_str(), eq(Some("active")));
    assert_that!(
        preview["data"]["availability"].as_str(),
        eq(Some("withheld"))
    );
    assert_that!(
        preview["context"]["assignment_handle"].as_str(),
        eq(Some(handle))
    );
    assert_that!(fixture.json(&["events", "list"]), eq(&before));
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
        eq(&head)
    );
    assert_that!(fs::read(&index_path).unwrap(), eq(&index));
}

#[googletest::test]
fn explicit_abandonment_preserves_the_actual_detached_tip_before_ending_ownership() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    fixture.git(
        &checkout,
        &[
            "commit".as_ref(),
            "--allow-empty".as_ref(),
            "-m".as_ref(),
            "abandoned committed work".as_ref(),
        ],
    );
    let tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let recovered = fixture.json(&["recover", "apply", "--assignment", handle, "--abandon"]);
    assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        recovered["data"]["ownership"].as_str(),
        eq(Some("released"))
    );
    let reference = recovered["data"]["preservation_reference"]
        .as_str()
        .unwrap();
    assert_that!(
        fixture.git_text(&checkout, &["show-ref", "--verify", "--hash", reference]),
        eq(&tip)
    );
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("released"))
    );
    let op = recovered["context"]["operation_id"].as_str().unwrap();
    assert_that!(
        fixture.json(&["operation", "inspect", op])["data"]["operation"]["state"].as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
        eq(&tip)
    );
}

#[googletest::test]
fn every_public_inspection_preserves_catalog_bytes_as_well_as_history_and_checkout() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let operation = acquired["context"]["operation_id"].as_str().unwrap();
    let worktree = acquired["context"]["worktree_id"].as_str().unwrap();
    let database = fs::read(fixture.database()).unwrap();
    let locator = fs::read(fixture.root.path().join("state/worktree-pool/active.json")).unwrap();
    let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let index = fs::read(checkout.join(".git/index")).unwrap();
    for args in [
        vec!["catalog", "info"],
        vec!["catalog", "check"],
        vec!["repo", "list"],
        vec!["repo", "inspect", &id],
        vec!["worktree", "list"],
        vec!["worktree", "inspect", worktree],
        vec!["assignment", "list"],
        vec!["assignment", "inspect", handle],
        vec!["operation", "list"],
        vec!["operation", "inspect", operation],
        vec!["events", "list"],
        vec!["recover", "preview", "--assignment", handle],
    ] {
        let result = fixture.run(&args);
        assert_that!(result.status.code(), eq(Some(0)));
        assert_that!(fs::read(fixture.database()).unwrap() == database, eq(true));
        assert_that!(
            fs::read(fixture.root.path().join("state/worktree-pool/active.json")).unwrap()
                == locator,
            eq(true)
        );
        assert_that!(
            fs::read(checkout.join(".git/index")).unwrap() == index,
            eq(true)
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&head)
        );
    }
}

impl Fixture {
    fn paused_recovery(&self, handle: &str, checkpoint: &str, continuable: bool) -> Child {
        use std::io::{BufRead, BufReader};
        let binary = PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
            .join(env::var_os("TEST_WORKSPACE").unwrap())
            .join(env!("CRASH_BINARY"));
        let environment = self.command();
        let mut command = Command::new(binary);
        command.env_clear();
        for (name, value) in environment.get_envs() {
            if let Some(value) = value {
                command.env(name, value);
            }
        }
        let mut child = command
            .env("OPERATION", "recover")
            .env("ASSIGNMENT_HANDLE", handle)
            .env("CHECKPOINT", checkpoint)
            .env("CONTINUABLE", if continuable { "1" } else { "0" })
            .stdin(Stdio::piped())
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
                .is_ok_and(|s| s != &format!("checkpoint:{checkpoint}\n"))
        {
            let _ = child.kill();
            panic!(
                "recovery checkpoint missing: {observed:?}; {:?}",
                child.wait_with_output().unwrap()
            );
        }
        reader.join().unwrap();
        child
    }
}

#[googletest::test]
fn interrupted_recovery_resumes_the_recorded_operation_without_duplicate_preservation_or_release() {
    for checkpoint in ["intent", "preservation-effect", "preserved", "result"] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        let acquired = fixture.json(&["acquire", "--repo", &id]);
        let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
        let mut child = fixture.paused_recovery(handle, checkpoint, false);
        child.kill().unwrap();
        child.wait().unwrap();
        if checkpoint == "intent" {
            let mut resumed = fixture.paused_recovery(handle, "preservation-effect", false);
            resumed.kill().unwrap();
            resumed.wait().unwrap();
        }
        let operations = fixture.json(&["operation", "list"]);
        let recoveries: Vec<_> = operations["data"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| {
                o["last_checkpoint"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("recovery_"))
            })
            .collect();
        assert_that!(recoveries.len(), eq(1));
        let operation_id = recoveries[0]["operation_id"].as_str().unwrap();
        let before = fixture.json(&["events", "list"]);
        let preview = fixture.json(&["recover", "preview", "--operation", operation_id]);
        assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
        assert_that!(fixture.json(&["events", "list"]), eq(&before));
        let reconciled = fixture.json(&["recover", "apply", "--operation", operation_id]);
        assert_that!(reconciled["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            reconciled["context"]["operation_id"].as_str(),
            eq(Some(operation_id))
        );
        assert_that!(
            reconciled["data"]["ownership"].as_str(),
            eq(Some("released"))
        );
        let reference = reconciled["data"]["preservation_reference"]
            .as_str()
            .unwrap();
        assert_that!(
            fixture.git_text(&checkout, &["show-ref", "--verify", "--hash", reference]),
            eq(&fixture.git_text(&checkout, &["rev-parse", "HEAD"]))
        );
        let events = fixture.json(&["events", "list"]);
        let count = events["data"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["operationid"].as_str() == Some(operation_id))
            .count();
        assert_that!(count, eq(3));
        let repeated = fixture.json(&["recover", "apply", "--operation", operation_id]);
        assert_that!(repeated["outcome"].as_str(), eq(Some("completed")));
        assert_that!(fixture.json(&["events", "list"]), eq(&events));
    }
}

#[googletest::test]
fn recovery_observes_a_completed_checkout_and_activates_the_same_preparing_assignment() {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "1"]);
    let mut child = fixture.paused_acquire(&id, "checkout-effect", false);
    child.kill().unwrap();
    child.wait().unwrap();
    let assignments = fixture.json(&["assignment", "list"]);
    let assignment = &assignments["data"]["assignments"][0];
    let handle = assignment["assignment_handle"].as_str().unwrap();
    let operation = assignment["operation_id"].as_str().unwrap();
    let before = fixture.json(&["events", "list"]);
    let database = fs::read(fixture.database()).unwrap();
    let preview = fixture.json(&["recover", "preview", "--operation", operation]);
    assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
    assert_that!(preview["data"]["ownership"].as_str(), eq(Some("preparing")));
    assert_that!(fs::read(fixture.database()).unwrap() == database, eq(true));
    assert_that!(fixture.json(&["events", "list"]), eq(&before));
    let recovered = fixture.json(&["recover", "apply", "--operation", operation]);
    assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
    assert_that!(recovered["data"]["ownership"].as_str(), eq(Some("active")));
    assert_that!(
        recovered["context"]["assignment_handle"].as_str(),
        eq(Some(handle))
    );
    assert_that!(
        recovered["context"]["operation_id"].as_str() != Some(operation),
        eq(true)
    );
    assert_that!(
        fixture.json(&["assignment", "list"])["data"]["assignments"]
            .as_array()
            .unwrap()
            .len(),
        eq(1)
    );
    assert_that!(
        fixture.json(&["operation", "inspect", operation])["data"]["operation"]["state"].as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        fixture.json(&["acquire", "--repo", &id])["reason_code"].as_str(),
        eq(Some("capacity_all_assigned"))
    );
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
        eq(&assignment["resolved_commit"].as_str().unwrap().to_owned())
    );
}

#[googletest::test]
fn creation_reconciliation_observes_linked_operations_and_keeps_unproven_partial_paths_withheld() {
    use std::os::unix::fs::MetadataExt;
    for checkpoint in ["creation-intent", "creation-path", "creation-effect"] {
        let fixture = Fixture::new();
        let (_, _, id) = fixture.empty_pool();
        fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "1"]);
        let mut child = fixture.paused_acquire(&id, checkpoint, false);
        child.kill().unwrap();
        child.wait().unwrap();
        let listed = fixture.json(&["worktree", "list"]);
        let w = &listed["data"]["worktrees"][0];
        let creation = w["creation"]["operation_id"].as_str().unwrap();
        let acquisition = w["creation"]["acquisition_operation_id"].as_str().unwrap();
        let handle = w["assignment_handle"].as_str().unwrap();
        let bytes: Vec<u8> = serde_json::from_value(w["path"]["bytes"].clone()).unwrap();
        let path = PathBuf::from(OsString::from_vec(bytes));
        let before_path = fs::symlink_metadata(&path)
            .ok()
            .map(|m| (m.dev(), m.ino(), m.mode()));
        let database = fs::read(fixture.database()).unwrap();
        let preview = fixture.json(&["recover", "preview", "--operation", creation]);
        assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
        assert_that!(fs::read(fixture.database()).unwrap() == database, eq(true));
        let recovered = fixture.json(&["recover", "apply", "--operation", creation]);
        assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            recovered["data"]["ownership"].as_str(),
            eq(Some(if checkpoint == "creation-effect" {
                "active"
            } else {
                "preparing"
            }))
        );
        assert_that!(
            recovered["data"]["availability"].as_str(),
            eq(Some("withheld"))
        );
        assert_that!(
            recovered["context"]["assignment_handle"].as_str(),
            eq(Some(handle))
        );
        for operation in [creation, acquisition] {
            assert_that!(
                fixture.json(&["operation", "inspect", operation])["data"]["operation"]["state"]
                    .as_str(),
                eq(Some(if checkpoint == "creation-effect" {
                    "completed"
                } else {
                    "reconciled"
                }))
            );
        }
        assert_that!(
            fs::symlink_metadata(&path)
                .ok()
                .map(|m| (m.dev(), m.ino(), m.mode())),
            eq(before_path)
        );
        if checkpoint == "creation-path" {
            assert_that!(fs::read_dir(&path).unwrap().count(), eq(0));
        }
        assert_that!(
            fixture.json(&["repo", "inspect", &id])["data"]["registered_count"].as_u64(),
            eq(Some(1))
        );
        assert_that!(
            fixture.json(&["acquire", "--repo", &id])["reason_code"].as_str(),
            eq(Some("capacity_all_assigned"))
        );
    }
}

impl Fixture {
    fn paused_catalog(&self, operation: &str, checkpoint: &str, continuable: bool) -> Child {
        self.paused_recovery_checkpoint(operation, checkpoint, continuable, None)
    }
    fn paused_repository_recovery(&self, operation_id: &str, checkpoint: &str) -> Child {
        self.paused_recovery_checkpoint("recover-repository", checkpoint, false, Some(operation_id))
    }
    fn paused_recovery_checkpoint(
        &self,
        operation: &str,
        checkpoint: &str,
        continuable: bool,
        operation_id: Option<&str>,
    ) -> Child {
        use std::io::{BufRead, BufReader};
        let binary = PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
            .join(env::var_os("TEST_WORKSPACE").unwrap())
            .join(env!("CRASH_BINARY"));
        let environment = self.command();
        let mut command = Command::new(binary);
        command.env_clear();
        for (name, value) in environment.get_envs() {
            if let Some(value) = value {
                command.env(name, value);
            }
        }
        if let Some(operation_id) = operation_id {
            command.env("OPERATION_ID", operation_id);
        }
        let mut child = command
            .env("OPERATION", operation)
            .env("CHECKPOINT", checkpoint)
            .env("CONTINUABLE", if continuable { "1" } else { "0" })
            .stdin(Stdio::piped())
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
                .is_ok_and(|s| s != &format!("checkpoint:{checkpoint}\n"))
        {
            let _ = child.kill();
            panic!(
                "catalog checkpoint missing: {observed:?}; {:?}",
                child.wait_with_output().unwrap()
            );
        }
        reader.join().unwrap();
        child
    }
}

#[googletest::test]
fn catalog_bootstrap_recovery_is_explicit_readonly_previewed_and_inspectable_by_its_recorded_id() {
    for checkpoint in ["intent", "store"] {
        let fixture = Fixture::new();
        let mut child = fixture.paused_catalog("init", checkpoint, false);
        child.kill().unwrap();
        child.wait().unwrap();
        let locator =
            fs::read(fixture.root.path().join("state/worktree-pool/active.json")).unwrap();
        let database = fs::read(fixture.database()).ok();
        let preview = fixture.json(&["recover", "preview", "--catalog"]);
        assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            preview["data"]["authority"]["phase"].as_str(),
            eq(Some("initializing"))
        );
        assert_that!(
            fs::read(fixture.root.path().join("state/worktree-pool/active.json")).unwrap()
                == locator,
            eq(true)
        );
        assert_that!(fs::read(fixture.database()).ok() == database, eq(true));
        let recovered = fixture.json(&["recover", "apply", "--catalog"]);
        assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            &recovered["context"]["catalog_id"],
            eq(&preview["context"]["catalog_id"])
        );
        assert_that!(
            recovered["data"]["authority"]["phase"].as_str(),
            eq(Some("active"))
        );
        let id = recovered["context"]["operation_id"].as_str().unwrap();
        assert_that!(
            fixture.json(&["operation", "inspect", id])["data"]["operation"]["state"].as_str(),
            eq(Some("completed"))
        );
        assert_that!(
            fixture.json(&["operation", "list"])["data"]["operations"]
                .as_array()
                .unwrap()
                .iter()
                .any(|o| o["operation_id"].as_str() == Some(id)),
            eq(true)
        );
        let before = fs::read(fixture.database()).unwrap();
        let before_locator =
            fs::read(fixture.root.path().join("state/worktree-pool/active.json")).unwrap();
        assert_that!(
            fixture.json(&["recover", "preview", "--operation", id])["outcome"].as_str(),
            eq(Some("completed"))
        );
        assert_that!(
            fixture.json(&["recover", "apply", "--operation", id])["outcome"].as_str(),
            eq(Some("completed"))
        );
        assert_that!(fs::read(fixture.database()).unwrap() == before, eq(true));
        assert_that!(
            fs::read(fixture.root.path().join("state/worktree-pool/active.json")).unwrap()
                == before_locator,
            eq(true)
        );
        assert_that!(
            fixture.json(&["events", "list"])["data"]["events"]
                .as_array()
                .unwrap()
                .len(),
            eq(1)
        );
    }
}

#[googletest::test]
fn release_reconciliation_observes_or_completes_the_recorded_preservation_before_release() {
    for checkpoint in ["intent", "preservation-effect", "preserved"] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        let acquired = fixture.json(&["acquire", "--repo", &id]);
        let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
        fixture.git(
            &checkout,
            &[
                "commit".as_ref(),
                "--allow-empty".as_ref(),
                "-m".as_ref(),
                "committed caller work".as_ref(),
            ],
        );
        let tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let mut child = fixture.paused_release(handle, checkpoint, false);
        child.kill().unwrap();
        child.wait().unwrap();
        let operations = fixture.json(&["operation", "list"]);
        let release = operations["data"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| {
                o["assignment_handle"].as_str() == Some(handle)
                    && o["tip"].as_str() == Some(tip.as_str())
            })
            .unwrap();
        let operation = release["operation_id"].as_str().unwrap();
        let before = fs::read(fixture.database()).unwrap();
        let preview = fixture.json(&["recover", "preview", "--operation", operation]);
        assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
        assert_that!(preview["data"]["ownership"].as_str(), eq(Some("active")));
        assert_that!(fs::read(fixture.database()).unwrap() == before, eq(true));
        let recovered = fixture.json(&["recover", "apply", "--operation", operation]);
        assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            recovered["data"]["ownership"].as_str(),
            eq(Some("released"))
        );
        assert_that!(
            recovered["context"]["assignment_handle"].as_str(),
            eq(Some(handle))
        );
        let reference = release["preservation_reference"].as_str().unwrap();
        assert_that!(
            fixture.git_text(&checkout, &["show-ref", "--verify", "--hash", reference]),
            eq(&tip)
        );
        assert_that!(
            fixture.json(&["operation", "inspect", operation])["data"]["operation"]["state"]
                .as_str(),
            eq(Some("completed"))
        );
        let events = fixture.json(&["events", "list"]);
        assert_that!(
            fixture.json(&["release", handle])["reason_code"].as_str(),
            eq(Some("already_released"))
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&events));
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&tip)
        );
    }
}

#[googletest::test]
fn refresh_reconciliation_never_repeats_fetch_or_claims_observed_refs_are_fresh() {
    for checkpoint in ["intent", "fetch"] {
        let fixture = Fixture::new();
        let (source, checkout, id) = fixture.acquisition();
        let mut child = fixture.paused_refresh(&id, checkpoint);
        child.kill().unwrap();
        child.wait().unwrap();
        let operations = fixture.json(&["operation", "list", "--repo", &id]);
        let operation = operations["data"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["state"].as_str() == Some("pending"))
            .unwrap()["operation_id"]
            .as_str()
            .unwrap();
        let before = fs::read(fixture.database()).unwrap();
        assert_that!(
            fixture.json(&["recover", "preview", "--operation", operation])["outcome"].as_str(),
            eq(Some("completed"))
        );
        assert_that!(fs::read(fixture.database()).unwrap() == before, eq(true));
        let ref_before = fixture.git_text(&checkout, &["rev-parse", "refs/remotes/origin/main"]);
        fixture.git(
            &source,
            &[
                "commit".as_ref(),
                "--allow-empty".as_ref(),
                "-m".as_ref(),
                "remote advanced after interrupted fetch".as_ref(),
            ],
        );
        let recovered = fixture.json(&["recover", "apply", "--operation", operation]);
        assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            recovered["data"]["fresh_fetch_proven"].as_bool(),
            eq(Some(false))
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "refs/remotes/origin/main"]),
            eq(&ref_before)
        );
        assert_that!(
            fixture.json(&["operation", "inspect", operation])["data"]["operation"]["state"]
                .as_str(),
            eq(Some("reconciled"))
        );
        let history = fixture.json(&["events", "list"]);
        assert_that!(
            fixture.json(&["recover", "apply", "--operation", operation])["outcome"].as_str(),
            eq(Some("completed"))
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&history));
        assert_that!(
            fixture.json(&["repo", "refresh", "--repo", &id])["outcome"].as_str(),
            eq(Some("completed"))
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "refs/remotes/origin/main"]),
            eq(&fixture.git_text(&source, &["rev-parse", "HEAD"]))
        );
    }
}

#[googletest::test]
fn unidentified_recovery_lists_candidates_and_requires_an_exact_identity_without_ending_ownership()
{
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "2"]);
    let first = fixture.json(&["acquire", "--repo", &id]);
    let second = fixture.json(&["acquire", "--repo", &id]);
    let first_handle = first["context"]["assignment_handle"].as_str().unwrap();
    let second_handle = second["context"]["assignment_handle"].as_str().unwrap();
    let first_worktree = first["context"]["worktree_id"].as_str().unwrap();
    let before = fs::read(fixture.database()).unwrap();
    let index_path = fixture.git_text(
        &checkout,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    );
    let index = fs::read(&index_path).unwrap();
    let preview = fixture.json(&["recover", "preview", "--repo", &id]);
    assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
    let candidates = preview["data"]["candidates"].as_array().unwrap();
    assert_that!(candidates.len(), eq(2));
    assert_that!(
        candidates
            .iter()
            .any(|a| a["assignment_handle"].as_str() == Some(first_handle)),
        eq(true)
    );
    assert_that!(
        candidates
            .iter()
            .any(|a| a["assignment_handle"].as_str() == Some(second_handle)),
        eq(true)
    );
    assert_that!(
        fixture.json(&["recover", "apply", "--repo", &id])["outcome"].as_str(),
        eq(Some("rejected"))
    );
    assert_that!(fs::read(fixture.database()).unwrap() == before, eq(true));
    assert_that!(fs::read(&index_path).unwrap() == index, eq(true));
    let recovered = fixture.json(&[
        "recover",
        "apply",
        "--worktree",
        first_worktree,
        "--repo",
        &id,
    ]);
    assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        recovered["context"]["assignment_handle"].as_str(),
        eq(Some(first_handle))
    );
    assert_that!(recovered["data"]["ownership"].as_str(), eq(Some("active")));
    assert_that!(
        recovered["data"]["availability"].as_str(),
        eq(Some("withheld"))
    );
    assert_that!(
        fixture.json(&["assignment", "inspect", second_handle])["data"]["assignment"]["state"]
            .as_str(),
        eq(Some("active"))
    );
    assert_that!(
        fixture.git_text(
            &checkout,
            &[
                "show-ref",
                "--verify",
                "--hash",
                recovered["data"]["preservation_reference"]
                    .as_str()
                    .unwrap()
            ]
        ),
        eq(&fixture.git_text(&checkout, &["rev-parse", "HEAD"]))
    );
}

#[googletest::test]
fn unassigned_withheld_worktree_reconciliation_observes_protected_work_and_releases_only_safe_withholding()
 {
    let fixture = Fixture::new();
    let (_, checkout, id) = fixture.acquisition();
    fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "1"]);
    let registered = fixture.json(&["worktree", "list", "--repo", &id]);
    let worktree = registered["data"]["worktrees"][0]["worktree_id"]
        .as_str()
        .unwrap();
    fs::write(checkout.join("tracked"), b"protected unfinished work\n").unwrap();
    assert_that!(
        fixture.json(&["acquire", "--repo", &id])["outcome"].as_str(),
        eq(Some("rejected"))
    );
    let before = fs::read(fixture.database()).unwrap();
    let index_path = fixture.git_text(
        &checkout,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    );
    let index = fs::read(&index_path).unwrap();
    let preview = fixture.path_json(&["recover", "preview", "--worktree"], &checkout);
    assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        preview["data"]["observation"]["safe"].as_bool(),
        eq(Some(false))
    );
    assert_that!(fs::read(fixture.database()).unwrap() == before, eq(true));
    assert_that!(fs::read(&index_path).unwrap() == index, eq(true));
    let unsafe_result = fixture.json(&["recover", "apply", "--worktree", worktree]);
    assert_that!(unsafe_result["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        unsafe_result["data"]["ownership"].as_str(),
        eq(Some("unassigned"))
    );
    assert_that!(
        unsafe_result["data"]["availability"].as_str(),
        eq(Some("withheld"))
    );
    assert_that!(
        fs::read(checkout.join("tracked")).unwrap(),
        eq(&b"protected unfinished work\n".to_vec())
    );
    assert_that!(fs::read(&index_path).unwrap() == index, eq(true));
    // The caller explicitly saves the unfinished content; recovery itself never edits it.
    fixture.git(&checkout, &["add".as_ref(), "tracked".as_ref()]);
    fixture.git(
        &checkout,
        &[
            "commit".as_ref(),
            "-m".as_ref(),
            "caller saved protected work".as_ref(),
        ],
    );
    let recovered = fixture.json(&["recover", "apply", "--worktree", worktree]);
    assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        recovered["data"]["ownership"].as_str(),
        eq(Some("unassigned"))
    );
    assert_that!(
        recovered["data"]["availability"].as_str(),
        eq(Some("unverified"))
    );
    assert_that!(
        fixture.json(&["assignment", "list"])["data"]["assignments"]
            .as_array()
            .unwrap()
            .is_empty(),
        eq(true)
    );
    assert_that!(
        fixture.json(&["worktree", "inspect", worktree])["data"]["worktree"]["availability"]
            .as_str(),
        eq(Some("unverified"))
    );
}

#[googletest::test]
fn recovery_pending_survives_process_death_blocks_competing_effects_and_allows_an_independent_repository()
 {
    let fixture = Fixture::new();
    let (source, _, id) = fixture.acquisition();
    let acquired = fixture.json(&["acquire", "--repo", &id]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let other = fixture.root.path().join("independent-recovery-clone");
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), source.as_os_str(), other.as_os_str()],
    );
    let registered = fixture.path_json(&["repo", "register"], &other);
    let other_id = registered["context"]["repository_id"].as_str().unwrap();
    fixture.path_json(&["worktree", "register", "--repo", other_id], &other);
    let mut recovering = fixture.paused_recovery(handle, "intent", false);
    let mut waiting = Vec::new();
    for args in [
        vec!["acquire", "--repo", &id],
        vec!["release", handle],
        vec!["repo", "refresh", "--repo", &id],
    ] {
        let mut command = fixture.command();
        command.args(args);
        waiting.push(command.spawn().unwrap());
    }
    assert_that!(
        fixture.json(&["acquire", "--repo", other_id])["outcome"].as_str(),
        eq(Some("completed"))
    );
    recovering.kill().unwrap();
    recovering.wait().unwrap();
    let before = fixture.json(&["events", "list"]);
    for child in waiting {
        let result = wait(child);
        assert_that!(result.status.code(), eq(Some(3)));
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_that!(value["reason_code"].as_str(), eq(Some("operation_pending")));
    }
    assert_that!(fixture.json(&["events", "list"]), eq(&before));
    let operations = fixture.json(&["operation", "list", "--repo", &id]);
    let recovery = operations["data"]["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["last_checkpoint"].as_str() == Some("recovery_intended"))
        .unwrap()["operation_id"]
        .as_str()
        .unwrap();
    let mut resumes = Vec::new();
    for _ in 0..2 {
        let mut command = fixture.command();
        command.args(["recover", "apply", "--operation", recovery]);
        resumes.push(command.spawn().unwrap());
    }
    for child in resumes {
        assert_that!(wait(child).status.code(), eq(Some(0)));
    }
    let after = fixture.json(&["events", "list"]);
    assert_that!(
        after["data"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["operationid"].as_str() == Some(recovery))
            .count(),
        eq(3)
    );
    assert_that!(
        fixture.json(&["release", handle])["reason_code"].as_str(),
        eq(Some("already_released"))
    );
    assert_that!(fixture.json(&["events", "list"]), eq(&after));
}

#[googletest::test]
fn preparing_assignment_abandonment_requires_safe_work_and_settles_the_original_preparation() {
    for checkpoint in ["reservation", "checkout-effect"] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        let mut child = fixture.paused_acquire(&id, checkpoint, false);
        child.kill().unwrap();
        child.wait().unwrap();
        let assignments = fixture.json(&["assignment", "list", "--repo", &id]);
        let assignment = &assignments["data"]["assignments"][0];
        let handle = assignment["assignment_handle"].as_str().unwrap();
        let operation = assignment["operation_id"].as_str().unwrap();
        fs::write(
            checkout.join("tracked"),
            b"caller owns unfinished preparation\n",
        )
        .unwrap();
        let before = fixture.json(&["events", "list"]);
        assert_that!(
            fixture.json(&["recover", "apply", "--assignment", handle, "--abandon"])["reason_code"]
                .as_str(),
            eq(Some("unfinished_work"))
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&before));
        fixture.git(&checkout, &["add".as_ref(), "tracked".as_ref()]);
        fixture.git(
            &checkout,
            &[
                "commit".as_ref(),
                "-m".as_ref(),
                "caller saved interrupted preparation".as_ref(),
            ],
        );
        let tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let recovered = fixture.json(&["recover", "apply", "--assignment", handle, "--abandon"]);
        assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            recovered["data"]["ownership"].as_str(),
            eq(Some("released"))
        );
        assert_that!(
            fixture.json(&["operation", "inspect", operation])["data"]["operation"]["state"]
                .as_str(),
            eq(Some("reconciled"))
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&tip)
        );
        assert_that!(
            fixture.json(&["release", handle])["reason_code"].as_str(),
            eq(Some("already_released"))
        );
        assert_that!(
            fixture.json(&["acquire", "--repo", &id])["outcome"].as_str(),
            eq(Some("completed"))
        );
    }
}

#[googletest::test]
fn recovery_rejects_conflicting_or_symbolic_preservation_roots_without_ending_ownership() {
    for symbolic in [false, true] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        let acquired = fixture.json(&["acquire", "--repo", &id]);
        let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
        let old = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        fixture.git(
            &checkout,
            &[
                "commit".as_ref(),
                "--allow-empty".as_ref(),
                "-m".as_ref(),
                "new detached caller tip".as_ref(),
            ],
        );
        let tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let mut child = fixture.paused_recovery(handle, "intent", false);
        child.kill().unwrap();
        child.wait().unwrap();
        let operations = fixture.json(&["operation", "list", "--repo", &id]);
        let operation = operations["data"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["last_checkpoint"].as_str() == Some("recovery_intended"))
            .unwrap();
        let recovery = operation["operation_id"].as_str().unwrap();
        let reference = operation["preservation_reference"].as_str().unwrap();
        if symbolic {
            fixture.git(
                &checkout,
                &[
                    "update-ref".as_ref(),
                    "refs/heads/private-fixture".as_ref(),
                    tip.as_ref(),
                ],
            );
            fixture.git(
                &checkout,
                &[
                    "symbolic-ref".as_ref(),
                    reference.as_ref(),
                    "refs/heads/private-fixture".as_ref(),
                ],
            );
        } else {
            fixture.git(
                &checkout,
                &["update-ref".as_ref(), reference.as_ref(), old.as_ref()],
            );
        }
        let before = fixture.json(&["events", "list"]);
        let before_refs = fixture.git_text(&checkout, &["show-ref"]);
        let result = fixture.json(&["recover", "apply", "--operation", recovery]);
        assert_that!(result["outcome"].as_str(), eq(Some("pending")));
        assert_that!(result["data"]["ownership"].as_str(), eq(Some("active")));
        assert_that!(fixture.json(&["events", "list"]), eq(&before));
        assert_that!(fixture.git_text(&checkout, &["show-ref"]), eq(&before_refs));
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&tip)
        );
    }
}

#[googletest::test]
fn exact_reconciled_original_ids_return_the_same_completed_withheld_result_without_new_history() {
    for creation in [false, true] {
        let fixture = Fixture::new();
        let (_, checkout, id) = fixture.acquisition();
        let original = if creation {
            fixture.json(&["acquire", "--repo", &id]);
            fixture.json(&["pool", "configure", "--repo", &id, "--max-worktrees", "2"]);
            let mut child = fixture.paused_acquire(&id, "creation-path", false);
            child.kill().unwrap();
            child.wait().unwrap();
            let operations = fixture.json(&["operation", "list", "--repo", &id]);
            operations["data"]["operations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|o| o["last_checkpoint"].as_str() == Some("creation_path_prepared"))
                .unwrap()["operation_id"]
                .as_str()
                .unwrap()
                .to_owned()
        } else {
            let acquired = fixture.json(&["acquire", "--repo", &id]);
            let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
            let mut child = fixture.paused_release(handle, "intent", false);
            child.kill().unwrap();
            child.wait().unwrap();
            fs::write(
                checkout.join("tracked"),
                b"protected work after interrupted release",
            )
            .unwrap();
            let operations = fixture.json(&["operation", "list", "--repo", &id]);
            operations["data"]["operations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|o| o["last_checkpoint"].as_str() == Some("release_intended"))
                .unwrap()["operation_id"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let first = fixture.json(&["recover", "apply", "--operation", &original]);
        assert_that!(first["outcome"].as_str(), eq(Some("completed")));
        assert_that!(first["data"]["availability"].as_str(), eq(Some("withheld")));
        let events = fixture.json(&["events", "list"]);
        let bytes = fs::read(fixture.database()).unwrap();
        let repeated = fixture.json(&["recover", "apply", "--operation", &original]);
        assert_that!(
            repeated["context"]["operation_id"].as_str(),
            eq(first["context"]["operation_id"].as_str())
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&events));
        assert_that!(fs::read(fixture.database()).unwrap() == bytes, eq(true));
        assert_that!(
            fixture.json(&[
                "recover",
                "preview",
                "--operation",
                &original,
                "--repo",
                "11111111-1111-4111-8111-111111111111"
            ])["outcome"]
                .as_str(),
            eq(Some("rejected"))
        );
        assert_that!(fs::read(fixture.database()).unwrap() == bytes, eq(true));
    }
}

#[googletest::test]
fn repository_recovery_resumes_its_exact_id_after_each_committed_boundary_without_fetch_or_duplicate_facts()
 {
    use std::os::unix::process::ExitStatusExt;
    for checkpoint in ["intent", "result"] {
        let fixture = Fixture::new();
        let (source, checkout, repository) = fixture.acquisition();
        let mut refresh = fixture.paused_refresh(&repository, "intent");
        refresh.kill().unwrap();
        assert_that!(refresh.wait().unwrap().signal(), eq(Some(9)));
        let listed = fixture.json(&["operation", "list", "--repo", &repository]);
        let original = listed["data"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|operation| operation["state"].as_str() == Some("pending"))
            .unwrap()["operation_id"]
            .as_str()
            .unwrap();
        let reference_before =
            fixture.git_text(&checkout, &["rev-parse", "refs/remotes/origin/main"]);
        let index_path = fixture.git_text(
            &checkout,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        );
        let index_before = fs::read(&index_path).unwrap();
        fixture.git(
            &source,
            &[
                "commit".as_ref(),
                "--allow-empty".as_ref(),
                "-m".as_ref(),
                "remote advanced before explicit reconciliation".as_ref(),
            ],
        );
        assert_that!(
            fixture.git_text(&source, &["rev-parse", "HEAD"]) == reference_before,
            eq(false)
        );

        let mut recovery = fixture.paused_repository_recovery(original, checkpoint);
        recovery.kill().unwrap();
        assert_that!(recovery.wait().unwrap().signal(), eq(Some(9)));
        let operations = fixture.json(&["operation", "list", "--repo", &repository]);
        let recorded: Vec<_> = operations["data"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|operation| operation["target_operation_id"].as_str() == Some(original))
            .collect();
        assert_that!(recorded.len(), eq(1));
        let own_id = recorded[0]["operation_id"].as_str().unwrap();
        assert_that!(own_id == original, eq(false));
        let inspected = fixture.json(&["operation", "inspect", own_id]);
        assert_that!(inspected["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            inspected["data"]["operation"]["target_operation_id"].as_str(),
            eq(Some(original))
        );
        assert_that!(
            inspected["data"]["operation"]["state"].as_str(),
            eq(Some(if checkpoint == "intent" {
                "intended"
            } else {
                "completed"
            }))
        );
        let database_before = fs::read(fixture.database()).unwrap();
        let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
        let locator_before = fs::read(&locator_path).unwrap();
        let preview = fixture.json(&["recover", "preview", "--operation", own_id]);
        assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            preview["context"]["operation_id"].as_str(),
            eq(Some(own_id))
        );
        assert_that!(
            preview["data"]["fresh_fetch_proven"].as_bool(),
            eq(Some(false))
        );
        assert_that!(
            fs::read(fixture.database()).unwrap() == database_before,
            eq(true)
        );
        assert_that!(fs::read(&locator_path).unwrap() == locator_before, eq(true));
        assert_that!(fs::read(&index_path).unwrap() == index_before, eq(true));
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "refs/remotes/origin/main"]),
            eq(&reference_before)
        );

        let resumed = fixture.json(&["recover", "apply", "--operation", own_id]);
        assert_that!(resumed["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            resumed["context"]["operation_id"].as_str(),
            eq(Some(own_id))
        );
        assert_that!(
            resumed["data"]["operation"]["state"].as_str(),
            eq(Some("completed"))
        );
        assert_that!(
            resumed["data"]["fresh_fetch_proven"].as_bool(),
            eq(Some(false))
        );
        assert_that!(resumed["data"]["ownership"].is_null(), eq(true));
        assert_that!(resumed["data"]["availability"].is_null(), eq(true));
        assert_that!(
            fixture.json(&["operation", "inspect", original])["data"]["operation"]["state"]
                .as_str(),
            eq(Some("reconciled"))
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "refs/remotes/origin/main"]),
            eq(&reference_before)
        );
        assert_that!(fs::read(&index_path).unwrap() == index_before, eq(true));

        let history = fixture.json(&["events", "list"]);
        let events = history["data"]["events"].as_array().unwrap();
        let own_facts: Vec<_> = events
            .iter()
            .filter(|event| event["operationid"].as_str() == Some(own_id))
            .collect();
        assert_that!(own_facts.len(), eq(2));
        assert_that!(
            own_facts[0]["type"].as_str(),
            eq(Some(
                "io.lowkeylab.worktreepool.repository.recovery.started.v1"
            ))
        );
        assert_that!(
            own_facts[1]["type"].as_str(),
            eq(Some(
                "io.lowkeylab.worktreepool.repository.recovery.finished.v1"
            ))
        );
        let refresh_facts: Vec<_> = events
            .iter()
            .filter(|event| event["operationid"].as_str() == Some(original))
            .collect();
        assert_that!(refresh_facts.len(), eq(1));
        assert_that!(&own_facts[0]["causationid"], eq(&refresh_facts[0]["id"]));
        assert_that!(&own_facts[1]["causationid"], eq(&own_facts[0]["id"]));
        let completed_database = fs::read(fixture.database()).unwrap();
        let repeated = fixture.json(&["recover", "apply", "--operation", own_id]);
        assert_that!(repeated["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            repeated["context"]["operation_id"].as_str(),
            eq(Some(own_id))
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&history));
        assert_that!(
            fs::read(fixture.database()).unwrap() == completed_database,
            eq(true)
        );
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "refs/remotes/origin/main"]),
            eq(&reference_before)
        );
    }
}

#[googletest::test]
fn recovery_preview_and_abandonment_protect_dirty_hidden_operation_moved_mismatched_and_required_ref_state()
 {
    for condition in [
        "dirty",
        "index",
        "operation",
        "moved",
        "mismatched",
        "required-root",
    ] {
        let fixture = Fixture::new();
        let (source, checkout, id) = fixture.acquisition();
        fixture.git(
            &checkout,
            &["checkout".as_ref(), "--detach".as_ref(), "HEAD".as_ref()],
        );
        let acquired = fixture.json(&["acquire", "--repo", &id]);
        let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
        let acquisition = acquired["context"]["operation_id"].as_str().unwrap();
        let mut actual = checkout.clone();
        match condition {
            "dirty" => fs::write(checkout.join("tracked"), b"protected unfinished work\n").unwrap(),
            "index" => {
                fixture.git(
                    &checkout,
                    &[
                        "update-index".as_ref(),
                        "--skip-worktree".as_ref(),
                        "tracked".as_ref(),
                    ],
                );
                fs::write(checkout.join("tracked"), b"hidden protected work\n").unwrap();
            }
            "operation" => fs::write(
                checkout.join(".git/MERGE_HEAD"),
                fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            )
            .unwrap(),
            "moved" | "mismatched" => {
                actual = fixture.root.path().join("caller-moved-checkout");
                fs::rename(&checkout, &actual).unwrap();
                if condition == "mismatched" {
                    fixture.git(
                        &source,
                        &[
                            "worktree".as_ref(),
                            "add".as_ref(),
                            "--detach".as_ref(),
                            checkout.as_os_str(),
                            "HEAD".as_ref(),
                        ],
                    );
                }
            }
            "required-root" => {
                fixture.git(
                    &checkout,
                    &[
                        "commit".as_ref(),
                        "--allow-empty".as_ref(),
                        "-m".as_ref(),
                        "caller advanced detached work".as_ref(),
                    ],
                );
                let tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
                fixture.git(
                    &checkout,
                    &[
                        "update-ref".as_ref(),
                        format!("refs/worktree-pool/{acquisition}").as_ref(),
                        tip.as_ref(),
                    ],
                );
            }
            _ => unreachable!(),
        }
        let protected = fs::read(actual.join("tracked")).unwrap();
        let index = fs::read(actual.join(".git/index")).unwrap();
        let refs = fixture.git_text(&actual, &["show-ref"]);
        let before = fs::read(fixture.database()).unwrap();
        let preview = fixture.json(&["recover", "preview", "--assignment", handle]);
        assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            preview["data"]["observation"]["safe"].as_bool(),
            eq(Some(false))
        );
        assert_that!(fs::read(fixture.database()).unwrap() == before, eq(true));
        assert_that!(
            fs::read(actual.join(".git/index")).unwrap() == index,
            eq(true)
        );
        let applied = fixture.json(&["recover", "apply", "--assignment", handle, "--abandon"]);
        assert_that!(
            applied["outcome"].as_str(),
            eq(Some(if condition == "required-root" {
                "pending"
            } else {
                "rejected"
            }))
        );
        assert_that!(
            fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"]
                .as_str(),
            eq(Some("active"))
        );
        assert_that!(
            fs::read(actual.join("tracked")).unwrap() == protected,
            eq(true)
        );
        assert_that!(
            fs::read(actual.join(".git/index")).unwrap() == index,
            eq(true)
        );
        assert_that!(fixture.git_text(&actual, &["show-ref"]), eq(&refs));
        if condition != "required-root" {
            assert_that!(fs::read(fixture.database()).unwrap() == before, eq(true));
        }
    }
}

#[googletest::test]
fn pending_catalog_operation_is_inspectable_without_storage_and_only_its_exact_id_can_resume() {
    for checkpoint in ["intent", "store", "published"] {
        let fixture = Fixture::new();
        let mut initialize = fixture.paused_catalog("init", "intent", false);
        initialize.kill().unwrap();
        initialize.wait().unwrap();
        let mut recovery = fixture.paused_catalog("recover-catalog", checkpoint, false);
        recovery.kill().unwrap();
        recovery.wait().unwrap();

        let locator_path = fixture.root.path().join("state/worktree-pool/active.json");
        let locator = fs::read(&locator_path).unwrap();
        let database = fs::read(fixture.database()).ok();
        assert_that!(database.is_none(), eq(checkpoint == "intent"));
        let listed = fixture.json(&["operation", "list"]);
        assert_that!(
            listed["outcome"].as_str(),
            eq(Some(if checkpoint == "published" {
                "completed"
            } else {
                "pending"
            }))
        );
        assert_that!(
            listed["data"]["repository_operations_available"].as_bool(),
            eq(Some(checkpoint == "published"))
        );
        let operations = listed["data"]["operations"].as_array().unwrap();
        assert_that!(operations.len(), eq(1));
        let id = operations[0]["operation_id"].as_str().unwrap();
        assert_that!(operations[0]["scope"].as_str(), eq(Some("catalog")));
        let inspected = fixture.json(&["operation", "inspect", id]);
        assert_that!(inspected["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            inspected["data"]["operation"]["state"].as_str(),
            eq(Some(if checkpoint == "published" {
                "completed"
            } else {
                "pending"
            }))
        );
        assert_that!(
            inspected["data"]["operation"]["checkpoint"].as_str(),
            eq(Some(if checkpoint == "published" {
                "completed"
            } else {
                "intent_recorded"
            }))
        );
        let preview = fixture.json(&["recover", "preview", "--operation", id]);
        assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
        assert_that!(preview["data"]["ownership"].is_null(), eq(true));
        assert_that!(preview["data"]["availability"].is_null(), eq(true));
        assert_that!(
            preview["data"]["catalog_mutations_available"].as_bool(),
            eq(Some(checkpoint == "published"))
        );
        assert_that!(fs::read(&locator_path).unwrap() == locator, eq(true));
        assert_that!(fs::read(fixture.database()).ok() == database, eq(true));

        let conflicted = fixture.json(&[
            "recover",
            "apply",
            "--operation",
            id,
            "--repo",
            "missing-repository",
        ]);
        assert_that!(
            conflicted["reason_code"].as_str(),
            eq(Some("selector_conflict"))
        );
        let unknown = fixture.json(&[
            "recover",
            "apply",
            "--operation",
            "11111111-1111-4111-8111-111111111111",
        ]);
        assert_that!(unknown["outcome"].as_str() == Some("completed"), eq(false));
        assert_that!(fs::read(&locator_path).unwrap() == locator, eq(true));
        assert_that!(fs::read(fixture.database()).ok() == database, eq(true));

        let recovered = fixture.json(&["recover", "apply", "--operation", id]);
        assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
        assert_that!(recovered["context"]["operation_id"].as_str(), eq(Some(id)));
        assert_that!(
            recovered["data"]["operation"]["state"].as_str(),
            eq(Some("completed"))
        );
        assert_that!(
            recovered["data"]["authority"]["phase"].as_str(),
            eq(Some("active"))
        );
        assert_that!(
            recovered["data"]["catalog_mutations_available"].as_bool(),
            eq(Some(true))
        );
        assert_that!(
            fixture.json(&["events", "list"])["data"]["events"]
                .as_array()
                .unwrap()
                .len(),
            eq(1)
        );
    }
}

#[googletest::test]
fn recovery_selects_non_utf8_registered_paths_and_known_historical_results_without_guessing() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let source = fixture.repository();
    let checkout = fixture
        .root
        .path()
        .join(OsString::from_vec(b"recovery-\xff".to_vec()));
    fixture.git(
        fixture.root.path(),
        &["clone".as_ref(), source.as_os_str(), checkout.as_os_str()],
    );
    let registered = fixture.path_json(&["repo", "register"], &checkout);
    let repository = registered["context"]["repository_id"].as_str().unwrap();
    fixture.path_json(&["worktree", "register", "--repo", repository], &checkout);
    let acquired = fixture.json(&["acquire", "--repo", repository]);
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    let original = acquired["context"]["operation_id"].as_str().unwrap();
    let before = fs::read(fixture.database()).unwrap();
    let preview = fixture.path_json(&["recover", "preview", "--worktree"], &checkout);
    assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        preview["context"]["assignment_handle"].as_str(),
        eq(Some(handle))
    );
    let bytes: Vec<u8> =
        serde_json::from_value(preview["data"]["assignment"]["path"]["bytes"].clone()).unwrap();
    assert_that!(bytes.as_slice(), eq(checkout.as_os_str().as_bytes()));
    assert_that!(fs::read(fixture.database()).unwrap() == before, eq(true));
    assert_that!(
        fixture.path_json(&["recover", "apply", "--worktree"], &checkout)["data"]["ownership"]
            .as_str(),
        eq(Some("active"))
    );
    assert_that!(fixture.json(&["recover","apply","--assignment",handle,"--abandon"])["data"]["ownership"].as_str(),eq(Some("released")));
    assert_that!(
        fixture.json(&["recover", "preview", "--operation", original])["data"]["availability"]
            .is_null(),
        eq(true)
    );
    let before = fs::read(fixture.database()).unwrap();
    for args in [
        vec![
            "recover",
            "apply",
            "--assignment",
            handle,
            "--operation",
            original,
        ],
        vec!["recover", "apply", "--operation", original, "--abandon"],
        vec!["recover", "apply", "--catalog", "--assignment", handle],
    ] {
        assert_that!(
            fixture.json(&args)["outcome"].as_str(),
            eq(Some("rejected"))
        );
    }
    assert_that!(fs::read(fixture.database()).unwrap() == before, eq(true));
}

mod relocation_cli_tests;

#[googletest::test]
fn catalog_rebuild_preserves_public_state_and_complete_history() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let before = fixture.json(&["catalog", "info"]);
    let history = fixture.json(&["events", "list"]);
    let result = fixture.run(&["catalog", "rebuild"]);
    assert_that!(result.status.code(), eq(Some(0)));
    let rebuilt: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_that!(rebuilt["command"].as_str(), eq(Some("catalog rebuild")));
    assert_that!(rebuilt["schema_version"].as_u64(), eq(Some(1)));
    assert_that!(
        &rebuilt["data"],
        eq(&catalog_projection_data(&before["data"]))
    );
    assert_that!(fixture.json(&["events", "list"]), eq(&history));
}

// Info adds observations beside the complete projection; rebuild returns that projection only.
fn catalog_projection_data(data: &Value) -> Value {
    let mut projection = data.clone();
    for field in [
        "authority",
        "catalog_mutations_available",
        "worktrees_moved",
        "next_action",
    ] {
        projection.as_object_mut().unwrap().remove(field);
    }
    projection
}

fn damage_derived_catalog(path: &std::path::Path, kind: &str) {
    use redb::{Database, ReadableTable, TableDefinition};
    let database = Database::open(path).unwrap();
    let transaction = database.begin_write().unwrap();
    {
        let mut state = transaction
            .open_table(TableDefinition::<&str, &str>::new("state"))
            .unwrap();
        let mut value: Value =
            serde_json::from_str(state.get("catalog").unwrap().unwrap().value()).unwrap();
        match kind {
            "revision" => value["revision"] = serde_json::json!(999),
            "ownership" => value["assignments"] = serde_json::json!([]),
            "missing" => {
                state.remove("catalog").unwrap();
            }
            "malformed" => {
                state.insert("catalog", "broken derived JSON").unwrap();
            }
            "index" => {
                let mut identities = transaction
                    .open_table(TableDefinition::<&str, u64>::new("event_identities"))
                    .unwrap();
                let keys: Vec<String> = identities
                    .iter()
                    .unwrap()
                    .map(|r| r.unwrap().0.value().to_owned())
                    .collect();
                for key in keys {
                    identities.remove(key.as_str()).unwrap();
                }
            }
            _ => panic!("unknown derived fixture"),
        }
        if matches!(kind, "revision" | "ownership") {
            state
                .insert("catalog", serde_json::to_string(&value).unwrap().as_str())
                .unwrap();
        }
    }
    transaction.commit().unwrap();
}

#[googletest::test]
fn catalog_rebuild_repairs_only_derived_damage_and_preserves_active_ownership() {
    for kind in ["revision", "ownership", "missing", "malformed", "index"] {
        let fixture = Fixture::new();
        let (_, checkout, repository) = fixture.acquisition();
        let acquired = fixture.json(&["acquire", "--repo", &repository]);
        assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
        let before = fixture.json(&["catalog", "info"]);
        let history = fixture.json(&["events", "list"]);
        let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let index = fs::read(checkout.join(".git/index")).unwrap();
        damage_derived_catalog(&fixture.database(), kind);
        let damaged = fs::read(fixture.database()).unwrap();
        assert_that!(
            fixture.run(&["catalog", "check"]).status.code(),
            eq(Some(2))
        );
        assert_that!(fs::read(fixture.database()).unwrap(), eq(&damaged));
        let rebuilt = fixture.run(&["catalog", "rebuild"]);
        assert_that!(rebuilt.status.code(), eq(Some(0)));
        assert_that!(
            fixture.json(&["catalog", "info"])["data"].clone(),
            eq(&before["data"])
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&history));
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&head)
        );
        assert_that!(fs::read(checkout.join(".git/index")).unwrap(), eq(&index));
        assert_that!(
            fixture.run(&["catalog", "rebuild"]).status.code(),
            eq(Some(0))
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&history));
    }
}

#[googletest::test]
fn catalog_rebuild_never_replays_pending_completed_or_withheld_workflow_effects() {
    for scenario in [
        "release",
        "acquire",
        "creation",
        "recovery",
        "recovered",
        "withheld",
        "refresh",
    ] {
        let fixture = Fixture::new();
        let (_, checkout, repository) = if scenario == "creation" {
            fixture.empty_pool()
        } else {
            fixture.acquisition()
        };
        fixture.json(&[
            "pool",
            "configure",
            "--repo",
            &repository,
            "--max-worktrees",
            "1",
        ]);
        match scenario {
            "creation" => {
                let mut child = fixture.paused_acquire(&repository, "creation-path", false);
                child.kill().unwrap();
                child.wait().unwrap();
            }
            "acquire" => {
                let mut child = fixture.paused_acquire(&repository, "checkout-effect", false);
                child.kill().unwrap();
                child.wait().unwrap();
            }
            "refresh" => {
                let mut child = fixture.paused_refresh(&repository, "intent");
                child.kill().unwrap();
                child.wait().unwrap();
            }
            _ => {
                let acquired = fixture.json(&["acquire", "--repo", &repository]);
                let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
                match scenario {
                    "release" => {
                        let mut child = fixture.paused_release(handle, "preserved", false);
                        child.kill().unwrap();
                        child.wait().unwrap();
                    }
                    "recovery" => {
                        let mut child = fixture.paused_recovery(handle, "intent", false);
                        child.kill().unwrap();
                        child.wait().unwrap();
                    }
                    "recovered" => {
                        assert_that!(
                            fixture
                                .run(&["recover", "apply", "--assignment", handle, "--abandon"])
                                .status
                                .code(),
                            eq(Some(0))
                        );
                    }
                    "withheld" => {
                        fixture.json(&["release", handle]);
                        fs::write(checkout.join("tracked"), b"protected dirty work\n").unwrap();
                        assert_that!(
                            fixture
                                .run(&["acquire", "--repo", &repository])
                                .status
                                .success(),
                            eq(false)
                        );
                    }
                    _ => unreachable!(),
                }
            }
        }
        let before = fixture.json(&["catalog", "info"]);
        let history = fixture.json(&["events", "list"]);
        let assignments = fixture.json(&["assignment", "list"]);
        let operations = fixture.json(&["operation", "list"]);
        let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let refs = fixture.git_text(&checkout, &["show-ref"]);
        let index = fs::read(checkout.join(".git/index")).unwrap();
        let tracked = fs::read(checkout.join("tracked")).unwrap();
        fs::write(
            checkout.join("ignored-build-state"),
            b"retained build state",
        )
        .unwrap();
        let locator = fixture.root.path().join("state/worktree-pool/active.json");
        let locator_before = fs::read(&locator).unwrap();
        // Replay succeeds even when registered paths are missing and Git cannot execute.
        let moved = fixture.root.path().join("moved-checkout");
        fs::rename(&checkout, &moved).unwrap();
        let mut command = fixture.command();
        command.env("PATH", "").args(["catalog", "rebuild"]);
        let rebuilt = output(command);
        assert_that!(rebuilt.status.code(), eq(Some(0)));
        assert_that!(rebuilt.stderr.is_empty(), eq(true));
        assert_that!(checkout.exists(), eq(false));
        fs::rename(&moved, &checkout).unwrap();
        assert_that!(
            fixture.json(&["catalog", "info"])["data"].clone(),
            eq(&before["data"])
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&history));
        assert_that!(fixture.json(&["assignment", "list"]), eq(&assignments));
        assert_that!(fixture.json(&["operation", "list"]), eq(&operations));
        assert_that!(fs::read(&locator).unwrap(), eq(&locator_before));
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&head)
        );
        assert_that!(fixture.git_text(&checkout, &["show-ref"]), eq(&refs));
        assert_that!(fs::read(checkout.join(".git/index")).unwrap(), eq(&index));
        assert_that!(fs::read(checkout.join("tracked")).unwrap(), eq(&tracked));
        assert_that!(
            fs::read(checkout.join("ignored-build-state")).unwrap(),
            eq(&b"retained build state".to_vec())
        );
    }
}

#[googletest::test]
fn catalog_rebuild_rejects_corrupt_unsupported_or_foreign_authority_without_writes() {
    use redb::{Database, ReadableTable, TableDefinition};
    for kind in [
        "position",
        "expected_revision",
        "stream",
        "unknown_type",
        "specversion",
        "store_version",
        "projection_version",
        "source",
        "duplicate",
        "empty",
        "foreign_projection",
        "future_projection",
    ] {
        let fixture = Fixture::new();
        let (_, checkout, _) = fixture.acquisition();
        {
            let database = Database::open(fixture.database()).unwrap();
            let transaction = database.begin_write().unwrap();
            if matches!(kind, "foreign_projection" | "future_projection") {
                let mut state = transaction
                    .open_table(TableDefinition::<&str, &str>::new("state"))
                    .unwrap();
                let mut value: Value =
                    serde_json::from_str(state.get("catalog").unwrap().unwrap().value()).unwrap();
                if kind == "foreign_projection" {
                    value["catalog_id"] = serde_json::json!("99999999-9999-4999-8999-999999999999");
                } else {
                    value["projection_version"] = serde_json::json!(2);
                }
                state
                    .insert("catalog", serde_json::to_string(&value).unwrap().as_str())
                    .unwrap();
            } else {
                let mut events = transaction
                    .open_table(TableDefinition::<u64, &str>::new("events"))
                    .unwrap();
                let mut record: Value =
                    serde_json::from_str(events.get(1).unwrap().unwrap().value()).unwrap();
                match kind {
                    "position" => record["position"] = serde_json::json!(2),
                    "expected_revision" => record["expected_revision"] = serde_json::json!(99),
                    "stream" => record["stream_id"] = serde_json::json!("repositories/wrong"),
                    "unknown_type" => {
                        record["event"]["type"] =
                            serde_json::json!("io.lowkeylab.worktreepool.unknown.v9");
                    }
                    "specversion" => record["event"]["specversion"] = serde_json::json!("2.0"),
                    "store_version" => {
                        record["event"]["data"]["store_version"] = serde_json::json!(2);
                    }
                    "projection_version" => {
                        record["event"]["data"]["projection_version"] = serde_json::json!(2);
                    }
                    "source" => {
                        record["event"]["source"] =
                            serde_json::json!("urn:uuid:99999999-9999-4999-8999-999999999999");
                    }
                    "duplicate" => {
                        record["position"] = serde_json::json!(4);
                        events
                            .insert(4, serde_json::to_string(&record).unwrap().as_str())
                            .unwrap();
                    }
                    "empty" => {
                        events.retain(|_, _| false).unwrap();
                    }
                    _ => unreachable!(),
                }
                if !matches!(kind, "duplicate" | "empty") {
                    events
                        .insert(1, serde_json::to_string(&record).unwrap().as_str())
                        .unwrap();
                }
            }
            transaction.commit().unwrap();
        }
        let bytes = fs::read(fixture.database()).unwrap();
        let locator = fixture.root.path().join("state/worktree-pool/active.json");
        let before = fs::read(&locator).unwrap();
        let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let result = fixture.run(&["catalog", "rebuild"]);
        assert_that!(result.status.code(), eq(Some(2)));
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        let reason = match kind {
            "unknown_type" | "specversion" | "store_version" | "projection_version"
            | "future_projection" => "unsupported_version",
            "foreign_projection" => "catalog_conflict",
            _ => "catalog_corrupt",
        };
        assert_that!(value["reason_code"].as_str(), eq(Some(reason)));
        assert_that!(fs::read(fixture.database()).unwrap(), eq(&bytes));
        assert_that!(fs::read(&locator).unwrap(), eq(&before));
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&head)
        );
        assert_that!(fixture.run(&["catalog", "info"]).status.code(), eq(Some(2)));
    }
}

#[googletest::test]
fn interrupted_rebuild_retains_ownership_until_explicit_complete_reconstruction() {
    use std::os::unix::process::ExitStatusExt;
    for checkpoint in ["validated", "before-commit", "committed"] {
        let fixture = Fixture::new();
        let (_, checkout, repository) = fixture.acquisition();
        fixture.json(&["acquire", "--repo", &repository]);
        let before = fixture.json(&["catalog", "info"]);
        let history = fixture.json(&["events", "list"]);
        let head = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let index = fs::read(checkout.join(".git/index")).unwrap();
        damage_derived_catalog(&fixture.database(), "ownership");
        let mut child = fixture.paused_catalog("rebuild", checkpoint, false);
        child.kill().unwrap();
        assert_that!(child.wait().unwrap().signal(), eq(Some(9)));
        let check = fixture.run(&["catalog", "check"]);
        if check.status.success() {
            let checked: Value = serde_json::from_slice(&check.stdout).unwrap();
            assert_that!(&checked["data"], eq(&before["data"]));
        }
        let rebuilt = fixture.run(&["catalog", "rebuild"]);
        if !rebuilt.status.success() {
            assert_that!(
                fixture
                    .run(&["recover", "apply", "--catalog"])
                    .status
                    .code(),
                eq(Some(0))
            );
            assert_that!(
                fixture.run(&["catalog", "rebuild"]).status.code(),
                eq(Some(0))
            );
        }
        let after = fixture.json(&["catalog", "info"]);
        assert_that!(
            catalog_projection_data(&after["data"]),
            eq(&catalog_projection_data(&before["data"]))
        );
        assert_that!(
            after["data"]["catalog_mutations_available"].as_bool(),
            eq(Some(true))
        );
        for field in [
            "catalog_id",
            "catalog_path",
            "phase",
            "store_state",
            "revision",
            "relocation",
            "relocation_history",
        ] {
            assert_that!(
                &after["data"]["authority"][field],
                eq(&before["data"]["authority"][field])
            );
        }
        let prior_facts = before["data"]["authority"]["recovery_history"]
            .as_array()
            .unwrap();
        let facts = after["data"]["authority"]["recovery_history"]
            .as_array()
            .unwrap();
        assert_that!(&facts[..prior_facts.len()], eq(prior_facts));
        if rebuilt.status.success() {
            assert_that!(facts, eq(prior_facts));
        } else {
            assert_that!(facts.len(), eq(prior_facts.len() + 2));
            let added = &facts[prior_facts.len()..];
            assert_that!(added[0]["kind"].as_str(), eq(Some("storage_repair")));
            assert_that!(added[0]["checkpoint"].as_str(), eq(Some("intent_recorded")));
            assert_that!(added[1]["checkpoint"].as_str(), eq(Some("completed")));
            assert_that!(&added[0]["operation_id"], eq(&added[1]["operation_id"]));
        }
        assert_that!(fixture.json(&["events", "list"]), eq(&history));
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]),
            eq(&head)
        );
        assert_that!(fs::read(checkout.join(".git/index")).unwrap(), eq(&index));
    }
}

fn wait_for_maintenance_waiter(fixture: &Fixture, child: &mut Child) {
    use std::os::unix::fs::MetadataExt;
    let inode = fs::metadata(
        fixture
            .root
            .path()
            .join("state/worktree-pool/maintenance.lock"),
    )
    .unwrap()
    .ino();
    let pid = child.id().to_string();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let locks = fs::read_to_string("/proc/locks").unwrap();
        if locks.lines().any(|line| {
            line.contains("->")
                && line.split_whitespace().any(|part| part == pid)
                && line.contains(&format!(":{inode} "))
        }) {
            return;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("command completed before its maintenance barrier: {status}");
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("maintenance lock waiter watchdog");
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[googletest::test]
fn rebuild_excludes_whole_live_commands_and_serializes_later_mutations_without_lost_facts() {
    use std::io::Write;
    let fixture = Fixture::new();
    let (_, _, repository) = fixture.acquisition();
    let mut acquisition = fixture.paused_acquire(&repository, "checkout-effect", false);
    let pending = fixture.json(&["catalog", "info"]);
    let history = fixture.json(&["events", "list"]);
    let mut command = fixture.command();
    command.args(["catalog", "rebuild"]);
    let mut rebuilding = command.spawn().unwrap();
    wait_for_maintenance_waiter(&fixture, &mut rebuilding);
    acquisition.kill().unwrap();
    acquisition.wait().unwrap();
    assert_that!(wait(rebuilding).status.code(), eq(Some(0)));
    assert_that!(
        fixture.json(&["catalog", "info"])["data"].clone(),
        eq(&pending["data"])
    );
    assert_that!(fixture.json(&["events", "list"]), eq(&history));
    let mut rebuilding = fixture.paused_catalog("rebuild", "validated", true);
    let mut command = fixture.command();
    command.args([
        "pool",
        "configure",
        "--repo",
        &repository,
        "--max-worktrees",
        "2",
    ]);
    let mut configuring = command.spawn().unwrap();
    wait_for_maintenance_waiter(&fixture, &mut configuring);
    rebuilding
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    assert_that!(wait(rebuilding).status.code(), eq(Some(0)));
    assert_that!(wait(configuring).status.code(), eq(Some(0)));
    let after = fixture.json(&["catalog", "info"]);
    assert_that!(
        after["data"]["repositories"][0]["capacity"].as_u64(),
        eq(Some(2))
    );
    assert_that!(
        after["data"]["assignments"].clone(),
        eq(&pending["data"]["assignments"])
    );
    assert_that!(
        after["data"]["revision"].as_u64(),
        eq(Some(pending["data"]["revision"].as_u64().unwrap() + 1))
    );
}

#[googletest::test]
fn lost_rebuild_stdout_preserves_the_committed_projection_and_complete_history() {
    let fixture = Fixture::new();
    let (_, _, repository) = fixture.acquisition();
    fixture.json(&["acquire", "--repo", &repository]);
    let before = fixture.json(&["catalog", "info"]);
    let history = fixture.json(&["events", "list"]);
    damage_derived_catalog(&fixture.database(), "ownership");
    let mut command = fixture.command();
    command.args(["catalog", "rebuild"]).stdout(
        fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .unwrap(),
    );
    assert_that!(output(command).status.code(), eq(Some(4)));
    assert_that!(
        fixture.json(&["catalog", "info"])["data"].clone(),
        eq(&before["data"])
    );
    assert_that!(fixture.json(&["events", "list"]), eq(&history));
    assert_that!(
        fixture.run(&["catalog", "rebuild"]).status.code(),
        eq(Some(0))
    );
    assert_that!(fixture.json(&["events", "list"]), eq(&history));
}

#[googletest::test]
fn catalog_rebuild_rejects_repository_scope_instead_of_ignoring_the_selector() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let database = fs::read(fixture.database()).unwrap();
    let result = fixture.run(&["catalog", "rebuild", "--repo", "unregistered-repository"]);
    assert_that!(result.status.code(), eq(Some(2)));
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_that!(value["reason_code"].as_str(), eq(Some("selector_conflict")));
    assert_that!(fs::read(fixture.database()).unwrap(), eq(&database));
}

#[googletest::test]
fn retirement_requires_human_removal_and_reconciliation_then_reuses_only_registered_capacity() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let repository = fixture.repository();
    let remote = fixture.root.path().join("historical-remote.git");
    fixture.git(
        &repository,
        &[
            "clone".as_ref(),
            "--bare".as_ref(),
            repository.as_os_str(),
            remote.as_os_str(),
        ],
    );
    fixture.git(
        &repository,
        &[
            "remote".as_ref(),
            "add".as_ref(),
            "origin".as_ref(),
            remote.as_os_str(),
        ],
    );
    let registered = fixture.path_json(&["repo", "register"], &repository);
    let repo = registered["context"]["repository_id"].as_str().unwrap();
    fixture.json(&["pool", "configure", "--repo", repo, "--max-worktrees", "1"]);
    let path = fixture.root.path().join("retiring");
    fixture.git(
        &repository,
        &[
            "worktree".as_ref(),
            "add".as_ref(),
            "--detach".as_ref(),
            path.as_os_str(),
        ],
    );
    let registration = fixture.path_json(&["worktree", "register"], &path);
    let worktree = registration["context"]["worktree_id"].as_str().unwrap();
    let reconciled = fixture.json(&["recover", "apply", "--worktree", worktree]);
    assert_that!(reconciled["outcome"].as_str(), eq(Some("completed")));
    let proof = reconciled["context"]["operation_id"].as_str().unwrap();
    let preservation = reconciled["data"]["preservation_reference"]
        .as_str()
        .unwrap();
    let tip = fixture.git_stdout(&repository, &["rev-parse", preservation]);
    fixture.git(
        &repository,
        &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
    );
    let before_retirement = fixture.json(&["catalog", "info"]);
    let result = fixture.json(&[
        "worktree",
        "retire",
        worktree,
        "--reconciliation",
        proof,
        "--removed",
    ]);
    assert_that!(result["outcome"].as_str(), eq(Some("completed")));
    assert_that!(result["data"]["retired"].as_bool(), eq(Some(true)));
    let after_retirement = fixture.json(&["catalog", "info"]);
    assert_that!(
        after_retirement["data"]["catalog_stream_revision"].as_u64(),
        eq(Some(
            before_retirement["data"]["catalog_stream_revision"]
                .as_u64()
                .unwrap()
                + 1
        ))
    );
    assert_that!(
        after_retirement["data"]["repositories"][0]["revision"].as_u64(),
        eq(Some(
            before_retirement["data"]["repositories"][0]["revision"]
                .as_u64()
                .unwrap()
                + 1
        ))
    );
    let history = fixture.json(&["events", "list"]);
    let events = history["data"]["events"].as_array().unwrap();
    let intent = &events[events.len() - 2];
    let finished = &events[events.len() - 1];
    assert_that!(
        intent["type"].as_str(),
        eq(Some(
            "io.lowkeylab.worktreepool.worktree.retirement.started.v1"
        ))
    );
    assert_that!(
        finished["type"].as_str(),
        eq(Some(
            "io.lowkeylab.worktreepool.worktree.retirement.finished.v1"
        ))
    );
    assert_that!(&finished["causationid"], eq(&intent["id"]));
    assert_that!(
        fixture.json(&["repo", "inspect", "--repo", repo])["data"]["registered_count"].as_u64(),
        eq(Some(0))
    );
    assert_that!(
        fixture.json(&["worktree", "inspect", worktree])["data"]["worktree"]["registration_state"]
            .as_str(),
        eq(Some("retired"))
    );
    assert_that!(
        fixture.git_stdout(&repository, &["rev-parse", preservation]),
        eq(&tip)
    );
    assert_that!(
        fs::read(repository.join("tracked")).unwrap().as_slice(),
        eq(b"protected checkout\n".as_slice())
    );
    let replacement = path;
    fixture.git(
        &repository,
        &[
            "worktree".as_ref(),
            "add".as_ref(),
            "--detach".as_ref(),
            replacement.as_os_str(),
        ],
    );
    assert_that!(
        fixture.path_json(&["worktree", "register"], &replacement)["outcome"].as_str(),
        eq(Some("completed"))
    );
    let newer = fixture.json(&["acquire", "--repo", repo]);
    assert_that!(newer["outcome"].as_str(), eq(Some("completed")));
    let newer_handle = newer["context"]["assignment_handle"].as_str().unwrap();
    fs::write(
        replacement.join("tracked"),
        b"new owner protected dirty bytes\n",
    )
    .unwrap();
    let events = fixture.json(&["events", "list"]);
    assert_that!(
        fixture.json(&[
            "worktree",
            "retire",
            worktree,
            "--reconciliation",
            proof,
            "--removed"
        ])["data"]["already_retired"]
            .as_bool(),
        eq(Some(true))
    );
    assert_that!(fixture.json(&["events", "list"]), eq(&events));
    assert_that!(
        fixture.json(&["repo", "inspect", "--repo", repo])["data"]["registered_count"].as_u64(),
        eq(Some(1))
    );
    let old = fixture.json(&["worktree", "inspect", worktree]);
    assert_that!(
        old["data"]["worktree"]["resources"]["worktree"]["bytes"].is_null(),
        eq(true)
    );
    assert_that!(
        old["data"]["worktree"]["resources"]["worktree"]["status"].as_str(),
        eq(Some("retired_path_unattributed"))
    );
    let events = fixture.json(&["events", "list"]);
    for action in ["preview", "apply"] {
        let historical = fixture.json(&["recover", action, "--operation", proof]);
        assert_that!(historical["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            historical["context"]["operation_id"].as_str(),
            eq(Some(proof))
        );
        assert_that!(
            historical["data"]["availability"].as_str(),
            eq(Some("retired"))
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&events));
        assert_that!(
            fixture.json(&["assignment", "inspect", newer_handle])["data"]["assignment"]["state"]
                .as_str(),
            eq(Some("active"))
        );
        assert_that!(
            fs::read(replacement.join("tracked")).unwrap().as_slice(),
            eq(b"new owner protected dirty bytes\n".as_slice())
        );
    }
}

#[googletest::test]
fn retirement_rejects_lost_attached_branch_protection_after_safe_reconciliation() {
    for changed in [false, true] {
        let fixture = Fixture::new();
        fixture.json(&["catalog", "init"]);
        let repository = fixture.repository();
        fixture.path_json(&["repo", "register"], &repository);
        let path = fixture.root.path().join("caller-checkout");
        fixture.git(
            &repository,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "-b".as_ref(),
                "caller".as_ref(),
                path.as_os_str(),
            ],
        );
        let registered = fixture.path_json(&["worktree", "register"], &path);
        let worktree = registered["context"]["worktree_id"].as_str().unwrap();
        let proof = fixture.json(&["recover", "apply", "--worktree", worktree]);
        let proof_id = proof["context"]["operation_id"].as_str().unwrap();
        fixture.git(
            &repository,
            &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
        );
        if changed {
            fs::write(repository.join("tracked"), b"new branch tip\n").unwrap();
            fixture.git(
                &repository,
                &["commit".as_ref(), "-am".as_ref(), "advance".as_ref()],
            );
            fixture.git(
                &repository,
                &[
                    "update-ref".as_ref(),
                    "refs/heads/caller".as_ref(),
                    "HEAD".as_ref(),
                ],
            );
        } else {
            fixture.git(
                &repository,
                &[
                    "update-ref".as_ref(),
                    "-d".as_ref(),
                    "refs/heads/caller".as_ref(),
                ],
            );
        }
        let events = fixture.json(&["events", "list"]);
        let result = fixture.json(&[
            "worktree",
            "retire",
            worktree,
            "--reconciliation",
            proof_id,
            "--removed",
        ]);
        assert_that!(result["outcome"].as_str(), eq(Some("rejected")));
        assert_that!(fixture.json(&["events", "list"]), eq(&events));
        assert_that!(fixture.json(&["worktree", "inspect", worktree])["data"]["worktree"]["registration_state"].as_str(), eq(Some("missing")));
    }
}

#[googletest::test]
fn resource_inspection_measures_private_bytes_once_and_explains_shared_unknown_state_readonly() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let repository = fixture.repository();
    let registered = fixture.path_json(&["repo", "register"], &repository);
    let repo = registered["context"]["repository_id"].as_str().unwrap();
    let path = fixture.root.path().join("measured");
    fixture.git(
        &repository,
        &[
            "worktree".as_ref(),
            "add".as_ref(),
            "--detach".as_ref(),
            path.as_os_str(),
        ],
    );
    let enrolled = fixture.path_json(&["worktree", "register"], &path);
    let worktree = enrolled["context"]["worktree_id"].as_str().unwrap();
    fs::create_dir(path.join("cache")).unwrap();
    fs::write(path.join("cache/first"), b"1234567").unwrap();
    fs::hard_link(path.join("cache/first"), path.join("cache/second")).unwrap();
    let external = fixture.root.path().join("external-state");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("sentinel"), b"unattributed shared state").unwrap();
    std::os::unix::fs::symlink(&external, path.join("bazel-out")).unwrap();
    let database = fs::read(fixture.database()).unwrap();
    let observed = fixture.json(&["worktree", "inspect", worktree]);
    assert_that!(
        observed["data"]["worktree"]["resources"]["worktree"]["bytes"].as_u64(),
        eq(Some(26))
    );
    assert_that!(
        observed["data"]["worktree"]["resources"]["known_external_build_state"]["bytes"].is_null(),
        eq(true)
    );
    assert_that!(
        observed["data"]["worktree"]["resources"]["unknown"]
            .as_array()
            .is_some_and(|v| !v.is_empty()),
        eq(true)
    );
    let aggregate = fixture.json(&["repo", "inspect", "--repo", repo]);
    assert_that!(
        aggregate["data"]["resources"]["worktree_bytes"].as_u64(),
        eq(Some(26))
    );
    assert_that!(
        aggregate["data"]["resources"]["shared"]["bytes"]
            .as_u64()
            .is_some_and(|v| v > 0),
        eq(true)
    );
    assert_that!(
        aggregate["data"]["resources"]["capacity_semantics"].as_str(),
        eq(Some("registered_count_not_byte_limit"))
    );
    assert_that!(fs::read(fixture.database()).unwrap(), eq(&database));
    assert_that!(
        fs::read(external.join("sentinel")).unwrap().as_slice(),
        eq(b"unattributed shared state".as_slice())
    );
    fs::remove_dir_all(&path).unwrap();
    let missing = fixture.json(&["worktree", "inspect", worktree]);
    assert_that!(
        missing["data"]["worktree"]["resources"]["worktree"]["bytes"].is_null(),
        eq(true)
    );
    assert_that!(
        missing["data"]["worktree"]["resources"]["worktree"]["status"].as_str(),
        eq(Some("missing"))
    );
}

#[googletest::test]
fn retirement_process_death_keeps_capacity_until_commit_and_exact_operation_resume_is_idempotent() {
    for checkpoint in ["intent", "result"] {
        let fixture = Fixture::new();
        fixture.json(&["catalog", "init"]);
        let repository = fixture.repository();
        let registration = fixture.path_json(&["repo", "register"], &repository);
        let repo = registration["context"]["repository_id"].as_str().unwrap();
        let path = fixture.root.path().join("crash-retire");
        fixture.git(
            &repository,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "--detach".as_ref(),
                path.as_os_str(),
            ],
        );
        let registered = fixture.path_json(&["worktree", "register"], &path);
        let worktree = registered["context"]["worktree_id"].as_str().unwrap();
        let reconciliation = fixture.json(&["recover", "apply", "--worktree", worktree]);
        let proof = reconciliation["context"]["operation_id"].as_str().unwrap();
        let reference = reconciliation["data"]["preservation_reference"]
            .as_str()
            .unwrap();
        let tip = fixture.git_stdout(&repository, &["rev-parse", reference]);
        fixture.git(
            &repository,
            &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
        );
        let mut child = fixture.paused_recovery_checkpoint(
            "retire",
            checkpoint,
            false,
            Some(&format!("{worktree}:{proof}")),
        );
        child.kill().unwrap();
        child.wait().unwrap();
        let count = fixture.json(&["repo", "inspect", "--repo", repo]);
        assert_that!(
            count["data"]["registered_count"].as_u64(),
            eq(Some(u64::from(checkpoint == "intent")))
        );
        if checkpoint == "intent" {
            assert_that!(
                fixture.json(&["acquire", "--repo", repo])["outcome"].as_str(),
                eq(Some("pending"))
            );
            let second = fixture.root.path().join("independent-repository");
            fixture.git(
                fixture.root.path(),
                &["clone".as_ref(), repository.as_os_str(), second.as_os_str()],
            );
            let registered = fixture.path_json(&["repo", "register"], &second);
            let independent = registered["context"]["repository_id"].as_str().unwrap();
            assert_that!(
                fixture.json(&["acquire", "--repo", independent])["outcome"].as_str(),
                eq(Some("completed"))
            );
        }
        let operations = fixture.json(&["operation", "list"]);
        let operation = operations["data"]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["reconciliation_id"] == proof)
            .unwrap();
        let operation_id = operation["operation_id"].as_str().unwrap();
        let database = fs::read(fixture.database()).unwrap();
        let preview = fixture.json(&["recover", "preview", "--operation", operation_id]);
        assert_that!(preview["outcome"].as_str(), eq(Some("completed")));
        assert_that!(fs::read(fixture.database()).unwrap(), eq(&database));
        let result = fixture.json(&["recover", "apply", "--operation", operation_id]);
        assert_that!(result["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            result["context"]["operation_id"].as_str(),
            eq(Some(operation_id))
        );
        let events = fixture.json(&["events", "list"]);
        assert_that!(fixture.json(&["recover", "apply", "--operation", operation_id])["data"]["already_retired"].as_bool(), eq(Some(true)));
        assert_that!(fixture.json(&["events", "list"]), eq(&events));
        assert_that!(
            fixture.json(&["repo", "inspect", "--repo", repo])["data"]["registered_count"].as_u64(),
            eq(Some(0))
        );
        assert_that!(
            fixture.git_stdout(&repository, &["rev-parse", reference]),
            eq(&tip)
        );
        assert_that!(
            fixture.json(&[
                "recover",
                "apply",
                "--operation",
                "00000000-0000-4000-8000-000000000001"
            ])["outcome"]
                .as_str(),
            eq(Some("rejected"))
        );
    }
}

#[googletest::test]
fn retirement_rejects_missing_paths_below_substituted_symlink_ancestors() {
    let fixture = Fixture::new();
    fixture.json(&["catalog", "init"]);
    let repository = fixture.repository();
    fixture.path_json(&["repo", "register"], &repository);
    let parent = fixture.root.path().join("original-parent");
    fs::create_dir(&parent).unwrap();
    let path = parent.join("retiring");
    fixture.git(
        &repository,
        &[
            "worktree".as_ref(),
            "add".as_ref(),
            "--detach".as_ref(),
            path.as_os_str(),
        ],
    );
    let registered = fixture.path_json(&["worktree", "register"], &path);
    let worktree = registered["context"]["worktree_id"].as_str().unwrap();
    let reconciled = fixture.json(&["recover", "apply", "--worktree", worktree]);
    let proof = reconciled["context"]["operation_id"].as_str().unwrap();
    fixture.git(
        &repository,
        &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
    );
    let moved = fixture.root.path().join("moved-parent");
    fs::rename(&parent, &moved).unwrap();
    fs::write(moved.join("sentinel"), b"protected parent").unwrap();
    std::os::unix::fs::symlink(&moved, &parent).unwrap();
    let events = fixture.json(&["events", "list"]);
    let result = fixture.json(&[
        "worktree",
        "retire",
        worktree,
        "--reconciliation",
        proof,
        "--removed",
    ]);
    assert_that!(result["outcome"].as_str(), eq(Some("rejected")));
    assert_that!(fixture.json(&["events", "list"]), eq(&events));
    assert_that!(
        fs::read(moved.join("sentinel")).unwrap().as_slice(),
        eq(b"protected parent".as_slice())
    );
}

impl Fixture {
    fn retirement_candidate(&self) -> (PathBuf, PathBuf, String, String) {
        self.json(&["catalog", "init"]);
        let (_, _, repository) = self.remote_checkout();
        let registered = self.path_json(&["repo", "register"], &repository);
        let repo = registered["context"]["repository_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let path = self.root.path().join("retirement-candidate");
        self.git(
            &repository,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "--detach".as_ref(),
                path.as_os_str(),
            ],
        );
        let enrolled = self.path_json(&["worktree", "register"], &path);
        let worktree = enrolled["context"]["worktree_id"]
            .as_str()
            .unwrap()
            .to_owned();
        (repository, path, repo, worktree)
    }
}

#[googletest::test]
fn retirement_refusals_preserve_present_ignored_moved_active_missing_and_stale_generations() {
    for scenario in [
        "present-ignored",
        "moved",
        "active",
        "missing-active",
        "stale-generation",
        "missing-withheld",
        "no-confirmation",
    ] {
        let fixture = Fixture::new();
        let (repository, path, repo, worktree) = fixture.retirement_candidate();
        let proof = fixture.json(&["recover", "apply", "--worktree", &worktree]);
        let proof_id = proof["context"]["operation_id"].as_str().unwrap();
        let mut marker = None;
        match scenario {
            "present-ignored" => {
                fs::write(repository.join(".git/info/exclude"), b"cache/\n").unwrap();
                fs::create_dir(path.join("cache")).unwrap();
                fs::write(path.join("cache/sentinel"), b"ignored bytes retained").unwrap();
                marker = Some(path.join("cache/sentinel"));
            }
            "moved" => {
                let moved = fixture.root.path().join("human-moved");
                fixture.git(
                    &repository,
                    &[
                        "worktree".as_ref(),
                        "move".as_ref(),
                        path.as_os_str(),
                        moved.as_os_str(),
                    ],
                );
                marker = Some(moved.join("tracked"));
            }
            "active" | "missing-active" | "stale-generation" => {
                let assigned = fixture.json(&["acquire", "--repo", &repo]);
                assert_that!(assigned["outcome"].as_str(), eq(Some("completed")));
                if scenario == "stale-generation" {
                    let handle = assigned["context"]["assignment_handle"].as_str().unwrap();
                    assert_that!(
                        fixture.json(&["release", handle])["outcome"].as_str(),
                        eq(Some("completed"))
                    );
                }
                if scenario != "active" {
                    fixture.git(
                        &repository,
                        &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
                    );
                }
            }
            _ => {
                fixture.git(
                    &repository,
                    &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
                );
                if scenario == "missing-withheld" {
                    let recovery = fixture.json(&["recover", "apply", "--worktree", &worktree]);
                    assert_that!(
                        recovery["data"]["availability"].as_str(),
                        eq(Some("withheld"))
                    );
                }
            }
        }
        let protected = marker.as_ref().map(|p| fs::read(p).unwrap());
        let before = fixture.json(&["worktree", "inspect", &worktree]);
        let events = fixture.json(&["events", "list"]);
        let mut args = vec![
            "worktree",
            "retire",
            &worktree,
            "--reconciliation",
            proof_id,
        ];
        if scenario != "no-confirmation" {
            args.push("--removed");
        }
        let result = fixture.json(&args);
        assert_that!(result["outcome"].as_str(), eq(Some("rejected")));
        assert_that!(fixture.json(&["events", "list"]), eq(&events));
        let after = fixture.json(&["worktree", "inspect", &worktree]);
        assert_that!(
            &after["data"]["worktree"]["ownership"],
            eq(&before["data"]["worktree"]["ownership"])
        );
        assert_that!(
            &after["data"]["worktree"]["assignment_handle"],
            eq(&before["data"]["worktree"]["assignment_handle"])
        );
        assert_that!(
            fixture.json(&["repo", "inspect", "--repo", &repo])["data"]["registered_count"]
                .as_u64(),
            eq(Some(1))
        );
        if let (Some(marker), Some(protected)) = (marker, protected) {
            assert_that!(fs::read(marker).unwrap(), eq(&protected));
        }
        assert_that!(
            fs::read(repository.join("tracked")).unwrap().as_slice(),
            eq(b"protected checkout\n".as_slice())
        );
    }
}

#[googletest::test]
fn retirement_refuses_missing_conflicting_or_symbolic_detached_preservation_without_rewriting_it() {
    for scenario in ["missing", "conflicting", "symbolic"] {
        let fixture = Fixture::new();
        let (repository, path, repo, worktree) = fixture.retirement_candidate();
        let proof = fixture.json(&["recover", "apply", "--worktree", &worktree]);
        let proof_id = proof["context"]["operation_id"].as_str().unwrap();
        let reference = proof["data"]["preservation_reference"].as_str().unwrap();
        fixture.git(
            &repository,
            &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
        );
        fixture.git(
            &repository,
            &["update-ref".as_ref(), "-d".as_ref(), reference.as_ref()],
        );
        if scenario == "symbolic" {
            fixture.git(
                &repository,
                &[
                    "symbolic-ref".as_ref(),
                    reference.as_ref(),
                    "refs/heads/main".as_ref(),
                ],
            );
        } else if scenario == "conflicting" {
            fs::write(repository.join("tracked"), b"unrelated new tip\n").unwrap();
            fixture.git(
                &repository,
                &["commit".as_ref(), "-am".as_ref(), "advance".as_ref()],
            );
            fixture.git(
                &repository,
                &["update-ref".as_ref(), reference.as_ref(), "HEAD".as_ref()],
            );
        }
        let refs = fixture.git_stdout(
            &repository,
            &[
                "for-each-ref",
                "--format=%(refname) %(objectname) %(symref)",
            ],
        );
        let events = fixture.json(&["events", "list"]);
        assert_that!(
            fixture.json(&[
                "worktree",
                "retire",
                &worktree,
                "--reconciliation",
                proof_id,
                "--removed"
            ])["outcome"]
                .as_str(),
            eq(Some("rejected"))
        );
        assert_that!(
            fixture.git_stdout(
                &repository,
                &[
                    "for-each-ref",
                    "--format=%(refname) %(objectname) %(symref)"
                ]
            ),
            eq(&refs)
        );
        assert_that!(fixture.json(&["events", "list"]), eq(&events));
        assert_that!(
            fixture.json(&["repo", "inspect", "--repo", &repo])["data"]["registered_count"]
                .as_u64(),
            eq(Some(1))
        );
    }
}

#[googletest::test]
fn completed_partial_creation_reconciliation_does_not_authorize_retirement_or_release_capacity() {
    let fixture = Fixture::new();
    let (_, _, repo) = fixture.empty_pool();
    fixture.json(&["pool", "configure", "--repo", &repo, "--max-worktrees", "1"]);
    let mut child = fixture.paused_acquire(&repo, "creation-intent", false);
    child.kill().unwrap();
    child.wait().unwrap();
    let listed = fixture.json(&["worktree", "list"]);
    let w = &listed["data"]["worktrees"][0];
    let creation = w["creation"]["operation_id"].as_str().unwrap();
    let worktree = w["worktree_id"].as_str().unwrap();
    let handle = w["assignment_handle"].as_str().unwrap();
    let recovered = fixture.json(&["recover", "apply", "--operation", creation]);
    let proof = recovered["context"]["operation_id"].as_str().unwrap();
    let events = fixture.json(&["events", "list"]);
    assert_that!(recovered["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        recovered["data"]["ownership"].as_str(),
        eq(Some("preparing"))
    );
    assert_that!(
        fixture.json(&[
            "worktree",
            "retire",
            worktree,
            "--reconciliation",
            proof,
            "--removed"
        ])["outcome"]
            .as_str(),
        eq(Some("rejected"))
    );
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("preparing"))
    );
    assert_that!(fixture.json(&["events", "list"]), eq(&events));
    assert_that!(
        fixture.json(&["repo", "inspect", &repo])["data"]["registered_count"].as_u64(),
        eq(Some(1))
    );
}

#[googletest::test]
fn retirement_after_latest_release_preserves_caller_tip_and_unrelated_active_assignment() {
    for attached in [false, true] {
        let fixture = Fixture::new();
        let (repository, path, repo, worktree) = fixture.retirement_candidate();
        let acquired = fixture.json(&["acquire", "--repo", &repo]);
        let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
        if attached {
            fixture.git(
                &path,
                &["checkout".as_ref(), "-b".as_ref(), "caller".as_ref()],
            );
        }
        fs::write(path.join("tracked"), b"committed caller work\n").unwrap();
        fixture.git(
            &path,
            &["commit".as_ref(), "-am".as_ref(), "caller work".as_ref()],
        );
        let tip = fixture.git_stdout(&path, &["rev-parse", "HEAD"]);
        let release = fixture.json(&["release", handle]);
        assert_that!(release["outcome"].as_str(), eq(Some("completed")));
        let proof = release["context"]["operation_id"].as_str().unwrap();
        let reference = if attached {
            "refs/heads/caller"
        } else {
            release["data"]["operation"]["preservation_reference"]
                .as_str()
                .unwrap()
        };
        let other = fixture.root.path().join("unrelated-active");
        fixture.git(
            &repository,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "--detach".as_ref(),
                other.as_os_str(),
            ],
        );
        fixture.path_json(&["worktree", "register"], &other);
        let unrelated = fixture.json(&["acquire", "--repo", &repo]);
        let other_handle = unrelated["context"]["assignment_handle"].as_str().unwrap();
        assert_that!(unrelated["outcome"].as_str(), eq(Some("completed")));
        fixture.git(
            &repository,
            &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
        );
        assert_that!(
            fixture.json(&[
                "worktree",
                "retire",
                &worktree,
                "--reconciliation",
                proof,
                "--removed"
            ])["outcome"]
                .as_str(),
            eq(Some("completed"))
        );
        assert_that!(
            fixture.git_stdout(&repository, &["rev-parse", reference]),
            eq(&tip)
        );
        assert_that!(
            fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"]
                .as_str(),
            eq(Some("released"))
        );
        assert_that!(
            fixture.json(&["assignment", "inspect", other_handle])["data"]["assignment"]["state"]
                .as_str(),
            eq(Some("active"))
        );
        assert_that!(
            fs::read(other.join("tracked")).unwrap().as_slice(),
            eq(b"protected checkout\n".as_slice())
        );
        assert_that!(
            fixture.json(&["repo", "inspect", "--repo", &repo])["data"]["registered_count"]
                .as_u64(),
            eq(Some(1))
        );
        fixture.git(
            &repository,
            &[
                "worktree".as_ref(),
                "add".as_ref(),
                "--detach".as_ref(),
                path.as_os_str(),
            ],
        );
        fixture.path_json(&["worktree", "register"], &path);
        let newer = fixture.json(&["acquire", "--repo", &repo]);
        assert_that!(newer["outcome"].as_str(), eq(Some("completed")));
        let newer_handle = newer["context"]["assignment_handle"].as_str().unwrap();
        fs::write(path.join("tracked"), b"new owner dirty sentinel\n").unwrap();
        let events = fixture.json(&["events", "list"]);
        for action in ["preview", "apply"] {
            for operation in [acquired["context"]["operation_id"].as_str().unwrap(), proof] {
                let historical = fixture.json(&["recover", action, "--operation", operation]);
                assert_that!(historical["outcome"].as_str(), eq(Some("completed")));
                assert_that!(
                    historical["data"]["observation"]["status"].as_str(),
                    eq(Some("historical"))
                );
                assert_that!(fixture.json(&["events", "list"]), eq(&events));
            }
            assert_that!(
                fixture.json(&["recover", action, "--assignment", handle])["outcome"].as_str(),
                eq(Some("completed"))
            );
            assert_that!(fixture.json(&["events", "list"]), eq(&events));
            assert_that!(fixture.json(&["assignment", "inspect", newer_handle])["data"]["assignment"]["state"].as_str(), eq(Some("active")));
            assert_that!(
                fixture.json(&["repo", "inspect", "--repo", &repo])["data"]["registered_count"]
                    .as_u64(),
                eq(Some(2))
            );
            assert_that!(
                fs::read(path.join("tracked")).unwrap().as_slice(),
                eq(b"new owner dirty sentinel\n".as_slice())
            );
        }
    }
}

#[googletest::test]
fn rebuild_preserves_intended_retirement_and_completed_tombstones_with_newer_owners() {
    use redb::{ReadableDatabase, ReadableTable, TableDefinition};
    for checkpoint in ["intent", "result"] {
        let fixture = Fixture::new();
        let (repository, path, repo, worktree) = fixture.retirement_candidate();
        let reconciliation = fixture.json(&["recover", "apply", "--worktree", &worktree]);
        let proof = reconciliation["context"]["operation_id"].as_str().unwrap();
        fixture.git(
            &repository,
            &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
        );
        let mut child = fixture.paused_recovery_checkpoint(
            "retire",
            checkpoint,
            false,
            Some(&format!("{worktree}:{proof}")),
        );
        child.kill().unwrap();
        child.wait().unwrap();
        let owner = if checkpoint == "result" {
            fixture.git(
                &repository,
                &[
                    "worktree".as_ref(),
                    "add".as_ref(),
                    "--detach".as_ref(),
                    path.as_os_str(),
                ],
            );
            fixture.path_json(&["worktree", "register"], &path);
            let acquired = fixture.json(&["acquire", "--repo", &repo]);
            assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
            fs::write(path.join("tracked"), b"newer owner survives replay\n").unwrap();
            Some(
                acquired["context"]["assignment_handle"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            )
        } else {
            None
        };
        let before = fixture.json(&["catalog", "info"]);
        let history = fixture.json(&["events", "list"]);
        // Inspect actual persisted stream metadata, retaining public envelopes unchanged.
        {
            let database = redb::ReadOnlyDatabase::open(fixture.database()).unwrap();
            let transaction = database.begin_read().unwrap();
            let events = transaction
                .open_table(TableDefinition::<u64, &str>::new("events"))
                .unwrap();
            for record in events.iter().unwrap() {
                let (_, value) = record.unwrap();
                let recorded: Value = serde_json::from_str(value.value()).unwrap();
                match recorded["event"]["type"].as_str().unwrap() {
                    "io.lowkeylab.worktreepool.worktree.retirement.started.v1" => assert_that!(
                        recorded["stream_id"].as_str(),
                        eq(Some(format!("repositories/{repo}").as_str()))
                    ),
                    "io.lowkeylab.worktreepool.worktree.retirement.finished.v1" => assert_that!(
                        recorded["stream_id"].as_str(),
                        eq(Some(
                            format!(
                                "catalogs/{}",
                                before["context"]["catalog_id"].as_str().unwrap()
                            )
                            .as_str()
                        ))
                    ),
                    _ => {}
                }
            }
        }
        for damaged in [false, true] {
            if damaged {
                damage_derived_catalog(&fixture.database(), "malformed");
            }
            assert_that!(
                fixture.run(&["catalog", "rebuild"]).status.code(),
                eq(Some(0))
            );
            assert_that!(
                &fixture.json(&["catalog", "info"])["data"],
                eq(&before["data"])
            );
            assert_that!(fixture.json(&["events", "list"]), eq(&history));
            assert_that!(
                fixture.json(&["repo", "inspect", "--repo", &repo])["data"]["registered_count"]
                    .as_u64(),
                eq(Some(1))
            );
            if let Some(owner) = &owner {
                assert_that!(
                    fixture.json(&["assignment", "inspect", owner])["data"]["assignment"]["state"]
                        .as_str(),
                    eq(Some("active"))
                );
                assert_that!(
                    fs::read(path.join("tracked")).unwrap().as_slice(),
                    eq(b"newer owner survives replay\n".as_slice())
                );
                assert_that!(fixture.json(&["worktree", "inspect", &worktree])["data"]["worktree"]["registration_state"].as_str(), eq(Some("retired")));
            } else {
                assert_that!(path.exists(), eq(false));
                assert_that!(
                    fixture.json(&["acquire", "--repo", &repo])["outcome"].as_str(),
                    eq(Some("pending"))
                );
                assert_that!(fixture.json(&["events", "list"]), eq(&history));
            }
        }
    }
}

#[googletest::test]
fn retirement_resume_and_resource_entrypoints_wait_for_catalog_maintenance() {
    use std::io::Write;
    for entrypoint in ["worktree", "repository", "preview", "apply"] {
        let fixture = Fixture::new();
        let (repository, path, repo, worktree) = fixture.retirement_candidate();
        let reconciliation = fixture.json(&["recover", "apply", "--worktree", &worktree]);
        let proof = reconciliation["context"]["operation_id"].as_str().unwrap();
        fixture.git(
            &repository,
            &["worktree".as_ref(), "remove".as_ref(), path.as_os_str()],
        );
        let mut retiring = fixture.paused_recovery_checkpoint(
            "retire",
            "intent",
            false,
            Some(&format!("{worktree}:{proof}")),
        );
        retiring.kill().unwrap();
        retiring.wait().unwrap();
        let catalog = fixture.json(&["catalog", "info"]);
        let operation = catalog["data"]["retirements"][0]["operation_id"]
            .as_str()
            .unwrap();
        let history = fixture.json(&["events", "list"]);
        let mut rebuilding = fixture.paused_catalog("rebuild", "validated", true);
        let mut command = fixture.command();
        match entrypoint {
            "worktree" => command.args(["worktree", "inspect", &worktree]),
            "repository" => command.args(["repo", "inspect", "--repo", &repo]),
            action => command.args(["recover", action, "--operation", operation]),
        };
        let mut waiting = command.spawn().unwrap();
        wait_for_maintenance_waiter(&fixture, &mut waiting);
        rebuilding
            .stdin
            .as_mut()
            .unwrap()
            .write_all(b"continue\n")
            .unwrap();
        assert_that!(wait(rebuilding).status.code(), eq(Some(0)));
        let result = wait(waiting);
        assert_that!(result.status.code(), eq(Some(0)));
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_that!(value["outcome"].as_str(), eq(Some("completed")));
        if entrypoint == "apply" {
            assert_that!(value["data"]["retired"].as_bool(), eq(Some(true)));
            assert_that!(
                fixture.json(&["events", "list"])["data"]["events"]
                    .as_array()
                    .unwrap()
                    .len(),
                eq(history["data"]["events"].as_array().unwrap().len() + 1)
            );
        } else {
            assert_that!(fixture.json(&["events", "list"]), eq(&history));
        }
        assert_that!(path.exists(), eq(false));
    }
}

#[googletest::test]
fn parse_errors_honor_available_configured_json_and_output_precedence() {
    for (explicit, environment, flag, expected_json) in [
        (false, None, false, true),
        (true, None, false, true),
        (true, Some("false"), false, false),
        (true, Some("0"), true, true),
    ] {
        let fixture = Fixture::new();
        let config = if explicit {
            fixture
                .root
                .path()
                .join(OsString::from_vec(b"settings-\xff.toml".to_vec()))
        } else {
            let directory = fixture.root.path().join("config/worktree-pool");
            fs::create_dir_all(&directory).unwrap();
            directory.join("config.toml")
        };
        fs::write(&config, "json = true\n").unwrap();
        let base = fixture.command();
        let mut command = Command::new(base.get_program());
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command.env_clear();
        for (name, value) in base.get_envs() {
            if let Some(value) = value {
                command.env(name, value);
            }
        }
        if explicit {
            command.arg("--config").arg(&config);
        }
        if flag {
            command.arg("--json");
        }
        if let Some(value) = environment {
            command.env("WORKTREE_POOL_JSON", value);
        }
        command.args(["catalog", "secret-not-a-command"]);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let result = output(command);
        assert_that!(result.status.code(), eq(Some(2)));
        assert_that!(!result.stdout.is_empty(), eq(expected_json));
        if expected_json {
            let value: Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_that!(value["command"].as_str(), eq(Some("parse")));
            assert_that!(value["reason_code"].as_str(), eq(Some("invalid_arguments")));
            assert_that!(value["outcome"].as_str(), eq(Some("rejected")));
            assert_that!(
                result
                    .stdout
                    .strip_suffix(b"\n")
                    .is_some_and(|line| !line.contains(&b'\n')),
                eq(true)
            );
        }
        assert_that!(
            String::from_utf8_lossy(&result.stdout).contains("secret-not-a-command"),
            eq(false)
        );
        assert_that!(
            String::from_utf8_lossy(&result.stderr).contains("secret-not-a-command"),
            eq(false)
        );
        assert_that!(fixture.database().exists(), eq(false));
        assert_that!(
            fs::read_to_string(config).unwrap().as_str(),
            eq("json = true\n")
        );
    }
    for option in ["--config", "--catalog-dir", "--repo"] {
        let fixture = Fixture::new();
        let base = fixture.command();
        let mut command = Command::new(base.get_program());
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command.env_clear();
        for (name, value) in base.get_envs() {
            if let Some(value) = value {
                command.env(name, value);
            }
        }
        command
            .env("WORKTREE_POOL_JSON", "false")
            .args([option, "--json", "secret-not-a-command"]);
        let result = output(command);
        assert_that!(result.status.code(), eq(Some(2)));
        let value: Value = serde_json::from_slice(&result.stdout)
            .expect("missing option value must retain explicit --json");
        assert_that!(value["reason_code"].as_str(), eq(Some("invalid_arguments")));
        assert_that!(
            result
                .stdout
                .strip_suffix(b"\n")
                .is_some_and(|line| !line.contains(&b'\n')),
            eq(true)
        );
        assert_that!(result.stderr.is_empty(), eq(true));
        assert_that!(fixture.database().exists(), eq(false));
    }
    for args in [
        vec!["--config=--json", "secret-not-a-command"],
        vec!["--", "--json", "secret-not-a-command"],
    ] {
        let fixture = Fixture::new();
        let base = fixture.command();
        let mut command = Command::new(base.get_program());
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        command.env_clear();
        for (name, value) in base.get_envs() {
            if let Some(value) = value {
                command.env(name, value);
            }
        }
        command.env("WORKTREE_POOL_JSON", "false").args(args);
        let result = output(command);
        assert_that!(result.status.code(), eq(Some(2)));
        assert_that!(result.stdout.is_empty(), eq(true));
        assert_that!(
            String::from_utf8_lossy(&result.stderr).contains("secret-not-a-command"),
            eq(false)
        );
        assert_that!(fixture.database().exists(), eq(false));
    }
}

#[googletest::test]
fn canonical_crlf_registered_checkouts_acquire_release_and_retain_ignored_state() {
    for attributes in [true, false] {
        let fixture = Fixture::new();
        let (source, checkout, repo) = fixture.acquisition();
        if attributes {
            fs::write(source.join(".gitattributes"), b"tracked text eol=crlf\n").unwrap();
            fixture.git(&source, &["add".as_ref(), ".gitattributes".as_ref()]);
            fixture.git(
                &source,
                &["commit".as_ref(), "-m".as_ref(), "line endings".as_ref()],
            );
            fixture.git(&checkout, &["pull".as_ref(), "--ff-only".as_ref()]);
        } else {
            fixture.git(
                &checkout,
                &["config".as_ref(), "core.autocrlf".as_ref(), "true".as_ref()],
            );
        }
        fs::remove_file(checkout.join("tracked")).unwrap();
        fixture.git(
            &checkout,
            &["checkout".as_ref(), "--".as_ref(), "tracked".as_ref()],
        );
        assert_that!(
            fs::read(checkout.join("tracked")).unwrap().as_slice(),
            eq(b"protected checkout\r\n".as_slice())
        );
        assert_that!(
            fixture
                .git_text(&checkout, &["status", "--porcelain"])
                .as_str(),
            eq("")
        );
        fs::write(checkout.join(".git/info/exclude"), b"build-cache\n").unwrap();
        fs::write(checkout.join("build-cache"), b"retained artifact").unwrap();
        let tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
        let acquired = fixture.json(&["acquire", "--repo", &repo]);
        assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
        let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
        assert_that!(
            fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
            eq(tip.as_str())
        );
        let released = fixture.json(&["release", handle]);
        assert_that!(released["outcome"].as_str(), eq(Some("completed")));
        assert_that!(
            fs::read(checkout.join("build-cache")).unwrap().as_slice(),
            eq(b"retained artifact".as_slice())
        );
        assert_that!(
            fs::read(checkout.join("tracked")).unwrap().as_slice(),
            eq(b"protected checkout\r\n".as_slice())
        );
    }
}

#[googletest::test]
fn canonical_crlf_new_creation_acquires_and_releases_without_changing_the_caller() {
    let fixture = Fixture::new();
    let (source, checkout, repo) = fixture.empty_pool();
    fs::write(source.join(".gitattributes"), b"tracked text eol=crlf\n").unwrap();
    fixture.git(&source, &["add".as_ref(), ".gitattributes".as_ref()]);
    fixture.git(
        &source,
        &["commit".as_ref(), "-m".as_ref(), "line endings".as_ref()],
    );
    let caller_tip = fixture.git_text(&checkout, &["rev-parse", "HEAD"]);
    let caller_index = fs::read(checkout.join(".git/index")).unwrap();
    let acquired = fixture.json(&["acquire", "--repo", &repo]);
    assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
    let bytes: Vec<u8> =
        serde_json::from_value(acquired["data"]["assignment"]["path"]["bytes"].clone()).unwrap();
    let path = PathBuf::from(OsString::from_vec(bytes));
    assert_that!(
        fs::read(path.join("tracked")).unwrap().as_slice(),
        eq(b"protected checkout\r\n".as_slice())
    );
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    assert_that!(
        fixture.json(&["release", handle])["outcome"].as_str(),
        eq(Some("completed"))
    );
    assert_that!(
        fixture.git_text(&checkout, &["rev-parse", "HEAD"]).as_str(),
        eq(caller_tip.as_str())
    );
    assert_that!(
        fs::read(checkout.join(".git/index")).unwrap().as_slice(),
        eq(caller_index.as_slice())
    );
    assert_that!(
        fs::read(checkout.join("tracked")).unwrap().as_slice(),
        eq(b"protected checkout\n".as_slice())
    );
}

#[googletest::test]
fn canonical_crlf_release_accepts_a_clean_conversion_after_acquisition() {
    let fixture = Fixture::new();
    let (_, checkout, repo) = fixture.acquisition();
    let acquired = fixture.json(&["acquire", "--repo", &repo]);
    assert_that!(acquired["outcome"].as_str(), eq(Some("completed")));
    let handle = acquired["context"]["assignment_handle"].as_str().unwrap();
    fixture.git(
        &checkout,
        &["config".as_ref(), "core.autocrlf".as_ref(), "true".as_ref()],
    );
    fs::remove_file(checkout.join("tracked")).unwrap();
    fixture.git(
        &checkout,
        &["checkout".as_ref(), "--".as_ref(), "tracked".as_ref()],
    );
    assert_that!(
        fs::read(checkout.join("tracked")).unwrap().as_slice(),
        eq(b"protected checkout\r\n".as_slice())
    );
    assert_that!(
        fixture
            .git_text(&checkout, &["status", "--porcelain"])
            .as_str(),
        eq("")
    );
    let released = fixture.json(&["release", handle]);
    assert_that!(released["outcome"].as_str(), eq(Some("completed")));
    assert_that!(
        fixture.json(&["assignment", "inspect", handle])["data"]["assignment"]["state"].as_str(),
        eq(Some("released"))
    );
    assert_that!(
        fs::read(checkout.join("tracked")).unwrap().as_slice(),
        eq(b"protected checkout\r\n".as_slice())
    );
}
