//! Core domain model and calculation graph engine for Kafka configuration sizing.

#![forbid(unsafe_code)]

mod expression;
mod graph;
mod node;
mod value;

pub use expression::{
    Add, AnyExpression, Ceiling, CeilingDivide, ConvertDataSize, Expression, ExpressionError,
    Maximum, Minimum, Multiply, Operand, Reference,
};
pub use graph::{Edge, GraphDefinition, GraphValidationError, ValidatedGraph};
pub use node::{
    AnyNode, Citation, CitationClaim, CitationId, Comparison, ComparisonOperator, Constant,
    ConstantOrigin, Derived, Finding, FindingCondition, FindingSeverity, IdentifierError, Input,
    InputConstraint, InputDefinitionError, Node, NodeId, NodeIdSuffix, NodeMetadata,
    NodeTypeMetadata, Setting, SettingScope, SettingUnit,
};
pub use value::{DataSize, DataUnit, ExactDecimal, Value, ValueError, ValueType};
