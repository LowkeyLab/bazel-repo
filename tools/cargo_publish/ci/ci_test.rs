use googletest::{
    assert_that,
    matchers::{contains_substring, eq},
};
use serde_json::Value;
use std::{env, fs, path::PathBuf, process::Command};
fn tool(path: &str) -> PathBuf {
    PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
        .join(env::var_os("TEST_WORKSPACE").unwrap())
        .join(path)
}
#[test]
fn packaging_failure_retains_incomplete_report_and_original_exit() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let bin = root.path().join("bazel-bin");
    fs::create_dir(&bin).unwrap();
    let evidence = root.path().join("evidence");
    let result = Command::new(tool(env!("CI_RUNNER")))
        .args(["--bazel"])
        .arg(tool(env!("FAKE_BAZEL")))
        .arg("--workspace")
        .arg(&workspace)
        .arg("--output")
        .arg(&evidence)
        .env("FAKE_BAZEL_BIN", &bin)
        .output()
        .unwrap();
    assert_that!(result.status.code(), eq(Some(17)));
    let summary: Value =
        serde_json::from_slice(&fs::read(evidence.join("summary.json")).unwrap()).unwrap();
    assert_that!(summary["status"].as_str(), eq(Some("failure")));
    assert_that!(summary["failed_stage"].as_str(), eq(Some("library")));
    let report = evidence.join("library/runs/new-failed-run/report.json");
    let report_data: Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
    assert_that!(report_data["status"].as_str(), eq(Some("incomplete")));
    assert_that!(
        fs::read_to_string(report.parent().unwrap().join("1-cargo_package.log")).unwrap(),
        contains_substring("missing crate")
    );
    assert_that!(
        report.parent().unwrap().join("cargo-home").exists(),
        eq(false)
    );
    assert_that!(evidence.join("installed").exists(), eq(false));
}

#[test]
fn serial_matrix_hands_exact_consumer_pair_to_installed_acceptance() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let bin = root.path().join("bazel-bin");
    fs::create_dir(&bin).unwrap();
    let stale = bin.join("tools/cargo_publish/fixtures/publish.runs/old-success");
    fs::create_dir_all(&stale).unwrap();
    fs::write(stale.join("report.json"), "stale report").unwrap();
    let evidence = root.path().join("evidence");
    let result = Command::new(tool(env!("CI_RUNNER")))
        .arg("--bazel")
        .arg(tool(env!("FAKE_BAZEL")))
        .arg("--workspace")
        .arg(&workspace)
        .arg("--output")
        .arg(&evidence)
        .env("FAKE_BAZEL_BIN", &bin)
        .env("FAKE_MODE", "success")
        .output()
        .unwrap();
    assert_that!(result.status.code(), eq(Some(0)));
    let summary: Value =
        serde_json::from_slice(&fs::read(evidence.join("summary.json")).unwrap()).unwrap();
    assert_that!(summary["status"].as_str(), eq(Some("success")));
    let names: Vec<_> = summary["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_that!(
        names,
        eq(&vec![
            "bazel-info",
            "library",
            "binary",
            "combined",
            "consumer",
            "installed"
        ])
    );
    let artifact: Value =
        serde_json::from_slice(&fs::read(evidence.join("installed/artifact.json")).unwrap())
            .unwrap();
    assert_that!(
        artifact["generic_run_id"].as_str(),
        eq(Some("new-consumer"))
    );
    assert_that!(
        fs::read_to_string(evidence.join("installed/installed-cli.log")).unwrap(),
        contains_substring("124 passed")
    );
    assert_that!(
        evidence.join("library/runs/old-success").exists(),
        eq(false)
    );
    assert_that!(
        evidence
            .join("installed/installation-configuration.json")
            .exists(),
        eq(false)
    );
}

#[test]
fn checked_in_consumer_cargo_input_rejects_certification() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir_all(workspace.join("worktree_pool")).unwrap();
    fs::write(
        workspace.join("worktree_pool/Cargo.toml"),
        "unexpected handwritten consumer input",
    )
    .unwrap();
    let bin = root.path().join("bazel-bin");
    fs::create_dir(&bin).unwrap();
    let evidence = root.path().join("evidence");
    let result = Command::new(tool(env!("CI_RUNNER")))
        .arg("--bazel")
        .arg(tool(env!("FAKE_BAZEL")))
        .arg("--workspace")
        .arg(&workspace)
        .arg("--output")
        .arg(&evidence)
        .env("FAKE_BAZEL_BIN", &bin)
        .env("FAKE_MODE", "success")
        .output()
        .unwrap();
    assert_that!(result.status.code(), eq(Some(1)));
    let summary: Value =
        serde_json::from_slice(&fs::read(evidence.join("summary.json")).unwrap()).unwrap();
    assert_that!(
        summary["failed_stage"].as_str(),
        eq(Some("consumer-inputs"))
    );
    assert_that!(
        fs::read_to_string(evidence.join("consumer-inputs.log")).unwrap(),
        contains_substring("Cargo.toml")
    );
    assert_that!(evidence.join("library").exists(), eq(false));
}

#[test]
fn installed_failure_retains_behavior_log_and_original_exit() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let bin = root.path().join("bazel-bin");
    fs::create_dir(&bin).unwrap();
    let evidence = root.path().join("evidence");
    let result = Command::new(tool(env!("CI_RUNNER")))
        .arg("--bazel")
        .arg(tool(env!("FAKE_BAZEL")))
        .arg("--workspace")
        .arg(&workspace)
        .arg("--output")
        .arg(&evidence)
        .env("FAKE_BAZEL_BIN", &bin)
        .env("FAKE_MODE", "installed-failure")
        .output()
        .unwrap();
    assert_that!(result.status.code(), eq(Some(23)));
    let summary: Value =
        serde_json::from_slice(&fs::read(evidence.join("summary.json")).unwrap()).unwrap();
    assert_that!(summary["failed_stage"].as_str(), eq(Some("installed")));
    assert_that!(
        fs::read_to_string(evidence.join("installed/installed-cli.log")).unwrap(),
        contains_substring("public CLI assertion failed")
    );
    assert_that!(evidence.join("installed/runtime.json").exists(), eq(false));
}

#[test]
fn collector_error_does_not_replace_failed_command_status() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let bin = root.path().join("bazel-bin");
    fs::create_dir(&bin).unwrap();
    let evidence = root.path().join("evidence");
    let result = Command::new(tool(env!("CI_RUNNER")))
        .arg("--bazel")
        .arg(tool(env!("FAKE_BAZEL")))
        .arg("--workspace")
        .arg(&workspace)
        .arg("--output")
        .arg(&evidence)
        .env("FAKE_BAZEL_BIN", &bin)
        .env("FAKE_MODE", "collection-failure")
        .env("FAKE_EVIDENCE", &evidence)
        .output()
        .unwrap();
    assert_that!(result.status.code(), eq(Some(17)));
    let summary: Value =
        serde_json::from_slice(&fs::read(evidence.join("summary.json")).unwrap()).unwrap();
    assert_that!(
        summary["collection_errors"].as_array().unwrap().is_empty(),
        eq(false)
    );
    let stderr = fs::read_to_string(evidence.join("library/stderr.log")).unwrap();
    assert_that!(&stderr, contains_substring("<REDACTED>"));
    assert_that!(stderr.contains("dummy-header-secret"), eq(false));
}

#[test]
fn redirected_package_directory_rejects_handoff_and_does_not_collect_outside_bytes() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let bin = root.path().join("bazel-bin");
    fs::create_dir(&bin).unwrap();
    let evidence = root.path().join("evidence");
    let result = Command::new(tool(env!("CI_RUNNER")))
        .arg("--bazel")
        .arg(tool(env!("FAKE_BAZEL")))
        .arg("--workspace")
        .arg(&workspace)
        .arg("--output")
        .arg(&evidence)
        .env("FAKE_BAZEL_BIN", &bin)
        .env("FAKE_MODE", "redirected-package")
        .output()
        .unwrap();
    assert_that!(result.status.code(), eq(Some(1)));
    let summary: Value =
        serde_json::from_slice(&fs::read(evidence.join("summary.json")).unwrap()).unwrap();
    assert_that!(summary["failed_stage"].as_str(), eq(Some("library")));
    assert_that!(
        evidence
            .join("library/runs/new-library/target/package/library.crate")
            .exists(),
        eq(false)
    );
    assert_that!(evidence.join("installed").exists(), eq(false));
}

#[test]
fn failure_before_archive_creation_retains_report_without_collection_error() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let bin = root.path().join("bazel-bin");
    fs::create_dir(&bin).unwrap();
    let evidence = root.path().join("evidence");
    let result = Command::new(tool(env!("CI_RUNNER")))
        .arg("--bazel")
        .arg(tool(env!("FAKE_BAZEL")))
        .arg("--workspace")
        .arg(&workspace)
        .arg("--output")
        .arg(&evidence)
        .env("FAKE_BAZEL_BIN", &bin)
        .env("FAKE_MODE", "early-failure")
        .output()
        .unwrap();
    assert_that!(result.status.code(), eq(Some(19)));
    let summary: Value =
        serde_json::from_slice(&fs::read(evidence.join("summary.json")).unwrap()).unwrap();
    assert_that!(
        summary["collection_errors"].as_array().unwrap().is_empty(),
        eq(true)
    );
    assert_that!(
        fs::read_to_string(evidence.join("library/runs/new-library/1-toolchain.log")).unwrap(),
        contains_substring("toolchain setup failed")
    );
}
