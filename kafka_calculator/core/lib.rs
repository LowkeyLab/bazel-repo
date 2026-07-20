//! Core domain model and calculation graph engine for Kafka configuration sizing.

#![forbid(unsafe_code)]

mod node;

pub use node::{Citation, CitationClaim, CitationId, IdentifierError, NodeId, NodeMetadata};
