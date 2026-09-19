use std::{
    env,
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use googletest::{
    assert_that,
    matchers::{contains_substring, eq},
};

fn bot() -> Command {
    let binary = PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
        .join(env::var_os("TEST_WORKSPACE").unwrap())
        .join(env!("BOT_BINARY"));
    let mut command = Command::new(binary);
    command
        .env_clear()
        .env("DISCORD_TOKEN", "dummy-token")
        .env("DATABASE_URL", "postgres://localhost/unused")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn output(mut command: Command) -> Output {
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!("bot did not exit before watchdog: {output:?}");
        }
        // Poll only for process termination; no timing assumption starts a probe.
        thread::sleep(Duration::from_millis(10));
    }
}

#[googletest::test]
fn occupied_health_port_fails_before_external_initialization() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut command = bot();
    command.env(
        "HEALTH_BIND_ADDRESS",
        listener.local_addr().unwrap().to_string(),
    );
    let output = output(command);
    assert_that!(output.status.success(), eq(false));
    assert_that!(
        String::from_utf8_lossy(&output.stderr),
        contains_substring("health listener bind failed")
    );
}

#[googletest::test]
fn migration_ignores_health_configuration_and_binding() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    for setting in [
        "invalid".to_owned(),
        listener.local_addr().unwrap().to_string(),
    ] {
        let mut command = bot();
        command.arg("--migrate").env("HEALTH_BIND_ADDRESS", setting);
        let output = output(command);
        assert_that!(output.status.success(), eq(false));
        assert_that!(
            String::from_utf8_lossy(&output.stderr),
            contains_substring("MIGRATION_DATABASE_URL is required")
        );
    }
}
