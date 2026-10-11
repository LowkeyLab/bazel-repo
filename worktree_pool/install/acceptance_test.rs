//! Public installed acceptance command. Historical Cargo evidence proves only
//! migration interoperability; fresh registry/native/124 CLI certification is separate.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use googletest::{
    assert_that, gtest,
    matchers::{contains_substring, eq},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn runfile(path: &str) -> PathBuf {
    PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
        .join(env::var_os("TEST_WORKSPACE").unwrap())
        .join(path)
}
fn invoke(root: &Path, archive: &Path, report: &[u8], output: &str, hostile: bool) -> Output {
    let report_path = root.join("relocated-report.json");
    fs::write(&report_path, report).unwrap();
    let mut command = Command::new(runfile(env!("DRIVER")));
    command
        .args([
            "/missing/cargo",
            "/missing/rustc",
            "/missing/git-source",
            "/missing/zlib-source",
            "/missing/cli-tests",
        ])
        .arg("--archive")
        .arg(archive)
        .arg("--report")
        .arg(report_path)
        .args([
            "--expected-version",
            "0.1.0",
            "--sysroot",
            "/missing/sysroot",
            "--runfiles-root",
        ])
        .arg(root)
        .args(["--workspace", "_main", "--output"])
        .arg(root.join(output))
        .current_dir(root);
    if hostile {
        command
            .env("RUSTFLAGS", "--cfg=ambient")
            .env("RUSTC_WRAPPER", "/missing/wrapper")
            .env("CARGO_HOME", "/missing/home")
            .env("CC", "/missing/cc")
            .env("HOME", "/missing/ambient-home");
    }
    command.output().unwrap()
}
fn rejected(root: &Path, archive: &Path, report: &[u8], output: &str) {
    let result = invoke(root, archive, report, output, false);
    assert_that!(result.status.success(), eq(false));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert_that!(stderr.as_ref(), contains_substring("artifact_validation"));
    assert_that!(stderr.contains("/missing/rustc"), eq(false));
    assert_that!(root.join(output).join("installed").exists(), eq(false));
    assert_that!(root.join(output).join("artifact.json").exists(), eq(false));
}
#[gtest]
fn unsupported_report_is_rejected_before_any_tool_runs() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let archive = root.join("package.crate");
    fs::write(&archive, b"unverified").unwrap();
    rejected(
        root,
        &archive,
        &serde_json::to_vec(&json!({"schema_version":99})).unwrap(),
        "acceptance",
    );
}

fn baseline(root: &Path) -> (PathBuf, Value) {
    let archive = root.join("relocated.crate");
    fs::copy(
        runfile("worktree_pool/install/testdata/consumer.crate"),
        &archive,
    )
    .unwrap();
    let report: Value = serde_json::from_slice(
        &fs::read(runfile(
            "worktree_pool/install/testdata/consumer-report.json",
        ))
        .unwrap(),
    )
    .unwrap();
    (archive, report)
}
#[gtest]
fn genuine_relocated_pair_reaches_tools_and_retains_exact_archive_lock_and_isolated_environment() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (archive, report) = baseline(root);
    let result = invoke(
        root,
        &archive,
        &serde_json::to_vec(&report).unwrap(),
        "acceptance",
        true,
    );
    assert_that!(result.status.success(), eq(false));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert_that!(stderr.contains("artifact_validation"), eq(false));
    assert_that!(stderr.as_ref(), contains_substring("/missing/rustc"));
    let artifact: Value =
        serde_json::from_slice(&fs::read(root.join("acceptance/artifact.json")).unwrap()).unwrap();
    assert_that!(
        fs::read(artifact["verified_archive"].as_str().unwrap()).unwrap(),
        eq(&fs::read(&archive).unwrap())
    );
    assert_that!(&artifact["generic_run_id"], eq(&report["run_id"]));
    assert_that!(
        artifact["archive_sha256"].as_str(),
        eq(Some(
            "237e5589d14294d4f12b98e65858f4856dac995a2751df63e38a48e50909c925"
        ))
    );
    let extracted = PathBuf::from(artifact["extracted_package"].as_str().unwrap());
    assert_that!(
        Sha256::digest(fs::read(extracted.join("Cargo.lock")).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
            .as_str(),
        eq(report["lockfile"]["sha256"].as_str().unwrap())
    );
    assert_that!(
        fs::read(root.join("acceptance/validation-report.json")).unwrap(),
        eq(&serde_json::to_vec(&report).unwrap())
    );
    let configuration: Value = serde_json::from_slice(
        &fs::read(root.join("acceptance/installation-configuration.json")).unwrap(),
    )
    .unwrap();
    let environment = &configuration["environment"];
    for key in ["RUSTFLAGS", "RUSTC_WRAPPER", "CC"] {
        assert_that!(environment.get(key).is_none(), eq(true));
    }
    for key in ["HOME", "CARGO_HOME", "TMPDIR", "RUSTC", "CARGO_TARGET_DIR"] {
        assert_that!(
            Path::new(environment[key].as_str().unwrap()).starts_with(root.join("acceptance")),
            eq(true)
        );
    }
    assert_that!(root.join("acceptance/runtime.json").exists(), eq(false));
}

const CHECKS: [&str; 12] = [
    "input_fidelity",
    "toolchain",
    "configuration",
    "generated_manifest",
    "cargo_package",
    "archive_policy",
    "archive_fidelity",
    "original_manifest",
    "normalized_manifest",
    "locked_metadata",
    "direct_dependencies",
    "locked_builds",
];
#[gtest]
fn unsuccessful_incomplete_unknown_or_missing_report_evidence_is_rejected() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (archive, baseline) = baseline(root);
    for (index, (key, value)) in [
        ("report_kind", json!("unknown")),
        ("status", json!("failure")),
        ("completed", json!(false)),
        ("completed_at", json!("")),
        ("commands", json!([])),
        ("configuration", Value::Null),
        ("inputs", Value::Null),
        ("archive", Value::Null),
    ]
    .into_iter()
    .enumerate()
    {
        let mut report = baseline.clone();
        report[key] = value;
        rejected(
            root,
            &archive,
            &serde_json::to_vec(&report).unwrap(),
            &format!("case-{index}"),
        );
    }
    let mut report = baseline.clone();
    report.as_object_mut().unwrap().remove("completed");
    rejected(
        root,
        &archive,
        &serde_json::to_vec(&report).unwrap(),
        "missing-completion",
    );
}
#[gtest]
fn every_required_check_must_be_present_complete_passed_and_supported_by_evidence() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (archive, baseline) = baseline(root);
    for name in CHECKS {
        let mut report = baseline.clone();
        report["checks"].as_object_mut().unwrap().remove(name);
        rejected(
            root,
            &archive,
            &serde_json::to_vec(&report).unwrap(),
            &format!("{name}-absent"),
        );
        for (key, value) in [
            ("completed", json!(false)),
            ("status", json!("failed")),
            ("evidence", json!({})),
        ] {
            let mut report = baseline.clone();
            report["checks"][name][key] = value;
            rejected(
                root,
                &archive,
                &serde_json::to_vec(&report).unwrap(),
                &format!("{name}-{key}"),
            );
        }
    }
}
#[gtest]
fn fabricated_commands_target_coverage_and_archive_fingerprints_are_rejected() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (archive, baseline) = baseline(root);
    let mut cases = Vec::new();
    let mut fabricated = baseline.clone();
    fabricated["checks"]["direct_dependencies"]["evidence"] = json!({"fabricated":true});
    cases.push(fabricated);
    let mut failed = baseline.clone();
    failed["commands"][2]["returncode"] = json!(17);
    cases.push(failed);
    let mut duplicate = baseline.clone();
    duplicate["commands"][1]["id"] = duplicate["commands"][0]["id"].clone();
    cases.push(duplicate);
    let mut no_binary = baseline.clone();
    for command in no_binary["commands"].as_array_mut().unwrap() {
        if command["phase"] == "locked_builds"
            && command["argv"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "--bin")
        {
            command["argv"] = json!(["cargo", "build", "--lib", "--locked"]);
        }
    }
    cases.push(no_binary);
    let mut bad_hash = baseline.clone();
    bad_hash["archive"]["sha256"] = json!("0".repeat(64));
    cases.push(bad_hash);
    let mut wrong_target = baseline.clone();
    wrong_target["configuration"]["target"] = json!("aarch64-unknown-linux-gnu");
    cases.push(wrong_target);
    for (index, report) in cases.into_iter().enumerate() {
        rejected(
            root,
            &archive,
            &serde_json::to_vec(&report).unwrap(),
            &format!("case-{index}"),
        );
    }
    let mut bytes = fs::read(&archive).unwrap();
    bytes.push(0);
    fs::write(&archive, bytes).unwrap();
    rejected(
        root,
        &archive,
        &serde_json::to_vec(&baseline).unwrap(),
        "changed-archive-bytes",
    );
}
#[gtest]
fn malformed_report_wrong_shape_absent_archive_and_wrong_consumer_identity_are_rejected() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (archive, baseline) = baseline(root);
    rejected(root, &archive, b"{", "malformed");
    rejected(root, &archive, b"[]", "wrong-shape");
    rejected(
        root,
        &root.join("absent.crate"),
        &serde_json::to_vec(&baseline).unwrap(),
        "absent-archive",
    );
    for (key, value) in [("name", "other-package"), ("version", "99.0.0")] {
        let mut report = baseline.clone();
        report["package"][key] = json!(value);
        rejected(root, &archive, &serde_json::to_vec(&report).unwrap(), key);
    }
}
#[gtest]
fn ambient_ancestor_cargo_configuration_is_rejected_before_tools() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (archive, report) = baseline(root);
    fs::create_dir(root.join(".cargo")).unwrap();
    fs::write(
        root.join(".cargo/config.toml"),
        "[build]\nrustflags = [\"--cfg=ambient\"]\n",
    )
    .unwrap();
    let result = invoke(
        root,
        &archive,
        &serde_json::to_vec(&report).unwrap(),
        "acceptance",
        false,
    );
    assert_that!(result.status.success(), eq(false));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert_that!(
        stderr.as_ref(),
        contains_substring("installation_configuration")
    );
    assert_that!(stderr.contains("/missing/rustc"), eq(false));
    assert_that!(root.join("acceptance/runtime.json").exists(), eq(false));
}

#[gtest]
fn existing_output_is_refused_without_altering_retained_files() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (archive, report) = baseline(root);
    fs::create_dir(root.join("retained")).unwrap();
    fs::write(root.join("retained/marker"), b"preserve").unwrap();
    let result = invoke(
        root,
        &archive,
        &serde_json::to_vec(&report).unwrap(),
        "retained",
        false,
    );
    assert_that!(result.status.success(), eq(false));
    assert_that!(
        String::from_utf8_lossy(&result.stderr).contains("/missing/rustc"),
        eq(false)
    );
    assert_that!(
        fs::read(root.join("retained/marker")).unwrap(),
        eq(b"preserve")
    );
    assert_that!(root.join("retained/artifact.json").exists(), eq(false));
}
