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
