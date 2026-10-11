//! Typed Cargo publishing contracts and verified archive handoffs.

#![forbid(unsafe_code)]

mod analysis;
mod archive;
mod error;
mod hash;
mod json;
mod model;
mod path;
mod report;

pub use analysis::{AnalysisInput, AnalysisSpec, CrateMetadata};
pub use archive::{InspectedArchive, VerifiedArchive, inspect_archive, validate_verified_archive};
pub use error::ContractError;
pub use hash::{Sha256, digest_bytes, digest_file};
pub use json::{read_json, write_json};
pub use model::{
    BinaryTarget, Dependency, DependencyKind, GenerationContract, InputFile, LibraryTarget,
    PackageMetadata, Provenance, SCHEMA_VERSION, SelectedTarget, Selection, TARGET, TestTarget,
};
pub use path::PackagePath;
pub use report::{
    ArchiveEntry, ArchiveEvidence, Check, CheckRecord, CheckStatus, Command, Configuration,
    DependencyEvidence, Diagnostic, ExpectedPackage, FileEvidence, ManifestEvidence, NetworkMode,
    REPORT_KIND, ReportStatus, ResolvedDependency, ResolvedDependencyKind, ResolvedPackage,
    Toolchain, ValidationReport, validate_report,
};

#[cfg(test)]
mod certification_tests;
#[cfg(test)]
mod tests;
