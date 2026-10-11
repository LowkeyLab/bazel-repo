use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use cargo_publish_core::{Dependency, DependencyKind, GenerationContract, PackageMetadata};
use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    package: ManifestPackage,
    lib: Option<Target>,
    #[serde(default)]
    bin: Vec<Target>,
    #[serde(flatten)]
    other: BTreeMap<String, toml::Value>,
}
#[derive(Deserialize)]
struct ManifestPackage {
    #[serde(flatten)]
    metadata: PackageMetadata,
    build: Option<toml::Value>,
}
#[derive(Deserialize)]
struct Target {
    name: Option<String>,
    path: Option<String>,
    edition: Option<String>,
    #[serde(rename = "crate-type")]
    crate_type: Option<Vec<String>>,
}
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Requirement {
    pub(crate) package: String,
    pub(crate) version: Option<String>,
    pub(crate) default_features: bool,
    pub(crate) features: Vec<String>,
}

pub(crate) fn declared_requirements(
    dependencies: &[Dependency],
) -> Result<BTreeMap<(String, String), Requirement>> {
    let mut expected = BTreeMap::new();
    for dependency in dependencies {
        let kind = match dependency.kind {
            DependencyKind::Normal => "normal",
            DependencyKind::Dev => "dev",
        };
        let mut features: Vec<String> = dependency
            .features
            .iter()
            .filter(|feature| feature.as_str() != "default")
            .cloned()
            .collect();
        features.sort();
        ensure!(
            expected
                .insert(
                    (dependency.alias.clone(), kind.into()),
                    Requirement {
                        package: dependency.package.clone(),
                        version: Some(format!("={}", dependency.version)),
                        default_features: dependency.default_features,
                        features
                    }
                )
                .is_none(),
            "conflicting direct dependency declaration: {}",
            dependency.alias
        );
    }
    Ok(expected)
}

fn dependency_fields(value: &toml::Value, alias: &str) -> Result<Requirement> {
    if let Some(version) = value.as_str() {
        return Ok(Requirement {
            package: alias.into(),
            version: Some(version.into()),
            default_features: true,
            features: vec![],
        });
    }
    let table = value
        .as_table()
        .context("dependency must be a version or table")?;
    ensure!(
        ![
            "path",
            "workspace",
            "git",
            "registry",
            "registry-index",
            "branch",
            "tag",
            "rev"
        ]
        .iter()
        .any(|key| table.contains_key(*key))
            && table
                .get("optional")
                .is_none_or(|value| value.as_bool() == Some(false)),
        "unsupported dependency requirement: {alias}: {value}"
    );
    let string = |key: &str| -> Result<Option<String>> {
        table
            .get(key)
            .map(|value| {
                value
                    .as_str()
                    .map(ToOwned::to_owned)
                    .context("dependency string field")
            })
            .transpose()
    };
    let mut features = table
        .get("features")
        .map(|value| -> Result<Vec<String>> {
            value
                .as_array()
                .context("dependency features must be array")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(ToOwned::to_owned)
                        .context("dependency feature must be string")
                })
                .collect()
        })
        .transpose()?
        .unwrap_or_default();
    features.sort();
    let default_features = table
        .get("default-features")
        .map(|value| {
            value
                .as_bool()
                .context("dependency default-features must be bool")
        })
        .transpose()?
        .unwrap_or(true);
    Ok(Requirement {
        package: string("package")?.unwrap_or_else(|| alias.into()),
        version: string("version")?,
        default_features,
        features,
    })
}

fn truthy(value: &toml::Value) -> bool {
    match value {
        toml::Value::Table(value) => !value.is_empty(),
        toml::Value::Array(value) => !value.is_empty(),
        toml::Value::Boolean(value) => *value,
        toml::Value::String(value) => !value.is_empty(),
        _ => true,
    }
}

pub(crate) fn compare_manifest(path: &Path, contract: &GenerationContract) -> Result<()> {
    let manifest: Manifest = toml::from_str(&fs::read_to_string(path)?)
        .with_context(|| format!("parse {}", path.display()))?;
    ensure!(
        manifest.package.metadata == contract.package,
        "{}: package metadata differs from declared metadata",
        path.display()
    );
    for key in [
        "workspace",
        "patch",
        "replace",
        "target",
        "features",
        "build-dependencies",
        "test",
        "example",
        "bench",
    ] {
        ensure!(
            !manifest.other.get(key).is_some_and(truthy),
            "{}: unsupported {key} requirement",
            path.display()
        );
    }
    ensure!(
        manifest
            .package
            .build
            .is_none_or(|value| value == toml::Value::Boolean(false)),
        "{}: publisher build script is unsupported",
        path.display()
    );
    if let Some(library) = &contract.selection.library {
        let actual = manifest
            .lib
            .as_ref()
            .context("selected library is missing from manifest")?;
        ensure!(
            actual.name.as_deref() == Some(library.crate_name.as_str())
                && actual.path.as_deref() == Some(library.path.as_str())
                && actual
                    .edition
                    .as_deref()
                    .unwrap_or(&manifest.package.metadata.edition)
                    == library.edition,
            "{}: selected library name/path/edition differs: {}",
            path.display(),
            library.label
        );
        ensure!(
            actual
                .crate_type
                .as_ref()
                .is_none_or(|types| types == &["lib"] || types == &["rlib"]),
            "unsupported library crate-type"
        );
    } else {
        ensure!(manifest.lib.is_none(), "undeclared package library");
    }
    let expected_binaries: BTreeSet<_> = contract
        .selection
        .binaries
        .iter()
        .map(|binary| {
            (
                Some(binary.name.clone()),
                Some(binary.path.to_string()),
                binary.edition.clone(),
            )
        })
        .collect();
    let actual_binaries: BTreeSet<_> = manifest
        .bin
        .iter()
        .map(|binary| {
            (
                binary.name.clone(),
                binary.path.clone(),
                binary
                    .edition
                    .clone()
                    .unwrap_or_else(|| manifest.package.metadata.edition.clone()),
            )
        })
        .collect();
    ensure!(
        actual_binaries.len() == manifest.bin.len() && actual_binaries == expected_binaries,
        "{}: selected binary name/path/edition differs",
        path.display()
    );
    let expected = declared_requirements(&contract.dependencies)?;
    let mut observed = BTreeMap::new();
    for (kind, group) in [("normal", "dependencies"), ("dev", "dev-dependencies")] {
        if let Some(table) = manifest.other.get(group) {
            for (alias, dependency) in table.as_table().context("dependency group must be table")? {
                observed.insert(
                    (alias.clone(), kind.into()),
                    dependency_fields(dependency, alias)?,
                );
            }
        }
    }
    ensure!(
        observed == expected,
        "{}: dependencies identity/version/alias/kind/defaults/features differ: expected {expected:?}; actual {observed:?}",
        path.display()
    );
    Ok(())
}
