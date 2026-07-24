//! Core domain model and calculation graph engine for Kafka configuration sizing.

#![forbid(unsafe_code)]

mod expression;
mod node;
mod value;

pub use expression::{
    Add, AnyExpression, Ceiling, CeilingDivide, Expression, ExpressionError, Maximum, Minimum,
    Multiply, Operand, Reference,
};
pub use node::{
    Citation, CitationClaim, CitationId, ConstantDefinition, ConstantOrigin, DerivedDefinition,
    IdentifierError, InputConstraint, InputDefinition, InputDefinitionError, NodeDefinition,
    NodeId, NodeKind, NodeMetadata,
};
pub use value::{DataSize, DataUnit, ExactDecimal, Value, ValueError, ValueType};
