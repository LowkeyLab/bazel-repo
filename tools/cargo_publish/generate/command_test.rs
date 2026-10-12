//! Exercise the public offline generation executable with explicit real inputs.

use std::{
    env,
    fmt::Write,
    fs,
    path::PathBuf,
    process::{Command, Output},
};

use googletest::{
    assert_that, gtest,
    matchers::{contains_substring, eq},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";

struct Fixture {
    root: TempDir,
    spec: Value,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        for (path, bytes) in [
            ("lib.rs", "pub fn answer() -> u32 { 42 }\n"),
            ("README.md", "# Fixture\n"),
            (
                "Cargo.toml",
                "[package]\nname=\"catalog\"\nversion=\"0.1.0\"\n",
            ),
            ("Cargo.lock", "version=4\n"),
            ("MODULE.bazel", ""),
        ] {
            fs::write(root.path().join(path), bytes).unwrap();
        }
        let spec = json!({
            "schema_version":1,"target":"x86_64-unknown-linux-gnu",
            "package":{"name":"fixture","version":"0.1.0","edition":"2024","description":"A fixture","license":"MIT","repository":"https://example.com/repo","rust-version":"1.96","readme":"README.md"},
            "library":{"label":"//fixture:library","crate_name":"fixture","crate_type":"rlib","edition":"2024","path":"lib.rs","dependencies":[]},
            "binaries":[],"tests":[],"dependency_closure":[],
            "inputs":[{"path":"lib.rs","source":root.path().join("lib.rs")},{"path":"README.md","source":root.path().join("README.md")}]
        });
        Self { root, spec }
    }

    fn write(&self, path: &str, bytes: &str) {
        // Annotation fixtures use a real extension binding, including renamed proxies.
        let contents = if path == "MODULE.bazel" && !bytes.contains("use_extension(") {
            format!("crate = use_extension(\"@rules_rs//rs:extensions.bzl\", \"crate\")\n{bytes}")
        } else {
            bytes.to_owned()
        };
        fs::write(self.root.path().join(path), contents).unwrap();
    }

    fn run(&self, spec: &Value, suffix: &str) -> Output {
        let input = self.root.path().join(format!("analysis{suffix}.json"));
        fs::write(&input, serde_json::to_vec(spec).unwrap()).unwrap();
        let binary = PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
            .join(env::var_os("TEST_WORKSPACE").unwrap())
            .join(env!("GENERATOR"));
        Command::new(binary)
            .args([
                "--spec".into(),
                input,
                "--root-manifest".into(),
                self.root.path().join("Cargo.toml"),
                "--root-lockfile".into(),
                self.root.path().join("Cargo.lock"),
                "--module".into(),
                self.root.path().join("MODULE.bazel"),
                "--output".into(),
                self.root.path().join(format!("output{suffix}")),
                "--contract".into(),
                self.root.path().join(format!("contract{suffix}.json")),
            ])
            .output()
            .unwrap()
    }

    fn success(&self, spec: &Value, suffix: &str) -> Value {
        let result = self.run(spec, suffix);
        assert_that!(String::from_utf8_lossy(&result.stderr).as_ref(), eq(""));
        assert_that!(result.status.code(), eq(Some(0)));
        let text = fs::read_to_string(self.root.path().join(format!("output{suffix}/Cargo.toml")))
            .unwrap();
        let manifest: toml::Value = toml::from_str(&text).unwrap();
        serde_json::to_value(manifest).unwrap()
    }

    fn failure(&self, spec: &Value, suffix: &str, diagnostic: &str) -> String {
        let result = self.run(spec, suffix);
        let error = String::from_utf8(result.stderr).unwrap();
        assert_that!(result.status.success(), eq(false));
        assert_that!(error.as_str(), contains_substring(diagnostic));
        assert_that!(
            self.root.path().join(format!("output{suffix}")).exists(),
            eq(false)
        );
        assert_that!(
            self.root
                .path()
                .join(format!("contract{suffix}.json"))
                .exists(),
            eq(false)
        );
        error
    }

    fn contract(&self, suffix: &str) -> Value {
        serde_json::from_slice(
            &fs::read(self.root.path().join(format!("contract{suffix}.json"))).unwrap(),
        )
        .unwrap()
    }

    fn binary(&self) -> Value {
        self.write("main.rs", "fn main() {}\n");
        json!({"label":"//fixture:binary","crate_name":"bazel_binary","name":"published-cli","crate_type":"bin","edition":"2024","path":"main.rs","dependencies":[]})
    }

    fn binary_input(&self) -> Value {
        json!({"path":"main.rs","source":self.root.path().join("main.rs")})
    }

    fn upstream(&self, name: &str, alias: &str, version: &str) -> Value {
        let path = format!("{name}.toml");
        self.write(&path,&format!("[package]\nname=\"{name}\"\nversion=\"{version}\"\n[features]\ndefault=[]\nstd=[]\n"));
        json!({"label":format!("@crates//:{name}"),"external":true,"repository":format!("rules_rs++crate+crates__{name}-{version}"),"crate_name":name.replace('-',"_"),"crate_type":"rlib","version":version,"manifest":self.root.path().join(path),"features":["default","std"],"compiler_settings":{},"alias":alias})
    }

    fn locked(name: &str, version: &str) -> String {
        format!(
            "[[package]]\nname=\"{name}\"\nversion=\"{version}\"\nsource=\"{REGISTRY}\"\nchecksum=\"{}\"\n",
            "a".repeat(64)
        )
    }
}

#[gtest]
fn binary_only_declares_the_requested_cargo_target() {
    let mut f = Fixture::new();
    f.spec["library"] = Value::Null;
    f.spec["binaries"] = json!([f.binary()]);
    f.spec["inputs"] = json!([f.binary_input(), f.spec["inputs"][1]]);
    let manifest = f.success(&f.spec, "");
    assert_that!(manifest.get("lib").is_none(), eq(true));
    assert_that!(
        &manifest["bin"],
        eq(&json!([{"name":"published-cli","path":"main.rs","edition":"2024"}]))
    );
    let contract = f.contract("");
    assert_that!(contract["library"].is_null(), eq(true));
    assert_that!(
        &contract["binaries"][0]["name"],
        eq(&json!("published-cli"))
    );
}

#[gtest]
fn combined_package_keeps_the_selected_library_edge_internal() {
    let mut f = Fixture::new();
    let mut binary = f.binary();
    let mut library = f.spec["library"].clone();
    library["external"] = json!(false);
    library["alias"] = json!("fixture");
    binary["dependencies"] = json!([library]);
    f.spec["binaries"] = json!([binary]);
    let input = f.binary_input();
    f.spec["inputs"].as_array_mut().unwrap().push(input);
    let manifest = f.success(&f.spec, "");
    assert_that!(
        &manifest["lib"],
        eq(&json!({"name":"fixture","path":"lib.rs"}))
    );
    assert_that!(&manifest["bin"][0]["name"], eq(&json!("published-cli")));
    assert_that!(&manifest["dependencies"], eq(&json!({})));
}

#[gtest]
fn selected_tests_preserve_dev_only_and_shared_dependencies_and_reject_alias_conflicts() {
    let mut f = Fixture::new();
    let shared = f.upstream("anyhow", "errors", "1.0.100");
    let helper = f.upstream("test-helper", "test_helper", "1.0.100");
    f.write(
        "Cargo.toml",
        "[dependencies]\nanyhow=\"1.0.100\"\n[dev-dependencies]\ntest-helper=\"1.0.100\"\n",
    );
    f.write(
        "Cargo.lock",
        &format!(
            "version=4\n{}{}",
            Fixture::locked("anyhow", "1.0.100"),
            Fixture::locked("test-helper", "1.0.100")
        ),
    );
    f.spec["library"]["dependencies"] = json!([shared]);
    f.spec["tests"] = json!([{"label":"//fixture:unit_test","dependencies":[shared,helper]}]);
    f.spec["dependency_closure"] = json!([shared, helper]);
    let manifest = f.success(&f.spec, "");
    assert_that!(
        manifest["dependencies"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        eq(&vec!["errors"])
    );
    assert_that!(
        manifest["dev-dependencies"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        eq(&vec!["errors", "test_helper"])
    );
    assert_that!(
        &manifest["dependencies"]["errors"],
        eq(&manifest["dev-dependencies"]["errors"])
    );
    assert_that!(
        &manifest["dev-dependencies"]["test_helper"]["version"],
        eq(&json!("=1.0.100"))
    );
    assert_that!(manifest.get("test").is_none(), eq(true));
    let contract = f.contract("");
    assert_that!(
        contract["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| (d["kind"].as_str().unwrap(), d["alias"].as_str().unwrap()))
            .collect::<Vec<_>>(),
        eq(&vec![
            ("dev", "errors"),
            ("dev", "test_helper"),
            ("normal", "errors")
        ])
    );
    f.spec["tests"][0]["dependencies"][1]["alias"] = json!("errors");
    let error = f.failure(
        &f.spec,
        "alias_conflict",
        "conflicting direct dependency alias errors",
    );
    assert_that!(error.as_str(), contains_substring("//fixture:unit_test"));
}

#[gtest]
fn invalid_production_selections_fail_before_staging() {
    for case in [
        "empty",
        "platform",
        "type",
        "edition",
        "name",
        "duplicate",
        "root",
        "self_alias",
        "local",
    ] {
        let mut f = Fixture::new();
        let mut binary = f.binary();
        let input = f.binary_input();
        f.spec["inputs"].as_array_mut().unwrap().push(input);
        let diagnostic = match case {
            "empty" => {
                f.spec["library"] = Value::Null;
                "requires a library or binary"
            }
            "platform" => {
                f.spec["target"] = json!("wasm32-unknown-unknown");
                "unsupported target platform"
            }
            "type" => {
                f.spec["library"] = Value::Null;
                binary["crate_type"] = json!("proc-macro");
                f.spec["binaries"] = json!([binary]);
                "unsupported binary crate type"
            }
            "edition" => {
                f.spec["library"] = Value::Null;
                binary["edition"] = json!("2099");
                f.spec["binaries"] = json!([binary]);
                "unsupported binary edition"
            }
            "name" => {
                f.spec["library"] = Value::Null;
                binary["name"] = json!("../outside");
                f.spec["binaries"] = json!([binary]);
                "Cargo binary name"
            }
            "duplicate" => {
                f.spec["library"] = Value::Null;
                let mut other = binary.clone();
                other["label"] = json!("//fixture:other");
                f.spec["binaries"] = json!([binary, other]);
                "conflicting Cargo binary name"
            }
            "root" => {
                f.spec["library"] = Value::Null;
                binary["path"] = json!("missing.rs");
                f.spec["binaries"] = json!([binary]);
                "missing required package path: missing.rs"
            }
            "self_alias" | "local" => {
                let mut library = f.spec["library"].clone();
                library["external"] = json!(false);
                library["alias"] = json!(if case == "local" { "other" } else { "renamed" });
                if case == "local" {
                    library["label"] = json!("//other:library");
                }
                binary["dependencies"] = json!([library]);
                f.spec["binaries"] = json!([binary]);
                if case == "local" {
                    "separate local dependency packages"
                } else {
                    "selected library alias"
                }
            }
            _ => unreachable!(),
        };
        f.failure(&f.spec, case, diagnostic);
    }
}

#[gtest]
fn contract_fingerprints_declared_catalog_inputs() {
    let f = Fixture::new();
    f.success(&f.spec, "");
    let contract = f.contract("");
    for (key, path) in [
        ("root_manifest_sha256", "Cargo.toml"),
        ("root_lockfile_sha256", "Cargo.lock"),
        ("module_sha256", "MODULE.bazel"),
    ] {
        let digest = Sha256::digest(fs::read(f.root.path().join(path)).unwrap())
            .iter()
            .fold(String::new(), |mut hex, byte| {
                write!(hex, "{byte:02x}").unwrap();
                hex
            });
        assert_that!(&contract["provenance"][key], eq(&json!(digest)));
    }
    assert_that!(
        fs::read_to_string(f.root.path().join("output/lib.rs")).unwrap(),
        eq("pub fn answer() -> u32 { 42 }\n")
    );
}

#[gtest]
fn malformed_or_missing_required_metadata_fails_offline() {
    for (key, value) in [
        ("version", "not-a-version"),
        ("version", "1.2.3-alpha.01"),
        ("name", "../bad"),
        ("edition", "2099"),
        ("rust-version", "latest"),
        ("description", ""),
    ] {
        let mut f = Fixture::new();
        f.spec["package"][key] = json!(value);
        let error = f.failure(&f.spec, "invalid_metadata", key);
        assert_that!(error.as_str(), contains_substring("//fixture:library"));
    }
}

#[gtest]
fn missing_escaping_and_conflicting_declared_inputs_fail_before_staging() {
    for case in ["escaping", "missing", "conflicting", "prefix", "required"] {
        let mut f = Fixture::new();
        let diagnostic = match case {
            "escaping" => {
                f.spec["inputs"] =
                    json!([{"path":"../outside","source":f.root.path().join("lib.rs")}]);
                "escape"
            }
            "missing" => {
                f.spec["inputs"] = json!([{"path":"lib.rs","source":f.root.path().join("absent")}]);
                "missing"
            }
            "conflicting" => {
                f.spec["inputs"] = json!([f.spec["inputs"][0], f.spec["inputs"][0]]);
                "conflicting"
            }
            "prefix" => {
                f.spec["inputs"] = json!([f.spec["inputs"][0],{"path":"lib.rs/README.md","source":f.root.path().join("README.md")}]);
                "file/directory"
            }
            "required" => {
                f.spec["inputs"] = json!([f.spec["inputs"][0]]);
                "README.md"
            }
            _ => unreachable!(),
        };
        f.failure(&f.spec, case, diagnostic);
    }
}

#[gtest]
fn ambiguous_and_unsupported_dependency_provenance_fails_offline() {
    for (case, diagnostic) in [
        ("local", "local dependency"),
        ("version", "identity/version"),
        ("missing_metadata", "missing Cargo manifest"),
        ("custom_repository", "repository provenance"),
        ("features", "configured features"),
        ("git", "crates.io lock provenance"),
        ("custom_registry", "crates.io lock provenance"),
        ("root_git", "root dependency provenance"),
        ("root_registry", "root dependency provenance"),
        ("ambiguous_lock", "ambiguous"),
        ("missing_catalog", "root Cargo manifest"),
        ("select_flags", "annotation/override"),
        ("select_environment", "annotation/override"),
        ("patch", "annotation/override"),
        ("override", "annotation/override"),
    ] {
        let mut f = Fixture::new();
        let mut metadata = f.upstream("anyhow", "friendly", "1.0.104");
        match case {
            "local" => metadata["external"] = json!(false),
            "version" => metadata["version"] = json!("1.0.103"),
            "missing_metadata" => metadata["manifest"] = Value::Null,
            "custom_repository" => metadata["repository"] = json!("custom"),
            "features" => metadata["features"] = json!(["unsupported"]),
            _ => {}
        }
        let catalog = match case {
            "missing_catalog" => String::new(),
            "root_git" => {
                "[dependencies]\nanyhow={version=\"1.0.100\",git=\"https://example.com/repo\"}\n"
                    .into()
            }
            "root_registry" => {
                "[dependencies]\nanyhow={version=\"1.0.100\",registry=\"private\"}\n".into()
            }
            _ => "[dependencies]\nanyhow=\"1.0.100\"\n".into(),
        };
        f.write("Cargo.toml", &catalog);
        let locked = Fixture::locked("anyhow", "1.0.104");
        f.write(
            "Cargo.lock",
            &match case {
                "git" => locked.replace(REGISTRY, "git+https://example.com/repo"),
                "custom_registry" => locked.replace(REGISTRY, "registry+https://example.com/index"),
                "ambiguous_lock" => locked.repeat(2),
                _ => locked,
            },
        );
        f.write("MODULE.bazel",match case {
            "select_flags"=>"crate.annotation_select(crate=\"anyhow\",triples=[\"x86_64-unknown-linux-gnu\"],rustc_flags=[\"--cfg=custom\"])",
            "select_environment"=>"crate.annotation_select(crate=\"anyhow\",triples=[\"x86_64-unknown-linux-gnu\"],build_script_env={\"CUSTOM\":\"value\"})",
            "patch"=>"crate.annotation(crate=\"anyhow\",patches=[\"//fixture:patch\"])",
            "override"=>"crate.override(crate=\"anyhow\",path=\"//fixture\")",
            _=>"",
        });
        f.spec["library"]["dependencies"] = json!([metadata]);
        f.spec["dependency_closure"] = json!([metadata]);
        let error = f.failure(&f.spec, case, diagnostic);
        assert_that!(error.as_str(), contains_substring("anyhow"));
    }
}

#[gtest]
fn cargo_native_and_inactive_annotations_remain_supported() {
    for (index,annotation) in [
        "crate.annotation(crate=\"anyhow\",gen_build_script=\"on\",crate_features=[\"std\"])",
        "crate.annotation(crate=\"anyhow\",version=\"1.0.103\",patches=[\"//fixture:patch\"])",
        "crate.annotation(crate=\"anyhow\",repositories=[\"another_hub\"],patches=[\"//fixture:patch\"])",
        "crate.annotation_select(crate=\"anyhow\",triples=[\"aarch64-apple-darwin\"],rustc_flags=[\"--cfg=custom\"])",
        "crate.annotation_select(crate=\"anyhow\",triples=[\"x86_64-unknown-linux-gnu\"],crate_features=[\"std\"])",
    ].iter().enumerate() {
        let mut f=Fixture::new();let metadata=f.upstream("anyhow","friendly","1.0.104");
        f.write("Cargo.toml","[dependencies]\nanyhow=\"1.0.100\"\n");f.write("Cargo.lock",&Fixture::locked("anyhow","1.0.104"));f.write("MODULE.bazel",annotation);
        f.spec["library"]["dependencies"]=json!([metadata]);f.spec["dependency_closure"]=json!([metadata]);
        f.success(&f.spec,&format!("supported{index}"));
    }
}

#[gtest]
fn module_comments_strings_and_literal_precedence_preserve_supported_packages() {
    for (index, module) in [
        "crate.annotation(crate=\"unrelated\",patches={1:\"unused\"},patch_strip=-1)\n",
        "crate.annotation(crate=r\"anyhow\",gen_build_script=\"on\",crate_features=[\"std\"])\n",
        "# crate.annotation(crate=\"anyhow\",patches=[\"bad\"])\ntext = '''crate.override(crate=\"anyhow\",path=\"bad\")'''\n",
        "crate.annotation(crate=\"anyhow\",patches=[\"bad\"])\ncrate.annotation(crate=\"anyhow\",version=\"1.0.104\",crate_features=[\"std\"])\n",
        "crate.annotation_select(crate=\"anyhow\",triples=[\"x86_64-unknown-linux-gnu\"],crate_features=[\"std\"])\ncrate.annotation(crate=\"anyhow\",version=\"1.0.104\",gen_build_script=\"auto\")\n",
    ].iter().enumerate() {
        let mut f = Fixture::new();
        let metadata = f.upstream("anyhow", "friendly", "1.0.104");
        f.write("Cargo.toml", "[dependencies]\nanyhow=\"1.0.100\"\n");
        f.write("Cargo.lock", &Fixture::locked("anyhow", "1.0.104"));
        f.write("MODULE.bazel", module);
        f.spec["library"]["dependencies"] = json!([metadata]);
        f.spec["dependency_closure"] = json!([metadata]);
        f.success(&f.spec, &format!("literal{index}"));
    }
}

#[gtest]
fn nonliteral_module_configuration_and_composed_source_mutations_fail_before_staging() {
    for (index, module, diagnostic) in [
        (
            0,
            "crate.annotation(crate=choose_crate(),patches=[])",
            "ambiguous crate annotation/override configuration",
        ),
        (
            1,
            "crate.annotation(crate=\"anyhow\",**settings)",
            "ambiguous crate annotation/override identity",
        ),
        (
            2,
            "crate.annotation_select(crate=\"anyhow\",triples=[\"x86_64-unknown-linux-gnu\"],patches=[\"wildcard\"])\ncrate.annotation_select(crate=\"anyhow\",version=\"1.0.104\",triples=[\"x86_64-unknown-linux-gnu\"],crate_features=[\"std\"])\n",
            "annotation/override",
        ),
        (
            3,
            "crate.annotation_select(crate=\"anyhow\",triples=[\"x86_64-unknown-linux-gnu\"],crate_features=[\"std\"])\ncrate.annotation_select(crate=\"anyhow\",version=\"1.0.104\",triples=[\"x86_64-unknown-linux-gnu\"],rustc_flags=[\"--cfg=custom\"])\n",
            "annotation/override",
        ),
    ] {
        let mut f = Fixture::new();
        let metadata = f.upstream("anyhow", "friendly", "1.0.104");
        f.write("Cargo.toml", "[dependencies]\nanyhow=\"1.0.100\"\n");
        f.write("Cargo.lock", &Fixture::locked("anyhow", "1.0.104"));
        f.write("MODULE.bazel", module);
        f.spec["library"]["dependencies"] = json!([metadata]);
        f.spec["dependency_closure"] = json!([metadata]);
        f.failure(&f.spec, &format!("nonliteral{index}"), diagnostic);
    }
}

#[gtest]
fn declared_asset_paths_remain_literal_in_cargo_include_patterns() {
    let mut f = Fixture::new();
    for path in [
        "assets/[draft].txt",
        "assets/{draft,final}.txt",
        "assets/star*.txt",
        "assets/question?.txt",
        "!literal.txt",
        "#literal.txt",
        "assets/trailing ",
    ] {
        f.spec["inputs"]
            .as_array_mut()
            .unwrap()
            .push(json!({"path":path,"source":f.root.path().join("README.md")}));
    }
    let manifest = f.success(&f.spec, "literal_assets");
    assert_that!(
        &manifest["package"]["include"],
        eq(&json!([
            "/Cargo.toml",
            "/!literal.txt",
            "/#literal.txt",
            "/README.md",
            "/assets/\\[draft\\].txt",
            "/assets/question\\?.txt",
            "/assets/star\\*.txt",
            "/assets/trailing\\ ",
            "/assets/\\{draft,final\\}.txt",
            "/lib.rs"
        ]))
    );
    let contract = f.contract("literal_assets");
    assert_that!(
        contract["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|input| input["path"] == "assets/[draft].txt"),
        eq(true)
    );
    assert_that!(
        fs::read_to_string(
            f.root
                .path()
                .join("outputliteral_assets/assets/[draft].txt")
        )
        .unwrap(),
        eq("# Fixture\n")
    );
}

#[gtest]
fn selected_dependencies_declared_only_under_target_tables_are_publishable() {
    for (index, target) in ["cfg(unix)", "x86_64-unknown-linux-gnu"].iter().enumerate() {
        let mut f = Fixture::new();
        let shared = f.upstream("anyhow", "errors", "1.0.104");
        let helper = f.upstream("test-helper", "test_helper", "1.0.100");
        f.spec["library"]["dependencies"] = json!([shared]);
        f.spec["tests"] = json!([{"label":"//fixture:unit_test","dependencies":[shared,helper]}]);
        f.spec["dependency_closure"] = json!([shared, helper]);
        let root = format!(
            "[target.'{target}'.dependencies]\nrenamed={{package=\"anyhow\",version=\"1.0.104\"}}\n[target.'{target}'.dev-dependencies]\ntest-helper=\"1.0.100\"\n[target.'cfg(windows)'.dependencies]\nwindows-only=\"0.1.0\"\n"
        );
        f.write("Cargo.toml", &root);
        f.write(
            "Cargo.lock",
            &format!(
                "{}{}",
                Fixture::locked("anyhow", "1.0.104"),
                Fixture::locked("test-helper", "1.0.100")
            ),
        );
        let manifest = f.success(&f.spec, &format!("target{index}"));
        assert_that!(
            &manifest["dependencies"]["errors"]["version"],
            eq(&json!("=1.0.104"))
        );
        assert_that!(
            &manifest["dev-dependencies"]["test_helper"]["version"],
            eq(&json!("=1.0.100"))
        );
        assert_that!(
            manifest["dependencies"].get("windows-only").is_none(),
            eq(true)
        );
        assert_that!(
            &manifest["dependencies"]["errors"],
            eq(&manifest["dev-dependencies"]["errors"])
        );
        f.write(
            "Cargo.toml",
            &root.replace(
                "package=\"anyhow\",",
                "package=\"anyhow\",git=\"https://example.com/unsupported\",",
            ),
        );
        f.failure(
            &f.spec,
            &format!("target_git{index}"),
            "unsupported/ambiguous root dependency provenance",
        );
    }
}

#[gtest]
fn unrelated_extension_calls_do_not_affect_rust_package_generation() {
    for (index, module) in [
        "tool = use_extension(\"@other//:extension.bzl\", \"crate\")\ntool.override(name=\"replacement\")\n",
        "tool = use_extension(\"@other//:extension.bzl\", \"tools\")\ntool.annotation(name=choose_name(), **settings)\n",
        "rust_packages = use_extension(\"@rules_rs//rs:extensions.bzl\", \"crate\")\nrust_packages.annotation(crate=\"anyhow\",gen_build_script=\"on\")\ntool = use_extension(\"@other//:extension.bzl\", \"tools\")\ntool.annotation(crate=\"anyhow\",patches=[\"unrelated\"])\n",
    ].iter().enumerate() {
        let mut f = Fixture::new();
        let dependency = f.upstream("anyhow", "friendly", "1.0.104");
        f.spec["library"]["dependencies"] = json!([dependency]);
        f.spec["dependency_closure"] = json!([dependency]);
        f.write("Cargo.toml", "[dependencies]\nanyhow=\"1.0.104\"\n");
        f.write("Cargo.lock", &Fixture::locked("anyhow", "1.0.104"));
        f.write("MODULE.bazel", module);
        let manifest = f.success(&f.spec, &format!("unrelated{index}"));
        assert_that!(&manifest["dependencies"]["friendly"]["version"], eq(&json!("=1.0.104")));
        f.write("MODULE.bazel", &format!("{module}\nselected = use_extension(\"@rules_rs//rs:extensions.bzl\", \"crate\")\nselected.annotation(crate=\"anyhow\",patches=[\"//fixture:unsupported\"])"));
        f.failure(&f.spec, &format!("selected_mutation{index}"), "unsupported Bazel annotation/override");
    }
}

#[gtest]
fn keyword_extension_bindings_preserve_crate_mutation_rejection() {
    for (index, binding) in [
        "rust_packages = use_extension(extension_bzl_file=\"@rules_rs//rs:extensions.bzl\", extension_name=\"crate\")",
        "rust_packages = use_extension(extension_name=\"crate\", extension_bzl_file=\"@rules_rs//rs:extensions.bzl\")",
        "rust_packages = use_extension(\"@rules_rs//rs:extensions.bzl\", extension_name=\"crate\", dev_dependency=True)",
    ].iter().enumerate() {
        let mut f = Fixture::new();
        let metadata = f.upstream("anyhow", "friendly", "1.0.104");
        f.spec["library"]["dependencies"] = json!([metadata]);
        f.spec["dependency_closure"] = json!([metadata]);
        f.write("Cargo.toml", "[dependencies]\nanyhow=\"1.0.104\"\n");
        f.write("Cargo.lock", &Fixture::locked("anyhow","1.0.104"));
        f.write("MODULE.bazel", &format!("{binding}\nrust_packages.annotation(crate=\"anyhow\",patches=[\"unsupported\"])"));
        f.failure(&f.spec, &format!("keyword{index}"), "unsupported Bazel annotation/override");
    }
}

#[gtest]
fn unresolved_extension_identities_and_proxy_aliases_cannot_hide_mutations() {
    for (index, module, diagnostic) in [
        (
            0,
            "EXT = \"@rules_rs//rs:extensions.bzl\"\ncrate = use_extension(EXT, \"crate\")\ncrate.annotation(crate=\"anyhow\",patches=[\"unsupported\"])",
            "ambiguous crate extension binding",
        ),
        (
            1,
            "crate = use_extension(\"@rules_rs//rs:extensions.bzl\", \"crate\")\nforwarded = crate\nforwarded.annotation(crate=\"anyhow\",patches=[\"unsupported\"])",
            "unsupported Bazel annotation/override",
        ),
        (
            2,
            "crate = use_extension(str(\"@rules_rs//rs:extensions.bzl\"), \"crate\")\ncrate.annotation(crate=\"anyhow\",patches=[\"unsupported\"])",
            "ambiguous crate extension binding",
        ),
        (
            3,
            "other = use_extension(\"@other//:extensions.bzl\", \"tools\")\ncrate = use_extension(\"@rules_rs//rs:extensions.bzl\", \"crate\")\nforwarded = other if False else crate\nforwarded.annotation(crate=\"anyhow\",patches=[\"unsupported\"])",
            "ambiguous crate extension binding",
        ),
        (
            4,
            "crate = use_extension(\"@rules_rs//rs:extensions.bzl\", \"crate\")\nforwarded = use_extension(\"@other//:extensions.bzl\", \"tools\") if False else crate\nforwarded.annotation(crate=\"anyhow\",patches=[\"unsupported\"])",
            "ambiguous crate extension binding",
        ),
    ] {
        let mut f = Fixture::new();
        let metadata = f.upstream("anyhow", "friendly", "1.0.104");
        f.spec["library"]["dependencies"] = json!([metadata]);
        f.spec["dependency_closure"] = json!([metadata]);
        f.write("Cargo.toml", "[dependencies]\nanyhow=\"1.0.104\"\n");
        f.write("Cargo.lock", &Fixture::locked("anyhow", "1.0.104"));
        f.write("MODULE.bazel", module);
        f.failure(&f.spec, &format!("receiver{index}"), diagnostic);
    }
    let f = Fixture::new();
    f.write("MODULE.bazel", "original = use_extension(\"@other//:extensions.bzl\", \"tools\")\nforwarded = original\nforwarded.override(name=\"replacement\")");
    f.success(&f.spec, "other_alias");
}
