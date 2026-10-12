use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{NullableString, PackageMetadata, PackagePath};

/// Configured analysis metadata. Sparse public inputs omit genuinely optional
/// fields; compiler/publisher settings are opaque rule-specific values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrateMetadata {
    pub label: String,
    #[serde(default)]
    pub crate_name: String,
    #[serde(default)]
    pub crate_type: String,
    #[serde(default)]
    pub edition: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub is_test: bool,
    #[serde(
        rename = "crate",
        default,
        skip_serializing_if = "NullableString::is_missing"
    )]
    pub crate_: NullableString,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub external: bool,
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub manifest: Option<PathBuf>,
    #[serde(default)]
    pub compiler_settings: BTreeMap<String, Value>,
    #[serde(default)]
    pub publisher_settings: BTreeMap<String, Value>,
    #[serde(default)]
    pub rustc_flags: Vec<String>,
    #[serde(default)]
    pub rustc_env: BTreeMap<String, String>,
    #[serde(default)]
    pub rustc_env_files: Vec<String>,
    #[serde(default)]
    pub link_deps: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<Self>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PackagePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnalysisInput {
    pub path: PackagePath,
    pub source: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalysisSpec {
    pub schema_version: u32,
    pub package: PackageMetadata,
    pub target: String,
    #[serde(default)]
    pub library: Option<CrateMetadata>,
    #[serde(default)]
    pub binaries: Vec<CrateMetadata>,
    #[serde(default)]
    pub tests: Vec<CrateMetadata>,
    pub inputs: Vec<AnalysisInput>,
    #[serde(default)]
    pub dependency_closure: Vec<CrateMetadata>,
}
