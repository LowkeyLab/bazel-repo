use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    ContractError, Dependency, InputFile, PackageMetadata, Provenance, SCHEMA_VERSION, Selection,
    Sha256, TARGET,
};

pub const REPORT_KIND: &str = "cargo_publish_validation";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    InputFidelity,
    Toolchain,
    Configuration,
    GeneratedManifest,
    CargoPackage,
    ArchivePolicy,
    ArchiveFidelity,
    OriginalManifest,
    NormalizedManifest,
    LockedMetadata,
    DirectDependencies,
    LockedBuilds,
}
impl Check {
    pub const ALL: [Self; 12] = [
        Self::InputFidelity,
        Self::Toolchain,
        Self::Configuration,
        Self::GeneratedManifest,
        Self::CargoPackage,
        Self::ArchivePolicy,
        Self::ArchiveFidelity,
        Self::OriginalManifest,
        Self::NormalizedManifest,
        Self::LockedMetadata,
        Self::DirectDependencies,
        Self::LockedBuilds,
    ];
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Incomplete,
    Success,
    Failure,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pending,
    Passed,
    Failed,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CheckRecord {
    pub status: CheckStatus,
    pub completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub evidence: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub phase: Check,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub log: PathBuf,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub completed: bool,
    pub returncode: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileEvidence {
    pub path: PathBuf,
    pub sha256: Sha256,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManifestEvidence {
    pub generated: FileEvidence,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<FileEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized: Option<FileEvidence>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Toolchain {
    pub rustc: String,
    pub cargo: String,
    pub host: String,
    pub target: String,
    pub rustc_path: PathBuf,
    pub cargo_path: PathBuf,
    pub sysroot: PathBuf,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    Online,
    Offline,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
    pub target: String,
    pub network_mode: NetworkMode,
    pub cargo_home: PathBuf,
    pub target_directory: PathBuf,
    pub registry_cache: PathBuf,
    pub environment: BTreeMap<String, String>,
    pub config_path: PathBuf,
    pub config_sha256: Sha256,
    pub rustc_wrapper: PathBuf,
    pub rustc_wrapper_sha256: Sha256,
    pub cwd: PathBuf,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ArchiveEntry {
    File {
        path: crate::PackagePath,
        sha256: Sha256,
        size: u64,
    },
    Directory {
        path: crate::PackagePath,
    },
}
impl ArchiveEntry {
    #[must_use]
    pub const fn path(&self) -> &crate::PackagePath {
        match self {
            Self::File { path, .. } | Self::Directory { path } => path,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArchiveEvidence {
    pub path: PathBuf,
    pub sha256: Sha256,
    pub size: u64,
    pub inventory: Vec<ArchiveEntry>,
    pub extracted_root: PathBuf,
    pub fidelity: CheckStatus,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedDependencyKind {
    pub kind: Option<String>,
    pub target: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedDependency {
    pub name: String,
    pub pkg: String,
    pub dep_kinds: Vec<ResolvedDependencyKind>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedPackage {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: Option<String>,
    pub features: Vec<String>,
    pub deps: Vec<ResolvedDependency>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyEvidence {
    pub requested: Vec<Dependency>,
    pub observed: Vec<ResolvedPackage>,
    pub metadata_path: PathBuf,
    pub metadata_sha256: Sha256,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub phase: Check,
    pub message: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidationReport {
    pub schema_version: u32,
    pub report_kind: String,
    pub run_id: String,
    pub status: ReportStatus,
    pub completed: bool,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub checks: BTreeMap<Check, CheckRecord>,
    pub commands: Vec<Command>,
    pub diagnostics: Option<Diagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Selection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<Vec<InputFile>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<Configuration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<Toolchain>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifests: Option<ManifestEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<ArchiveEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lockfile: Option<FileEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<DependencyEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_contract: Option<FileEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectedPackage {
    pub name: String,
    pub version: String,
}

fn reject(message: impl Into<String>) -> ContractError {
    ContractError::Policy(message.into())
}
pub fn required<'a, T>(value: Option<&'a T>, name: &str) -> Result<&'a T, ContractError> {
    value.ok_or_else(|| reject(format!("missing validation report evidence: {name}")))
}
fn evidence(report: &ValidationReport, phase: Check, expected: Value) -> Result<(), ContractError> {
    let Value::Object(expected) = expected else {
        return Err(reject("malformed mandatory evidence object"));
    };
    for (key, value) in expected {
        if report.checks[&phase].evidence.get(&key) != Some(&value) {
            return Err(reject(format!(
                "missing or inconsistent mandatory evidence: {phase:?}/{key}"
            )));
        }
    }
    Ok(())
}

/// Certify the complete versioned evidence contract. Incomplete reports remain
/// deserializable for diagnostics, but cannot be used as verified artifacts.
///
/// # Errors
/// Returns a policy error for unsupported or incomplete evidence, a mismatched
/// package, or commands that fail to cover the declared production targets.
pub fn validate_report(
    report: &ValidationReport,
    expected: Option<&ExpectedPackage>,
) -> Result<(), ContractError> {
    if report.schema_version != SCHEMA_VERSION || report.report_kind != REPORT_KIND {
        return Err(reject(
            "unsupported cargo_publish validation report contract",
        ));
    }
    if report.status != ReportStatus::Success || !report.completed {
        return Err(reject("validation report is unsuccessful or incomplete"));
    }
    if report.run_id.is_empty()
        || report.started_at.is_empty()
        || report.completed_at.as_ref().is_none_or(String::is_empty)
    {
        return Err(reject("missing completed run identity/timestamp"));
    }
    let package = required(report.package.as_ref(), "package")?;
    if expected.is_some_and(|expected| {
        package.name != expected.name || package.version != expected.version
    }) {
        return Err(reject(
            "verified package identity/version does not match expected consumer",
        ));
    }
    let complete = CompleteEvidence::new(report, package)?;
    validate_check_completion(report)?;
    if complete.archive.fidelity != CheckStatus::Passed {
        return Err(reject("archive fidelity is not passed"));
    }
    let targets = complete.validate(report)?;
    validate_commands(
        report,
        complete.selection,
        &targets,
        &complete.config.network_mode,
    )?;
    if complete.archive.inventory.is_empty() {
        return Err(reject(
            "missing archive inventory or packaged lockfile evidence",
        ));
    }
    Ok(())
}

struct CompleteEvidence<'a> {
    package: &'a PackageMetadata,
    selection: &'a Selection,
    inputs: &'a [InputFile],
    config: &'a Configuration,
    toolchain: &'a Toolchain,
    manifests: &'a ManifestEvidence,
    original: &'a FileEvidence,
    normalized: &'a FileEvidence,
    archive: &'a ArchiveEvidence,
    lockfile: &'a FileEvidence,
    deps: &'a DependencyEvidence,
}

impl<'a> CompleteEvidence<'a> {
    fn new(
        report: &'a ValidationReport,
        package: &'a PackageMetadata,
    ) -> Result<Self, ContractError> {
        let selection = required(report.selection.as_ref(), "selection")?;
        let inputs = required(report.inputs.as_ref(), "inputs")?;
        let config = required(report.configuration.as_ref(), "configuration")?;
        let toolchain = required(report.toolchain.as_ref(), "toolchain")?;
        let manifests = required(report.manifests.as_ref(), "manifests")?;
        let original = required(manifests.original.as_ref(), "manifests.original")?;
        let normalized = required(manifests.normalized.as_ref(), "manifests.normalized")?;
        let archive = required(report.archive.as_ref(), "archive")?;
        let lockfile = required(report.lockfile.as_ref(), "lockfile")?;
        let deps = required(report.dependencies.as_ref(), "dependencies")?;
        Ok(Self {
            package,
            selection,
            inputs,
            config,
            toolchain,
            manifests,
            original,
            normalized,
            archive,
            lockfile,
            deps,
        })
    }

    fn validate(&self, report: &ValidationReport) -> Result<Vec<String>, ContractError> {
        let Self {
            package,
            selection,
            inputs,
            config,
            toolchain,
            manifests,
            original,
            normalized,
            archive,
            lockfile,
            deps,
        } = self;
        let targets: Vec<String> = selection
            .selected_targets()?
            .iter()
            .map(ToString::to_string)
            .collect();
        if toolchain.host != TARGET
            || toolchain.target != TARGET
            || !toolchain.rustc.contains("release: 1.96.0\n")
            || !toolchain.cargo.starts_with("cargo 1.96.0 ")
        {
            return Err(reject("unsupported pinned toolchain report evidence"));
        }
        if config.target != TARGET {
            return Err(reject("unsupported validation target configuration"));
        }
        evidence(
            report,
            Check::InputFidelity,
            json!({"inputs": inputs, "manifest_sha256": manifests.generated.sha256}),
        )?;
        evidence(report, Check::Configuration, json!(config))?;
        evidence(
            report,
            Check::CargoPackage,
            json!({"archive": archive.path, "verification_enabled": true}),
        )?;
        evidence(
            report,
            Check::GeneratedManifest,
            json!({"package": package, "targets": targets, "dependencies": deps.requested}),
        )?;
        evidence(
            report,
            Check::ArchivePolicy,
            json!({"inventory": archive.inventory, "root": format!("{}-{}", package.name, package.version)}),
        )?;
        evidence(
            report,
            Check::ArchiveFidelity,
            json!({"inputs": inputs, "verified": true}),
        )?;
        evidence(report, Check::OriginalManifest, json!(original))?;
        evidence(
            report,
            Check::NormalizedManifest,
            json!({"manifest": normalized, "package": package, "dependencies": deps.requested}),
        )?;
        evidence(
            report,
            Check::DirectDependencies,
            json!({"requested": deps.requested, "resolved_graph": deps.observed, "compared": true}),
        )?;
        evidence(report, Check::Toolchain, json!({"versions": toolchain}))?;
        evidence(
            report,
            Check::LockedMetadata,
            json!({"sha256": deps.metadata_sha256, "path": deps.metadata_path, "lockfile_sha256": lockfile.sha256}),
        )?;
        evidence(report, Check::LockedBuilds, json!({"targets": targets}))?;
        if deps.observed.is_empty() {
            return Err(reject("missing Cargo resolved graph evidence"));
        }
        Ok(targets)
    }
}

fn validate_check_completion(report: &ValidationReport) -> Result<(), ContractError> {
    for phase in Check::ALL {
        let check = report
            .checks
            .get(&phase)
            .ok_or_else(|| reject(format!("mandatory validation check missing: {phase:?}")))?;
        if !check.completed || check.status != CheckStatus::Passed || check.evidence.is_empty() {
            return Err(reject(format!(
                "mandatory validation check incomplete or missing evidence: {phase:?}"
            )));
        }
    }
    Ok(())
}

fn validate_commands(
    report: &ValidationReport,
    selection: &Selection,
    targets: &[String],
    network_mode: &NetworkMode,
) -> Result<(), ContractError> {
    let mut commands = BTreeMap::new();
    for command in &report.commands {
        if command.id.is_empty() || commands.insert(command.id.as_str(), command).is_some() {
            return Err(reject("duplicate or empty validation command identities"));
        }
    }
    let mut built = Vec::new();
    for phase in [
        Check::Toolchain,
        Check::CargoPackage,
        Check::LockedMetadata,
        Check::LockedBuilds,
    ] {
        let identities = report.checks[&phase]
            .evidence
            .get("command_ids")
            .and_then(Value::as_array)
            .filter(|ids| !ids.is_empty())
            .ok_or_else(|| reject(format!("missing successful command evidence: {phase:?}")))?;
        for identity in identities {
            let identity = identity
                .as_str()
                .ok_or_else(|| reject("malformed validation command identity"))?;
            let command = commands
                .get(identity)
                .ok_or_else(|| reject(format!("missing command: {phase:?}/{identity}")))?;
            if !command.completed
                || command.returncode != Some(0)
                || command.phase != phase
                || command.argv.is_empty()
                || command.log.as_os_str().is_empty()
            {
                return Err(reject(format!(
                    "unsuccessful command evidence: {phase:?}/{identity}"
                )));
            }
            let has = |argument: &str| command.argv.iter().any(|value| value == argument);
            if network_mode == &NetworkMode::Offline
                && phase != Check::Toolchain
                && !has("--offline")
            {
                return Err(reject(format!(
                    "missing offline command evidence: {phase:?}"
                )));
            }
            if matches!(phase, Check::LockedMetadata | Check::LockedBuilds) && !has("--locked") {
                return Err(reject(format!(
                    "missing locked command evidence: {phase:?}"
                )));
            }
            if phase == Check::CargoPackage
                && (has("--locked") || has("--no-verify") || !has("package"))
            {
                return Err(reject(
                    "package command must resolve and verify the extracted package",
                ));
            }
            if phase == Check::LockedBuilds {
                if !has("build") {
                    return Err(reject(
                        "locked build command does not build selected targets",
                    ));
                }
                if has("--lib")
                    && let Some(library) = &selection.library
                {
                    built.push(format!("lib:{}", library.crate_name));
                }
                for pair in command.argv.windows(2) {
                    if pair[0] == "--bin" {
                        built.push(format!("bin:{}", pair[1]));
                    }
                }
            }
        }
    }
    built.sort_unstable();
    if built != targets {
        return Err(reject(
            "successful commands do not cover every selected library/binary",
        ));
    }
    Ok(())
}
