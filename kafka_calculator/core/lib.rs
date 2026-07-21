//! Core domain model and calculation graph engine for Kafka configuration sizing.

#![forbid(unsafe_code)]

mod node;
mod value;

pub use node::{
    Citation, CitationClaim, CitationId, ConstantDefinition, ConstantOrigin, IdentifierError,
    InputConstraint, InputDefinition, InputDefinitionError, NodeDefinition, NodeId, NodeKind,
    NodeMetadata,
};
pub use value::{DataSize, DataUnit, ExactDecimal, Value, ValueError, ValueType};
