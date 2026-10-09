pub mod catalog;
pub mod cli;
pub mod coordination;
pub mod diagnostics;
#[cfg(test)]
mod diagnostics_tests;
pub mod domain;
pub mod error;
pub mod paths;
pub mod store;
#[cfg(test)]
mod store_tests;

pub mod git;
pub mod management;
pub mod workflows;

pub mod acquisition;
pub mod acquisition_workflow;

#[cfg(test)]
mod acquisition_tests;

pub mod release;
pub mod release_workflow;

pub mod creation;
pub mod creation_workflow;

pub mod recovery_workflow;

pub mod recovery;

pub mod catalog_recovery_cli;

pub mod recovery_inspection;

pub mod repository_recovery;
