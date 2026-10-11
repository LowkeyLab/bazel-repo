//! Typed Cargo publishing contracts and verified archive handoffs.

#![forbid(unsafe_code)]

mod analysis;
mod error;
mod hash;
mod json;
mod model;
mod path;

pub use analysis::{AnalysisInput, AnalysisSpec, CrateMetadata};
pub use error::ContractError;
pub use hash::{Sha256, digest_bytes, digest_file};
pub use json::{read_json, write_json};
pub use model::{
    BinaryTarget, Dependency, DependencyKind, GenerationContract, InputFile, LibraryTarget,
    PackageMetadata, Provenance, SCHEMA_VERSION, SelectedTarget, Selection, TARGET, TestTarget,
};
pub use path::PackagePath;

#[cfg(test)]
mod tests;
