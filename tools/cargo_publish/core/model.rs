use serde::{Deserialize, Serialize};

use crate::{ContractError, PackagePath, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const TARGET: &str = "x86_64-unknown-linux-gnu";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository: String,
    #[serde(
        rename = "rust-version",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub rust_version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub readme: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub edition: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LibraryTarget {
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub crate_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub crate_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub edition: String,
    pub path: PackagePath,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BinaryTarget {
    pub label: String,
    pub crate_name: String,
    pub name: String,
    pub crate_type: String,
    pub edition: String,
    pub path: PackagePath,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TestTarget {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crate_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crate_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition: Option<String>,
    #[serde(
        rename = "crate",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "nullable_string"
    )]
    pub crate_: Option<Option<String>>,
}

pub(crate) fn nullable_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    #[serde(default)]
    pub library: Option<LibraryTarget>,
    #[serde(default)]
    pub binaries: Vec<BinaryTarget>,
    #[serde(default)]
    pub tests: Vec<TestTarget>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectedTarget {
    Library(String),
    Binary(String),
}

impl std::fmt::Display for SelectedTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Library(name) => write!(formatter, "lib:{name}"),
            Self::Binary(name) => write!(formatter, "bin:{name}"),
        }
    }
}

impl Selection {
    pub fn selected_targets(&self) -> Result<Vec<SelectedTarget>, ContractError> {
        let mut targets = Vec::new();
        if let Some(library) = &self.library {
            targets.push(SelectedTarget::Library(library.crate_name.clone()));
        }
        targets.extend(
            self.binaries
                .iter()
                .map(|binary| SelectedTarget::Binary(binary.name.clone())),
        );
        targets.sort_by_key(ToString::to_string);
        if targets.is_empty()
            || targets.iter().any(|target| match target {
                SelectedTarget::Library(name) | SelectedTarget::Binary(name) => name.is_empty(),
            })
            || targets.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(ContractError::Policy(
                "missing or conflicting selected library/binary targets".into(),
            ));
        }
        Ok(targets)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyKind {
    Normal,
    Dev,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub target: String,
    pub package: String,
    pub version: String,
    pub source: String,
    /// Opaque upstream registry checksum; package byte fingerprints use Sha256.
    pub checksum: String,
    pub features: Vec<String>,
    pub default_features: bool,
    pub alias: String,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InputFile {
    pub path: PackagePath,
    pub sha256: Sha256,
    pub size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub root_manifest_sha256: Sha256,
    pub root_lockfile_sha256: Sha256,
    pub module_sha256: Sha256,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GenerationContract {
    pub schema_version: u32,
    pub package: PackageMetadata,
    pub target: String,
    #[serde(flatten)]
    pub selection: Selection,
    pub dependencies: Vec<Dependency>,
    pub inputs: Vec<InputFile>,
    pub manifest_sha256: Sha256,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

impl GenerationContract {
    /// Validate the supported schema and selected target coverage. Generator and
    /// runtime phases separately validate package/dependency/manifest semantics.
    pub fn validate(&self) -> Result<(), ContractError> {
        if self.schema_version != SCHEMA_VERSION || self.target != TARGET {
            return Err(ContractError::Policy(
                "unsupported generation contract schema_version or target".into(),
            ));
        }
        self.selection.selected_targets()?;
        let mut paths = std::collections::BTreeSet::new();
        for input in &self.inputs {
            PackagePath::declared(input.path.as_str())?;
            if !paths.insert(input.path.as_str()) {
                return Err(ContractError::Policy(format!(
                    "conflicting declared package path: {}",
                    input.path
                )));
            }
        }
        for path in &paths {
            let mut candidate = *path;
            while let Some((parent, _)) = candidate.rsplit_once('/') {
                if paths.contains(parent) {
                    return Err(ContractError::Policy(format!(
                        "conflicting file/directory package path: {parent}"
                    )));
                }
                candidate = parent;
            }
        }
        Ok(())
    }
}
