//! Consumer publishing artifacts and ordinary dependency independence.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use cargo_publish_core::{
    DependencyKind, GenerationContract, PackageMetadata, digest_file, read_json,
};
use googletest::{assert_that, gtest, matchers::eq};
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
struct Library {
    name: String,
    path: String,
}
#[derive(Debug, Deserialize, PartialEq)]
struct Binary {
    name: String,
    path: String,
    edition: String,
}
#[derive(Deserialize)]
struct Manifest {
    package: PackageMetadata,
    lib: Library,
    bin: Vec<Binary>,
    dependencies: BTreeMap<String, Dependency>,
    #[serde(rename = "dev-dependencies")]
    dev_dependencies: BTreeMap<String, Dependency>,
}
fn root() -> PathBuf {
    env::var_os("PUBLISHING_GUARD_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("TEST_SRCDIR").unwrap())
                .join(env::var_os("TEST_WORKSPACE").unwrap())
        })
}
fn manifest() -> Manifest {
    toml::from_str(
        &fs::read_to_string(root().join("worktree_pool/publish_files.package/Cargo.toml")).unwrap(),
    )
    .unwrap()
}
#[gtest]
fn generated_identity_matches_compiled_public_cli_version() {
    let manifest = manifest();
    assert_that!(
        manifest.package,
        eq(&PackageMetadata {
            name: "worktree-pool".into(),
            version: "0.1.0".into(),
            edition: "2024".into(),
            rust_version: "1.96".into(),
            description: "Explicit ownership for persistent local Git worktrees".into(),
            license: "AGPL-3.0-only".into(),
            repository: "https://github.com/LowkeyLab/bazel-repo".into(),
            readme: "README.md".into(),
        })
    );
    assert_that!(
        manifest.lib,
        eq(&Library {
            name: "worktree_pool".into(),
            path: "lib.rs".into()
        })
    );
    assert_that!(
        manifest.bin,
        eq(&vec![Binary {
            name: "worktree-pool".into(),
            path: "main.rs".into(),
            edition: "2024".into()
        }])
    );
    let version = Command::new(root().join("worktree_pool/worktree_pool"))
        .arg("--version")
        .output()
        .unwrap();
    assert_that!(version.status.success(), eq(true));
    assert_that!(
        String::from_utf8(version.stdout).unwrap(),
        eq(&format!("worktree-pool {}\n", manifest.package.version))
    );
}

fn files(root: &Path) -> BTreeSet<String> {
    fn walk(root: &Path, current: &Path, result: &mut BTreeSet<String>) {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .to_owned(),
                );
            }
        }
    }
    let mut result = BTreeSet::new();
    walk(root, root, &mut result);
    result
}
fn expected_inputs() -> BTreeSet<String> {
    [
        "acquisition.rs",
        "acquisition_tests.rs",
        "acquisition_workflow.rs",
        "catalog.rs",
        "catalog_recovery_cli.rs",
        "cli.rs",
        "coordination.rs",
        "creation.rs",
        "creation_workflow.rs",
        "diagnostics.rs",
        "diagnostics_tests.rs",
        "domain.rs",
        "error.rs",
        "git.rs",
        "lib.rs",
        "main.rs",
        "management.rs",
        "paths.rs",
        "rebuild.rs",
        "rebuild_tests.rs",
        "recovery.rs",
        "recovery_inspection.rs",
        "recovery_workflow.rs",
        "release.rs",
        "release_workflow.rs",
        "relocation.rs",
        "relocation_events.rs",
        "relocation_tests.rs",
        "repository_recovery.rs",
        "resources.rs",
        "retirement.rs",
        "retirement_workflow.rs",
        "store.rs",
        "store_tests.rs",
        "workflows.rs",
        "README.md",
        "LICENSE",
        "CONTRIBUTING.md",
        "WIRE_SCHEMA.md",
        "DEPENDENCIES.md",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
#[gtest]
fn package_inventory_and_declared_asset_bytes_match_public_contract() {
    let root = root().join("worktree_pool");
    let tree = root.join("publish_files.package");
    let contract: GenerationContract = read_json(root.join("publish_files.json")).unwrap();
    contract.validate().unwrap();
    let expected = expected_inputs();
    assert_that!(
        contract
            .inputs
            .iter()
            .map(|item| item.path.to_string())
            .collect::<BTreeSet<_>>(),
        eq(&expected)
    );
    let mut all_files = expected.clone();
    all_files.insert("Cargo.toml".into());
    assert_that!(files(&tree), eq(&all_files));
    for asset in [
        "README.md",
        "LICENSE",
        "CONTRIBUTING.md",
        "WIRE_SCHEMA.md",
        "DEPENDENCIES.md",
    ] {
        assert_that!(
            fs::read(tree.join(asset)).unwrap(),
            eq(&fs::read(root.join(asset)).unwrap())
        );
    }
    for input in &contract.inputs {
        let path = tree.join(input.path.as_str());
        assert_that!(digest_file(&path).unwrap(), eq(&input.sha256));
        assert_that!(fs::metadata(path).unwrap().len(), eq(input.size));
    }
    assert_that!(
        contract
            .selection
            .tests
            .iter()
            .map(|test| test.label.rsplit(':').next().unwrap().to_owned())
            .collect::<BTreeSet<_>>(),
        eq(&BTreeSet::from([
            "worktree_pool_lib_test".into(),
            "cli_test".into()
        ]))
    );
    let manifest: toml::Table =
        toml::from_str(&fs::read_to_string(tree.join("Cargo.toml")).unwrap()).unwrap();
    assert_that!(manifest.contains_key("test"), eq(false));
}

#[derive(Debug, Deserialize, PartialEq)]
struct Dependency {
    package: String,
    version: String,
    #[serde(rename = "default-features")]
    default_features: bool,
    features: Vec<String>,
}
#[derive(Deserialize)]
struct Lock {
    package: Vec<LockedPackage>,
}
#[derive(Deserialize)]
struct LockedPackage {
    name: String,
    version: String,
    source: Option<String>,
}
#[gtest]
fn direct_dependencies_preserve_bazel_kinds_exact_versions_aliases_and_features() {
    let root = root();
    let manifest = manifest();
    let production = &manifest.dependencies;
    let development = &manifest.dev_dependencies;
    fn keys(map: &BTreeMap<String, Dependency>) -> BTreeSet<&str> {
        map.keys().map(String::as_str).collect()
    }
    assert_that!(
        keys(production),
        eq(&BTreeSet::from([
            "chrono",
            "clap",
            "cloudevents",
            "config",
            "redb",
            "serde",
            "serde_json",
            "sha2",
            "thiserror",
            "tracing",
            "tracing_subscriber",
            "uuid"
        ]))
    );
    assert_that!(
        keys(development),
        eq(&BTreeSet::from([
            "googletest",
            "insta",
            "proptest",
            "redb",
            "serde_json",
            "tempfile",
            "uuid"
        ]))
    );
    assert_that!(production["cloudevents"].package, eq("cloudevents-sdk"));
    assert_that!(
        production["tracing_subscriber"].package,
        eq("tracing-subscriber")
    );
    let locked: Lock =
        toml::from_str(&fs::read_to_string(root.join("Cargo.lock")).unwrap()).unwrap();
    for dependency in production.values().chain(development.values()) {
        let version = dependency
            .version
            .strip_prefix('=')
            .expect("exact dependency version");
        assert_that!(
            locked
                .package
                .iter()
                .filter(|p| p.name == dependency.package
                    && p.version == version
                    && p.source.as_deref()
                        == Some("registry+https://github.com/rust-lang/crates.io-index"))
                .count(),
            eq(1)
        );
    }
    for alias in ["redb", "serde_json", "uuid"] {
        assert_that!(production[alias], eq(&development[alias]));
    }
    for alias in ["config", "serde_json"] {
        assert_that!(production[alias].default_features, eq(true));
    }
    for alias in ["sha2", "cloudevents"] {
        assert_that!(production[alias].default_features, eq(false));
    }
    assert_that!(development["proptest"].default_features, eq(false));
    for (map, alias, feature) in [
        (production, "config", "json"),
        (production, "config", "toml"),
        (production, "config", "yaml"),
        (production, "uuid", "v7"),
        (production, "clap", "env"),
        (production, "serde", "derive"),
        (development, "proptest", "std"),
    ] {
        assert_that!(
            map[alias].features.iter().any(|item| item == feature),
            eq(true)
        );
    }
    let contract: GenerationContract =
        read_json(root.join("worktree_pool/publish_files.json")).unwrap();
    let expected: BTreeSet<_> = production
        .keys()
        .map(|alias| ("normal", alias.as_str()))
        .chain(development.keys().map(|alias| ("dev", alias.as_str())))
        .collect();
    assert_that!(
        contract
            .dependencies
            .iter()
            .map(|item| (
                match item.kind {
                    DependencyKind::Normal => "normal",
                    DependencyKind::Dev => "dev",
                },
                item.alias.as_str()
            ))
            .collect::<BTreeSet<_>>(),
        eq(&expected)
    );
    for dependency in &contract.dependencies {
        let map = match dependency.kind {
            DependencyKind::Normal => production,
            DependencyKind::Dev => development,
        };
        assert_that!(
            map[&dependency.alias],
            eq(&Dependency {
                package: dependency.package.clone(),
                version: format!("={}", dependency.version),
                default_features: dependency.default_features,
                features: dependency
                    .features
                    .iter()
                    .filter(|feature| feature.as_str() != "default")
                    .cloned()
                    .collect()
            })
        );
    }
}
#[gtest]
fn ordinary_library_binary_and_test_compilation_has_no_publishing_dependencies() {
    assert_that!(
        fs::read_to_string(root().join("worktree_pool/forbidden_compilation_dependencies"))
            .unwrap()
            .trim(),
        eq("")
    );
}
