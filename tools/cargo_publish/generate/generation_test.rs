//! Public macro output contracts, independent of the generator implementation.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use googletest::{assert_that, gtest, matchers::eq};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn fixtures() -> PathBuf {
    PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
        .join(env::var_os("TEST_WORKSPACE").unwrap())
        .join("tools/cargo_publish/fixtures")
}

fn manifest(package: &str) -> Value {
    let text =
        fs::read_to_string(fixtures().join(format!("{package}_files.package/Cargo.toml"))).unwrap();
    let parsed: toml::Value = toml::from_str(&text).unwrap();
    serde_json::to_value(parsed).unwrap()
}

fn contract(package: &str) -> Value {
    serde_json::from_slice(&fs::read(fixtures().join(format!("{package}_files.json"))).unwrap())
        .unwrap()
}

fn keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

fn files(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(files(&path));
        } else {
            result.push(path);
        }
    }
    result.sort();
    result
}

fn assert_identical(first: &str, second: &str) {
    let root = fixtures();
    assert_that!(
        fs::read(root.join(format!("{first}_files.json"))).unwrap(),
        eq(&fs::read(root.join(format!("{second}_files.json"))).unwrap())
    );
    let first = root.join(format!("{first}_files.package"));
    let second = root.join(format!("{second}_files.package"));
    let paths = files(&first);
    assert_that!(
        paths
            .iter()
            .map(|p| p.strip_prefix(&first).unwrap().to_path_buf())
            .collect::<Vec<_>>(),
        eq(&files(&second)
            .iter()
            .map(|p| p.strip_prefix(&second).unwrap().to_path_buf())
            .collect::<Vec<_>>())
    );
    for path in paths {
        assert_that!(
            fs::read(&path).unwrap(),
            eq(&fs::read(second.join(path.strip_prefix(&first).unwrap())).unwrap())
        );
    }
}

#[gtest]
fn binary_selection_preserves_mapped_name_and_configured_dependency() {
    let manifest = manifest("binary_publish");
    assert_that!(manifest.get("lib").is_none(), eq(true));
    assert_that!(
        &manifest["bin"],
        eq(&json!([{"name":"fixture-cli","path":"src/bin/main.rs","edition":"2024"}]))
    );
    assert_that!(
        &manifest["dependencies"]["anyhow"]["version"],
        eq(&json!("=1.0.104"))
    );
    let contract = contract("binary_publish");
    assert_that!(contract["library"].is_null(), eq(true));
    assert_that!(
        &contract["binaries"][0]["crate_name"],
        eq(&json!("bazel_binary"))
    );
}

#[gtest]
fn combined_selection_keeps_dependency_kinds_without_test_runtime_wiring() {
    let manifest = manifest("combined_publish");
    assert_that!(
        &manifest["lib"],
        eq(&json!({"name":"fixture_library","path":"src/lib.rs"}))
    );
    assert_that!(
        manifest["bin"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| (b["name"].as_str().unwrap(), b["edition"].as_str().unwrap()))
            .collect::<Vec<_>>(),
        eq(&vec![("fixture-cli", "2024"), ("fixture-other", "2021")])
    );
    assert_that!(
        keys(&manifest["dependencies"]),
        eq(&vec!["anyhow", "clap", "renamed_json", "sha2"])
    );
    assert_that!(
        keys(&manifest["dev-dependencies"]),
        eq(&vec!["anyhow", "googletest", "renamed_json", "tempfile"])
    );
    assert_that!(
        &manifest["dependencies"]["anyhow"],
        eq(&manifest["dev-dependencies"]["anyhow"])
    );
    assert_that!(
        &manifest["dependencies"]["renamed_json"]["default-features"],
        eq(&json!(true))
    );
    assert_that!(
        manifest["dependencies"]["clap"]["features"]
            .as_array()
            .unwrap()
            .contains(&json!("derive")),
        eq(true)
    );
    assert_that!(
        manifest["dev-dependencies"]["tempfile"]["features"]
            .as_array()
            .unwrap()
            .contains(&json!("getrandom")),
        eq(true)
    );
    assert_that!(
        &manifest["dev-dependencies"]["googletest"]["default-features"],
        eq(&json!(true))
    );
    let tree = fixtures().join("combined_publish_files.package");
    assert_that!(tree.join("src/tests/helper.rs").exists(), eq(false));
    assert_that!(tree.join("src/tests/cli.rs").exists(), eq(false));
    assert_that!(manifest.get("test").is_none(), eq(true));
    let contract = contract("combined_publish");
    let mut kinds = contract["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["kind"].as_str().unwrap())
        .collect::<Vec<_>>();
    kinds.sort_unstable();
    kinds.dedup();
    assert_that!(kinds, eq(&vec!["dev", "normal"]));
    assert_that!(contract["tests"].as_array().unwrap().len(), eq(2));
    assert_identical("combined_publish", "combined_repeat");
}

#[gtest]
fn library_has_standalone_exact_dependencies_and_no_generated_lockfile() {
    let manifest = manifest("publish");
    assert_that!(
        &manifest["package"]["name"],
        eq(&json!("publishing-fixture"))
    );
    assert_that!(&manifest["package"]["edition"], eq(&json!("2024")));
    assert_that!(
        &manifest["lib"],
        eq(&json!({"name":"fixture_library","path":"src/lib.rs"}))
    );
    assert_that!(
        &manifest["dependencies"]["anyhow"]["version"],
        eq(&json!("=1.0.104"))
    );
    assert_that!(
        fixtures().join("publish_files.package/Cargo.lock").exists(),
        eq(false)
    );
}

#[gtest]
fn alias_preserves_configured_default_and_explicit_features() {
    let manifest = manifest("publish");
    let dependency = &manifest["dependencies"]["renamed_json"];
    assert_that!(&dependency["package"], eq(&json!("serde_json")));
    assert_that!(&dependency["version"], eq(&json!("=1.0.154")));
    assert_that!(&dependency["default-features"], eq(&json!(true)));
    for feature in ["raw_value", "unbounded_depth"] {
        assert_that!(
            dependency["features"]
                .as_array()
                .unwrap()
                .contains(&json!(feature)),
            eq(true)
        );
    }
    assert_that!(
        manifest["dependencies"].get("serde_json").is_none(),
        eq(true)
    );
}

#[gtest]
fn declared_nested_sources_and_mapped_assets_are_staged_and_fingerprinted() {
    let tree = fixtures().join("publish_files.package");
    assert_that!(
        files(&tree)
            .iter()
            .map(|p| p.strip_prefix(&tree).unwrap().to_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        eq(&vec![
            "!literal.txt",
            "#literal.txt",
            "Cargo.toml",
            "README.md",
            "assets/[draft].txt",
            "assets/license.txt",
            "assets/trailing ",
            "assets/{draft,final}.txt",
            "src/lib.rs",
            "src/nested/value.rs"
        ])
    );
    assert_that!(
        fs::read_to_string(tree.join("src/nested/value.rs")).unwrap(),
        eq("pub const ANSWER: u32 = 42;\n")
    );
    assert_that!(
        fs::read_to_string(tree.join("assets/license.txt")).unwrap(),
        eq("Fixture license asset.\n")
    );
    for path in [
        "!literal.txt",
        "#literal.txt",
        "assets/[draft].txt",
        "assets/trailing ",
        "assets/{draft,final}.txt",
    ] {
        assert_that!(
            fs::read_to_string(tree.join(path)).unwrap(),
            eq("Fixture license asset.\n")
        );
    }
    assert_that!(
        manifest("publish")["package"]["include"]
            .as_array()
            .unwrap()
            .contains(&json!("/assets/\\[draft\\].txt")),
        eq(true)
    );
    let contract = contract("publish");
    assert_that!(&contract["schema_version"], eq(&json!(1)));
    assert_that!(&contract["target"], eq(&json!("x86_64-unknown-linux-gnu")));
    for item in contract["inputs"].as_array().unwrap() {
        let bytes = fs::read(tree.join(item["path"].as_str().unwrap())).unwrap();
        assert_that!(
            &item["sha256"],
            eq(&json!(
                Sha256::digest(&bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            ))
        );
        assert_that!(&item["size"], eq(&json!(bytes.len())));
    }
}

#[gtest]
fn dependency_with_disabled_defaults_remains_disabled() {
    assert_that!(
        &manifest("publish")["dependencies"]["sha2"],
        eq(&json!({"package":"sha2","version":"=0.11.0","default-features":false,"features":[]}))
    );
}

#[gtest]
fn identical_public_macro_inputs_generate_identical_files() {
    assert_identical("publish", "repeat");
}
