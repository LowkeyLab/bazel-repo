use std::{collections::BTreeMap, fmt::Write, fs, path::PathBuf, process::Command};

use googletest::{
    assert_that, gtest,
    matchers::{contains_substring, eq},
};
use serde_json::{Value, json};
use tempfile::tempdir;

fn verifier() -> PathBuf {
    std::env::current_dir().unwrap().join(env!("VERIFIER"))
}

#[gtest]
fn changed_declared_bytes_retain_failed_input_report_without_launching_tools() {
    let root = tempdir().unwrap();
    let package = root.path().join("package");
    fs::create_dir(&package).unwrap();
    fs::write(
        package.join("Cargo.toml"),
        "[package]\nname='fixture'\nversion='0.1.0'\n",
    )
    .unwrap();
    fs::write(package.join("lib.rs"), "changed").unwrap();
    let contract = json!({"schema_version":1,"target":"x86_64-unknown-linux-gnu","package":{"name":"fixture","version":"0.1.0"},"library":{"label":"//fixture:lib","path":"lib.rs"},"dependencies":[],"inputs":[{"path":"lib.rs","sha256":"0".repeat(64),"size":1}],"manifest_sha256":"0".repeat(64)});
    fs::write(
        root.path().join("contract.json"),
        serde_json::to_vec(&contract).unwrap(),
    )
    .unwrap();
    let output = Command::new(verifier())
        .args([
            "--package",
            package.to_str().unwrap(),
            "--contract",
            root.path().join("contract.json").to_str().unwrap(),
            "--cargo",
            "/missing/cargo",
            "--rustc",
            "/missing/rustc",
            "--sysroot",
            "/missing/sysroot",
            "--output-root",
            root.path().join("outputs").to_str().unwrap(),
            "--offline",
        ])
        .output()
        .unwrap();
    assert_that!(output.status.code(), eq(Some(1)));
    assert_that!(
        String::from_utf8_lossy(&output.stderr).as_ref(),
        contains_substring("lib.rs")
    );
    let runs = fs::read_dir(root.path().join("outputs"))
        .unwrap()
        .collect::<Vec<_>>();
    assert_that!(runs.len(), eq(1));
    let run = runs[0].as_ref().unwrap().path();
    let report: Value =
        serde_json::from_slice(&fs::read(run.join("report.json")).unwrap()).unwrap();
    assert_that!(&report["status"], eq(&json!("failure")));
    assert_that!(&report["completed"], eq(&json!(false)));
    assert_that!(
        &report["diagnostics"]["phase"],
        eq(&json!("input_fidelity"))
    );
    assert_that!(&report["commands"], eq(&json!([])));
    assert_that!(report["checks"].as_object().unwrap().len(), eq(12));
    assert_that!(run.join("report.pending").exists(), eq(false));
}

const CONTROLLED_RELEASE: &str = include_str!("testdata/toolchain-version.txt");

struct Fixture {
    root: tempfile::TempDir,
    contract: Value,
    metadata: Value,
    normalized: Option<String>,
    proxy_environment: BTreeMap<&'static str, &'static str>,
}

impl Fixture {
    fn new(form: &str) -> Self {
        let root = tempdir().unwrap();
        fs::create_dir(root.path().join("package")).unwrap();
        let library = (form != "binary").then(|| json!({"label":"//fixture:lib","crate_name":"fixture_library","crate_type":"rlib","edition":"2024","path":"src/lib.rs"}));
        let binaries = if form == "library" {
            vec![]
        } else {
            vec![
                json!({"label":"//fixture:first","crate_name":"first","name":"first","crate_type":"bin","edition":"2024","path":"src/first.rs"}),
                json!({"label":"//fixture:second","crate_name":"second","name":"second","crate_type":"bin","edition":"2021","path":"src/second.rs"}),
            ]
        };
        let dependencies = [
            json!({"target":"@crates//:upstream","package":"upstream","alias":"friendly-name","version":"1.2.3","source":"registry+https://github.com/rust-lang/crates.io-index","checksum":"a".repeat(64),"kind":"normal","default_features":false,"features":["extra"]}),
            json!({"target":"@crates//:upstream","package":"upstream","alias":"friendly-name","version":"1.2.3","source":"registry+https://github.com/rust-lang/crates.io-index","checksum":"a".repeat(64),"kind":"dev","default_features":true,"features":["default","extra"]}),
        ];
        let mut manifest = "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\nrust-version='1.96'\nbuild=false\n".to_string();
        let mut targets = vec![];
        let mut sources = vec![("assets/readme.txt".to_string(), "Asset bytes\n".to_string())];
        if library.is_some() {
            manifest.push_str("[lib]\nname='fixture_library'\npath='src/lib.rs'\n");
            targets.push(json!({"name":"fixture_library","kind":["lib"],"src_path":"src/lib.rs","edition":"2024"}));
            sources.push(("src/lib.rs".into(), "pub fn fixture() {}\n".into()));
        }
        for binary in &binaries {
            writeln!(
                manifest,
                "[[bin]]\nname='{}'\npath='{}'\nedition='{}'",
                binary["name"].as_str().unwrap(),
                binary["path"].as_str().unwrap(),
                binary["edition"].as_str().unwrap()
            )
            .unwrap();
            targets.push(json!({"name":binary["name"],"kind":["bin"],"src_path":binary["path"],"edition":binary["edition"]}));
            sources.push((
                binary["path"].as_str().unwrap().into(),
                "fn main() {}\n".into(),
            ));
        }
        manifest.push_str("[dependencies.friendly-name]\npackage='upstream'\nversion='=1.2.3'\ndefault-features=false\nfeatures=['extra']\n[dev-dependencies.friendly-name]\npackage='upstream'\nversion='=1.2.3'\ndefault-features=true\nfeatures=['extra']\n");
        fs::write(root.path().join("package/Cargo.toml"), &manifest).unwrap();
        let mut inputs = vec![];
        for (name, bytes) in sources {
            let destination = root.path().join("package").join(&name);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(destination, &bytes).unwrap();
            inputs.push(json!({"path":name,"size":bytes.len(),"sha256":cargo_publish_core::digest_bytes(bytes.as_bytes())}));
        }
        let contract = json!({"schema_version":1,"target":"x86_64-unknown-linux-gnu","package":{"name":"fixture","version":"0.1.0","edition":"2024","rust-version":"1.96"},"library":library,"binaries":binaries,"tests":[],"dependencies":dependencies,"inputs":inputs,"manifest_sha256":cargo_publish_core::digest_bytes(manifest.as_bytes())});
        let declarations = dependencies.iter().map(|dependency| json!({"name":"upstream","rename":"friendly-name","kind":if dependency["kind"] == "normal" {Value::Null} else {json!("dev")},"req":"=1.2.3","uses_default_features":dependency["default_features"],"features":["extra"],"optional":false,"target":null,"source":"registry+https://github.com/rust-lang/crates.io-index"})).collect::<Vec<_>>();
        let mut metadata = json!({"packages":[{"id":"fixture","name":"fixture","version":"0.1.0","source":null,"dependencies":declarations,"targets":targets},{"id":"upstream","name":"upstream","version":"1.2.3","source":"registry+https://github.com/rust-lang/crates.io-index","dependencies":[],"targets":[]}],"resolve":{"root":"fixture","nodes":[{"id":"fixture","features":[],"deps":[{"name":"friendly_name","pkg":"upstream","dep_kinds":[{"kind":null,"target":null},{"kind":"dev","target":null}]}]},{"id":"upstream","features":["extra","transitive-observation","default"],"deps":[]}]}});
        // Only root declarations/targets are required; transitive packages supply observed graph identity.
        metadata["packages"][1]
            .as_object_mut()
            .unwrap()
            .remove("dependencies");
        metadata["packages"][1]
            .as_object_mut()
            .unwrap()
            .remove("targets");
        let helper = std::env::current_dir().unwrap().join(env!("FAKE_TOOL"));
        for name in ["rustc", "cargo"] {
            fs::copy(&helper, root.path().join(name)).unwrap();
        }
        Self {
            root,
            contract,
            metadata,
            normalized: None,
            proxy_environment: BTreeMap::new(),
        }
    }

    fn invoke(&self, mode: &str) -> (std::process::Output, PathBuf, Value) {
        self.invoke_options(mode, true)
    }

    fn invoke_options(
        &self,
        mode: &str,
        optional_flags: bool,
    ) -> (std::process::Output, PathBuf, Value) {
        fs::write(
            self.root.path().join("contract.json"),
            serde_json::to_vec(&self.contract).unwrap(),
        )
        .unwrap();
        fs::write(self.root.path().join("scenario.json"), serde_json::to_vec(&json!({"mode":mode,"release":CONTROLLED_RELEASE.trim(),"contract":self.contract,"metadata":self.metadata,"normalized":self.normalized,"expected_proxy_environment":self.proxy_environment})).unwrap()).unwrap();
        let mut command = Command::new(verifier());
        command.args([
            "--package",
            self.root.path().join("package").to_str().unwrap(),
            "--contract",
            self.root.path().join("contract.json").to_str().unwrap(),
            "--cargo",
            self.root.path().join("cargo").to_str().unwrap(),
            "--rustc",
            self.root.path().join("rustc").to_str().unwrap(),
            "--sysroot",
            self.root.path().to_str().unwrap(),
            "--output-root",
            self.root.path().join("outputs").to_str().unwrap(),
        ]);
        if optional_flags {
            command.args([
                "--registry-cache",
                self.root.path().join("cache").to_str().unwrap(),
                "--offline",
            ]);
        }
        let result = command
            .env("RUSTFLAGS", "ambient-setting")
            .env("RUSTC_WRAPPER", "ambient-setting")
            .env("CARGO_HOME", "ambient-setting")
            .env("CC", "ambient-setting")
            .env("CFLAGS", "ambient-setting")
            .envs(&self.proxy_environment)
            .output()
            .unwrap();
        let run = fs::read_dir(self.root.path().join("outputs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.join("report.json").exists())
            .unwrap();
        let report: Value =
            serde_json::from_slice(&fs::read(run.join("report.json")).unwrap()).unwrap();
        (result, run, report)
    }

    fn failure(&self, mode: &str, phase: &str, message: &str) -> Value {
        let (result, run, report) = self.invoke(mode);
        assert_that!(result.status.code(), eq(Some(1)));
        assert_that!(
            String::from_utf8_lossy(&result.stderr).as_ref(),
            contains_substring(message)
        );
        assert_that!(&report["status"], eq(&json!("failure")));
        assert_that!(&report["completed"], eq(&json!(false)));
        assert_that!(&report["diagnostics"]["phase"], eq(&json!(phase)));
        assert_that!(&report["checks"][phase]["status"], eq(&json!("failed")));
        assert_that!(run.join("report.pending").exists(), eq(false));
        report
    }
}

#[gtest]
fn missing_compiler_retains_completed_spawn_error_command_and_private_log() {
    let fixture = Fixture::new("library");
    fs::remove_file(fixture.root.path().join("rustc")).unwrap();
    let report = fixture.failure("", "toolchain", "rustc");
    assert_that!(&report["commands"][0]["completed"], eq(&json!(true)));
    assert_that!(&report["commands"][0]["returncode"], eq(&Value::Null));
    assert_that!(
        report["commands"][0]["error"].as_str().unwrap(),
        contains_substring("No such file")
    );
    let log = PathBuf::from(report["commands"][0]["log"].as_str().unwrap());
    assert_that!(
        fs::read_to_string(log).unwrap().as_str(),
        contains_substring("No such file")
    );
}

#[gtest]
fn controlled_pipeline_covers_all_selected_targets_and_preserves_shared_cache_bytes() {
    use std::os::unix::fs::PermissionsExt;
    for form in ["library", "binary", "combined"] {
        let fixture = Fixture::new(form);
        fs::create_dir(fixture.root.path().join("cache")).unwrap();
        for name in [".package-cache", ".package-cache-mutate", ".global-cache"] {
            fs::write(
                fixture.root.path().join("cache").join(name),
                "existing SQLite/cache bytes",
            )
            .unwrap();
        }
        let (result, run, report) = fixture.invoke("");
        assert_that!(result.status.code(), eq(Some(0)));
        assert_that!(String::from_utf8_lossy(&result.stderr).as_ref(), eq(""));
        let public: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_that!(&public["status"], eq(&json!("success")));
        assert_that!(&public["run_id"], eq(&report["run_id"]));
        assert_that!(report["checks"].as_object().unwrap().len(), eq(12));
        let typed: cargo_publish_core::ValidationReport =
            serde_json::from_value(report.clone()).unwrap();
        assert_that!(
            cargo_publish_core::validate_report(&typed, None).is_ok(),
            eq(true)
        );
        let accepted = cargo_publish_core::validate_verified_archive(
            run.join("report.json"),
            report["archive"]["path"].as_str().unwrap(),
            fixture.root.path().join("accepted"),
            None,
        )
        .unwrap();
        assert_that!(
            fs::read_to_string(accepted.extracted_root.join("assets/readme.txt"))
                .unwrap()
                .as_str(),
            eq("Asset bytes\n")
        );
        let targets = report["checks"]["locked_builds"]["evidence"]["targets"]
            .as_array()
            .unwrap();
        assert_that!(
            targets.len(),
            eq(match form {
                "library" => 1,
                "binary" => 2,
                _ => 3,
            })
        );
        assert_that!(
            report["commands"].as_array().unwrap().len(),
            eq(4 + targets.len())
        );
        for command in report["commands"].as_array().unwrap() {
            assert_that!(command["cwd"].as_str().unwrap(), eq(run.to_str().unwrap()));
            assert_that!(&command["returncode"], eq(&json!(0)));
            if command["phase"] == "locked_builds" {
                assert_that!(
                    command["argv"].to_string().as_str(),
                    contains_substring("extracted")
                );
            }
        }
        assert_that!(
            run.metadata().unwrap().permissions().mode() & 0o777,
            eq(0o700)
        );
        assert_that!(
            run.join("report.json")
                .metadata()
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            eq(0o600)
        );
        assert_that!(
            fixture.root.path().join("package/Cargo.lock").exists(),
            eq(false)
        );
        assert_that!(
            &report["configuration"]["environment"]["CARGO_BUILD_JOBS"],
            eq(&json!("2"))
        );
        for key in ["RUSTFLAGS", "RUSTC_WRAPPER", "CC", "CFLAGS", "LD_PRELOAD"] {
            assert_that!(
                report["configuration"]["environment"].get(key).is_none(),
                eq(true)
            );
        }
        for name in [".package-cache", ".package-cache-mutate", ".global-cache"] {
            assert_that!(
                fs::read_to_string(fixture.root.path().join("cache").join(name))
                    .unwrap()
                    .as_str(),
                eq("existing SQLite/cache bytes")
            );
        }
        for name in ["cargo", "rustc"] {
            let probe: Value = serde_json::from_slice(
                &fs::read(fixture.root.path().join(format!("probe-{name}.json"))).unwrap(),
            )
            .unwrap();
            assert_that!(
                probe.as_object().unwrap().keys().all(|key| [
                    "PATH",
                    "NIX_LD",
                    "NIX_LD_LIBRARY_PATH"
                ]
                .contains(&key.as_str())),
                eq(true)
            );
        }
    }
}

#[gtest]
fn command_failures_and_signal_codes_retain_real_command_outcomes() {
    for (mode, phase, code) in [
        ("package", "cargo_package", 23),
        ("metadata", "locked_metadata", 23),
        ("build", "locked_builds", 23),
        ("signal", "toolchain", -15),
    ] {
        let fixture = Fixture::new("combined");
        let report = fixture.failure(mode, phase, &format!("command exited {code}"));
        let command = report["commands"].as_array().unwrap().last().unwrap();
        assert_that!(&command["returncode"], eq(&json!(code)));
        assert_that!(&command["completed"], eq(&json!(true)));
        assert_that!(command["completed_at"].is_string(), eq(true));
        assert_that!(
            PathBuf::from(command["log"].as_str().unwrap()).is_file(),
            eq(true)
        );
    }
}

#[gtest]
fn pipeline_rejects_wrong_versions_archives_manifests_metadata_and_changed_lock() {
    for (mode, phase, message) in [
        ("version", "toolchain", "unsupported pinned toolchain"),
        ("host", "toolchain", "unsupported pinned toolchain"),
        ("archive", "archive_policy", "unexpected"),
        ("original", "original_manifest", "Cargo.toml.orig"),
        ("normalized", "normalized_manifest", "unsupported features"),
        (
            "malformed_metadata",
            "locked_metadata",
            "parse Cargo metadata",
        ),
        ("lock", "locked_builds", "Cargo.lock changed"),
    ] {
        Fixture::new("combined").failure(mode, phase, message);
    }
}

#[gtest]
fn direct_metadata_declarations_edges_and_selected_targets_must_match_contract() {
    for case in [
        "alias",
        "version",
        "kind",
        "defaults",
        "features",
        "conditional",
        "optional",
        "source",
        "edge_alias",
        "edge_version",
        "edge_kind",
        "edge_target",
        "target_path",
        "missing_binary",
        "extra_binary",
        "duplicate_binary",
        "target_name",
        "target_edition",
        "target_kind",
        "root_identity",
        "edge_source",
    ] {
        let mut fixture = Fixture::new("combined");
        match case {
            "alias" => {
                fixture.metadata["packages"][0]["dependencies"][0]["rename"] = json!("different");
            }
            "version" => {
                fixture.metadata["packages"][0]["dependencies"][0]["req"] = json!("^1.2.3");
            }
            "kind" => fixture.metadata["packages"][0]["dependencies"][0]["kind"] = json!("build"),
            "defaults" => {
                fixture.metadata["packages"][0]["dependencies"][0]["uses_default_features"] =
                    json!(true);
            }
            "features" => {
                fixture.metadata["packages"][0]["dependencies"][0]["features"] = json!([]);
            }
            "conditional" => {
                fixture.metadata["packages"][0]["dependencies"][0]["target"] = json!("cfg(unix)");
            }
            "optional" => {
                fixture.metadata["packages"][0]["dependencies"][0]["optional"] = json!(true);
            }
            "source" => {
                fixture.metadata["packages"][0]["dependencies"][0]["source"] = json!("path");
            }
            "edge_alias" => {
                fixture.metadata["resolve"]["nodes"][0]["deps"][0]["name"] = json!("different");
            }
            "edge_version" => fixture.metadata["packages"][1]["version"] = json!("1.2.4"),
            "edge_kind" => {
                fixture.metadata["resolve"]["nodes"][0]["deps"][0]["dep_kinds"] =
                    json!([{"kind":null,"target":null}]);
            }
            "edge_target" => {
                fixture.metadata["resolve"]["nodes"][0]["deps"][0]["dep_kinds"][0]["target"] =
                    json!("cfg(unix)");
            }
            "target_path" => {
                fixture.metadata["packages"][0]["targets"][0]["src_path"] = json!("wrong.rs");
            }
            "extra_binary" => {
                let extra =
                    json!({"name":"extra","kind":["bin"],"src_path":"extra.rs","edition":"2024"});
                fixture.metadata["packages"][0]["targets"]
                    .as_array_mut()
                    .unwrap()
                    .push(extra);
            }
            "duplicate_binary" => {
                let target = fixture.metadata["packages"][0]["targets"][1].clone();
                fixture.metadata["packages"][0]["targets"]
                    .as_array_mut()
                    .unwrap()
                    .push(target);
            }
            "target_name" => {
                fixture.metadata["packages"][0]["targets"][1]["name"] = json!("changed");
            }
            "target_edition" => {
                fixture.metadata["packages"][0]["targets"][1]["edition"] = json!("2021");
            }
            "target_kind" => fixture.metadata["packages"][0]["targets"][1]["kind"] = json!(["lib"]),
            "root_identity" => fixture.metadata["packages"][0]["name"] = json!("changed"),
            "edge_source" => {
                fixture.metadata["packages"][1]["source"] = json!("git+https://example.com/source");
            }
            "missing_binary" => {
                fixture.metadata["packages"][0]["targets"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            _ => unreachable!(),
        }
        fixture.failure("", "direct_dependencies", "retained report");
    }
}

#[gtest]
fn ambient_cargo_configuration_is_rejected_and_required_paths_parse_before_effects() {
    for name in ["config", "config.toml"] {
        let fixture = Fixture::new("library");
        fs::create_dir(fixture.root.path().join(".cargo")).unwrap();
        fs::write(
            fixture.root.path().join(".cargo").join(name),
            "[build]\nrustflags=['--cfg=ambient']\n",
        )
        .unwrap();
        fixture.failure("", "configuration", "ambient Cargo configuration");
    }
    let result = Command::new(verifier()).arg("--offline").output().unwrap();
    assert_that!(result.status.code(), eq(Some(2)));
    for flag in [
        "--package",
        "--contract",
        "--cargo",
        "--rustc",
        "--sysroot",
        "--output-root",
    ] {
        assert_that!(
            String::from_utf8_lossy(&result.stderr).as_ref(),
            contains_substring(flag)
        );
    }
    let help = Command::new(verifier()).arg("--help").output().unwrap();
    assert_that!(help.status.code(), eq(Some(0)));
    assert_that!(
        String::from_utf8_lossy(&help.stdout).as_ref(),
        contains_substring("--registry-cache")
    );
}

#[gtest]
fn omitted_optional_flags_use_managed_online_cache_and_missing_contract_retains_report() {
    let fixture = Fixture::new("binary");
    let (result, run, report) = fixture.invoke_options("", false);
    assert_that!(result.status.code(), eq(Some(0)));
    assert_that!(
        &report["configuration"]["network_mode"],
        eq(&json!("online"))
    );
    assert_that!(
        &report["configuration"]["registry_cache"],
        eq(&json!(fixture.root.path().join("outputs/registry-cache")))
    );
    assert_that!(
        report["commands"]
            .as_array()
            .unwrap()
            .iter()
            .all(|command| !command["argv"]
                .as_array()
                .unwrap()
                .contains(&json!("--offline"))),
        eq(true)
    );
    assert_that!(run.join("report.json").is_file(), eq(true));
    let missing = tempdir().unwrap();
    let result = Command::new(verifier())
        .args([
            "--package",
            "/missing/package",
            "--contract",
            "/missing/contract.json",
            "--cargo",
            "/missing/cargo",
            "--rustc",
            "/missing/rustc",
            "--sysroot",
            "/missing/sysroot",
            "--output-root",
            missing.path().join("outputs").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_that!(result.status.code(), eq(Some(1)));
    assert_that!(
        String::from_utf8_lossy(&result.stderr).as_ref(),
        contains_substring("retained report")
    );
    let path = fs::read_dir(missing.path().join("outputs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("report.json");
    let failed: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_that!(
        &failed["diagnostics"]["phase"],
        eq(&json!("input_fidelity"))
    );
    assert_that!(&failed["completed"], eq(&json!(false)));
}

#[gtest]
fn generated_manifest_rejects_undeclared_metadata_targets_dependencies_and_publisher_requirements()
{
    for (from, to) in [
        ("version='0.1.0'", "version='0.2.0'"),
        ("build=false", "build='build.rs'"),
        ("[lib]", "[workspace]\nmembers=['other']\n[lib]"),
        ("[lib]", "[features]\npublisher=[]\n[lib]"),
        ("[lib]", "[patch.crates-io]\nupstream={path='other'}\n[lib]"),
        ("[lib]", "[build-dependencies]\nupstream='=1.2.3'\n[lib]"),
        ("name='fixture_library'", "name='different'"),
        ("path='src/lib.rs'", "path='different.rs'"),
        (
            "path='src/lib.rs'",
            "path='src/lib.rs'\ncrate-type=['proc-macro']",
        ),
        ("name='second'", "name='different'"),
        ("edition='2021'", "edition='2024'"),
        ("[dependencies.friendly-name]", "[dependencies.different]"),
        ("version='=1.2.3'", "version='^1.2.3'"),
        ("default-features=false", "default-features=true"),
        ("features=['extra']", "features=[]"),
        ("package='upstream'", "package='upstream'\npath='other'"),
        ("package='upstream'", "package='upstream'\noptional=true"),
    ] {
        let mut fixture = Fixture::new("combined");
        let path = fixture.root.path().join("package/Cargo.toml");
        let manifest = fs::read_to_string(&path).unwrap().replace(from, to);
        fs::write(&path, &manifest).unwrap();
        fixture.contract["manifest_sha256"] =
            json!(cargo_publish_core::digest_bytes(manifest.as_bytes()));
        fixture.failure("", "generated_manifest", "retained report");
    }
}

#[gtest]
fn failing_second_binary_retains_prior_successful_build_and_failed_selector() {
    let fixture = Fixture::new("combined");
    let report = fixture.failure(
        "second_binary",
        "locked_builds",
        "second binary compilation failed",
    );
    let builds = report["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|command| command["phase"] == "locked_builds")
        .collect::<Vec<_>>();
    assert_that!(builds.len(), eq(2));
    assert_that!(&builds[0]["returncode"], eq(&json!(0)));
    assert_that!(&builds[1]["returncode"], eq(&json!(24)));
    assert_that!(
        builds[1]["argv"]
            .as_array()
            .unwrap()
            .contains(&json!("second")),
        eq(true)
    );
    assert_that!(
        &report["checks"]["locked_builds"]["completed"],
        eq(&json!(false))
    );
}

#[gtest]
fn normalized_binary_selection_rejects_missing_extra_duplicate_changed_paths_and_editions() {
    let binary = "[[bin]]\nname='second'\npath='src/second.rs'\nedition='2021'\n";
    for changed in [
        String::new(),
        binary.repeat(2),
        format!("{}{binary}", binary.replace("second", "extra")),
        binary.replace("name='second'", "name='different'"),
        binary.replace("src/second.rs", "missing.rs"),
        binary.replace("2021", "2024"),
    ] {
        let mut fixture = Fixture::new("combined");
        let manifest = fs::read_to_string(fixture.root.path().join("package/Cargo.toml")).unwrap();
        fixture.normalized = Some(manifest.replace(binary, &changed));
        fixture.failure("", "normalized_manifest", "selected binary");
    }
    let mut fixture = Fixture::new("combined");
    let path = fixture.root.path().join("package/Cargo.toml");
    let manifest = format!("{}{binary}", fs::read_to_string(&path).unwrap());
    fs::write(path, &manifest).unwrap();
    fixture.contract["manifest_sha256"] =
        json!(cargo_publish_core::digest_bytes(manifest.as_bytes()));
    fixture.failure("", "generated_manifest", "selected binary");
}

#[gtest]
fn valid_metadata_json_with_invalid_shape_fails_dependency_phase_after_retaining_raw_metadata() {
    let mut fixture = Fixture::new("library");
    fixture.metadata["packages"][0]["dependencies"] = json!("invalid declaration shape");
    let report = fixture.failure("", "direct_dependencies", "retained report");
    assert_that!(
        &report["checks"]["locked_metadata"]["status"],
        eq(&json!("passed"))
    );
    assert_that!(
        PathBuf::from(
            report["checks"]["locked_metadata"]["evidence"]["path"]
                .as_str()
                .unwrap()
        )
        .is_file(),
        eq(true)
    );
}

#[gtest]
fn packaged_lockfile_must_match_direct_registry_identities_and_checksums() {
    for mode in [
        "lock_checksum",
        "lock_version",
        "lock_source",
        "lock_missing_checksum",
        "lock_missing_dependency",
        "lock_duplicate_dependency",
    ] {
        Fixture::new("combined").failure(mode, "direct_dependencies", "packaged Cargo.lock");
    }
}

#[gtest]
fn retained_reports_redact_proxy_credentials_without_changing_child_environment() {
    let proxies = [
        (
            "HTTP_PROXY",
            "HTTP://fixture-user:fixture-password@[::1]:8080/path?transport=tcp#hint",
            "<redacted>",
        ),
        (
            "HTTPS_PROXY",
            "https://proxy.invalid:443",
            "https://proxy.invalid:443",
        ),
        (
            "ALL_PROXY",
            "socks5://fixture-user:fixture-password@proxy.invalid:1080",
            "<redacted>",
        ),
        (
            "http_proxy",
            "fixture-user:fixture-password@proxy.invalid:8080",
            "<redacted>",
        ),
        (
            "https_proxy",
            "socks5h://fixture-user:fixture-password@proxy.invalid:1080",
            "<redacted>",
        ),
        (
            "all_proxy",
            "socks5://fixture%40user:fixture%3Apassword@proxy.invalid:1080",
            "<redacted>",
        ),
    ];
    for (mode, expected_code) in [("", 0), ("metadata", 1)] {
        let mut fixture = Fixture::new("combined");
        fixture.proxy_environment = proxies
            .iter()
            .map(|(key, value, _)| (*key, *value))
            .collect();
        let (result, run, report) = fixture.invoke(mode);
        assert_that!(result.status.code(), eq(Some(expected_code)));
        for (key, _, expected) in proxies {
            assert_that!(
                &report["configuration"]["environment"][key],
                eq(&json!(expected))
            );
            assert_that!(
                &report["checks"]["configuration"]["evidence"]["environment"][key],
                eq(&json!(expected))
            );
        }
        let retained = fs::read_to_string(run.join("report.json")).unwrap();
        for credential in [
            "fixture-user",
            "fixture-password",
            "fixture%40user",
            "fixture%3Apassword",
        ] {
            assert_that!(retained.contains(credential), eq(false));
        }
    }
}
