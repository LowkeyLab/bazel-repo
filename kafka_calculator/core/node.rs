use std::{fmt, str::FromStr};

use thiserror::Error;

use crate::{
    expression::{AnyExpression, Operand},
    value::{Value, ValueType},
};

/// Error returned when a graph identifier does not use the canonical syntax.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IdentifierError {
    /// The identifier contains no characters.
    #[error("identifier must not be empty")]
    Empty,
    /// A dot-delimited segment contains no characters.
    #[error("identifier segment {segment} must not be empty")]
    EmptySegment { segment: usize },
    /// A segment does not start with a lowercase ASCII letter.
    #[error(
        "identifier segment {segment} must start with a lowercase ASCII letter, found `{character}`"
    )]
    InvalidSegmentStart { segment: usize, character: char },
    /// A segment contains a character outside `[a-z0-9_-]`.
    #[error(
        "identifier segment {segment} contains invalid character `{value}` at position {character}"
    )]
    InvalidCharacter {
        segment: usize,
        character: usize,
        value: char,
    },
}

/// Stable identity of a node in a calculation graph.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(String);

impl NodeId {
    /// Creates an identifier from dot-delimited lowercase ASCII segments.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }

    fn from_parts(prefix: &'static str, suffix: NodeIdSuffix) -> Self {
        Self(format!("{prefix}.{}", suffix.as_str()))
    }

    /// Returns the canonical textual identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for NodeId {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Caller-provided portion of a node ID after its node-type prefix.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeIdSuffix(String);

impl NodeIdSuffix {
    /// Creates a suffix from dot-delimited lowercase ASCII segments.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }

    /// Returns the canonical textual suffix.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeIdSuffix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for NodeIdSuffix {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Stable identity of a citation in a graph's citation catalog.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CitationId(String);

impl CitationId {
    /// Creates an identifier from dot-delimited lowercase ASCII segments.
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_identifier(&value)?;
        Ok(Self(value))
    }

    /// Returns the canonical textual identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CitationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CitationId {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Source material supporting one or more graph decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Citation {
    id: CitationId,
    title: String,
    url: String,
    summary: String,
}

impl Citation {
    pub fn new(id: CitationId, title: String, url: String, summary: String) -> Self {
        Self {
            id,
            title,
            url,
            summary,
        }
    }

    pub fn id(&self) -> &CitationId {
        &self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }
}

/// A specific claim made by a node and supported by a citation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CitationClaim {
    citation_id: CitationId,
    claim: String,
}

impl CitationClaim {
    pub fn new(citation_id: CitationId, claim: String) -> Self {
        Self { citation_id, claim }
    }

    pub fn citation_id(&self) -> &CitationId {
        &self.citation_id
    }

    pub fn claim(&self) -> &str {
        &self.claim
    }
}

/// Common, client-independent metadata attached to a graph node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeMetadata {
    label: String,
    description: String,
    citation_claims: Vec<CitationClaim>,
}

impl NodeMetadata {
    pub fn new(label: String, description: String, citation_claims: Vec<CitationClaim>) -> Self {
        Self {
            label,
            description,
            citation_claims,
        }
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn citation_claims(&self) -> &[CitationClaim] {
        &self.citation_claims
    }
}

/// Why a fixed value is part of the calculation graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstantOrigin {
    /// A mathematical or measurement-unit definition.
    UnitDefinition,
    /// A default defined by the Kafka client.
    ClientDefault,
    /// A bound or requirement imposed by the Kafka client.
    ClientConstraint,
    /// A deliberate policy chosen by this calculator.
    CalculatorPolicy,
}

/// The fixed value and provenance stored by a constant node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constant {
    value: Value,
    origin: ConstantOrigin,
    rationale: String,
}

impl Constant {
    pub fn new(value: Value, origin: ConstantOrigin, rationale: String) -> Self {
        Self {
            value,
            origin,
            rationale,
        }
    }

    pub fn value(&self) -> Value {
        self.value
    }

    pub fn origin(&self) -> ConstantOrigin {
        self.origin
    }

    pub fn rationale(&self) -> &str {
        &self.rationale
    }
}

/// The unevaluated expression stored by a derived node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Derived {
    expression: AnyExpression,
}

impl Derived {
    pub fn new(expression: AnyExpression) -> Self {
        Self { expression }
    }

    pub fn expression(&self) -> &AnyExpression {
        &self.expression
    }
}

/// A hard bound that an input value must satisfy during binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputConstraint {
    MinimumInclusive(Value),
    MinimumExclusive(Value),
    MaximumInclusive(Value),
    MaximumExclusive(Value),
}

impl InputConstraint {
    /// Returns the bound value used by this constraint.
    pub fn value(self) -> Value {
        match self {
            Self::MinimumInclusive(value)
            | Self::MinimumExclusive(value)
            | Self::MaximumInclusive(value)
            | Self::MaximumExclusive(value) => value,
        }
    }
}

/// Error returned when an input contains incompatible values.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum InputDefinitionError {
    /// The default value does not have the input's declared type.
    #[error("input default has type {actual:?}, expected {expected:?}")]
    DefaultTypeMismatch {
        expected: ValueType,
        actual: ValueType,
    },
    /// A hard constraint does not have the input's declared type.
    #[error("input constraint {index} has type {actual:?}, expected {expected:?}")]
    ConstraintTypeMismatch {
        index: usize,
        expected: ValueType,
        actual: ValueType,
    },
}

/// Type, optional default, and hard constraints stored by an input node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Input {
    value_type: ValueType,
    default: Option<Value>,
    hard_constraints: Vec<InputConstraint>,
}

impl Input {
    /// Creates an input whose default and constraints match its declared type.
    pub fn new(
        value_type: ValueType,
        default: Option<Value>,
        hard_constraints: Vec<InputConstraint>,
    ) -> Result<Self, InputDefinitionError> {
        if let Some(default) = default {
            let actual = default.value_type();
            if actual != value_type {
                return Err(InputDefinitionError::DefaultTypeMismatch {
                    expected: value_type,
                    actual,
                });
            }
        }

        for (index, constraint) in hard_constraints.iter().copied().enumerate() {
            let actual = constraint.value().value_type();
            if actual != value_type {
                return Err(InputDefinitionError::ConstraintTypeMismatch {
                    index,
                    expected: value_type,
                    actual,
                });
            }
        }

        Ok(Self {
            value_type,
            default,
            hard_constraints,
        })
    }

    pub fn value_type(&self) -> ValueType {
        self.value_type
    }

    pub fn default(&self) -> Option<Value> {
        self.default
    }

    pub fn hard_constraints(&self) -> &[InputConstraint] {
        &self.hard_constraints
    }
}

/// The client configuration area to which a setting applies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingScope {
    Producer,
    Consumer,
    Common,
}

/// The unit used by a rendered client configuration value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingUnit {
    Bytes,
    KBytes,
    Messages,
}

/// Configuration semantics and expression stored by a setting node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Setting {
    key: String,
    scope: SettingScope,
    unit: SettingUnit,
    expression: AnyExpression,
}

impl Setting {
    pub fn new(
        key: String,
        scope: SettingScope,
        unit: SettingUnit,
        expression: AnyExpression,
    ) -> Self {
        Self {
            key,
            scope,
            unit,
            expression,
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn scope(&self) -> SettingScope {
        self.scope
    }

    pub fn unit(&self) -> SettingUnit {
        self.unit
    }

    pub fn expression(&self) -> &AnyExpression {
        &self.expression
    }
}

/// Importance of an active finding.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FindingSeverity {
    Informational,
    Warning,
    Error,
}

/// Operation used to compare a finding's two operands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

/// An explicit comparison between two graph values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Comparison {
    left: Operand,
    operator: ComparisonOperator,
    right: Operand,
}

impl Comparison {
    pub fn new(left: Operand, operator: ComparisonOperator, right: Operand) -> Self {
        Self {
            left,
            operator,
            right,
        }
    }

    pub fn left(&self) -> &Operand {
        &self.left
    }

    pub fn operator(&self) -> ComparisonOperator {
        self.operator
    }

    pub fn right(&self) -> &Operand {
        &self.right
    }
}

/// Condition that determines whether a finding is active.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FindingCondition {
    Always,
    Comparison(Comparison),
}

/// Severity and condition stored by a finding node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    severity: FindingSeverity,
    condition: FindingCondition,
}

impl Finding {
    pub fn new(severity: FindingSeverity, condition: FindingCondition) -> Self {
        Self {
            severity,
            condition,
        }
    }

    pub fn severity(&self) -> FindingSeverity {
        self.severity
    }

    pub fn condition(&self) -> &FindingCondition {
        &self.condition
    }
}

mod private {
    pub trait Sealed {}
}

/// Static metadata for one of the engine's closed set of node types.
pub trait NodeTypeMetadata: private::Sealed {
    const ID_PREFIX: &'static str;
}

macro_rules! impl_node_type_metadata {
    ($node_type:ty, $prefix:literal) => {
        impl private::Sealed for $node_type {}

        impl NodeTypeMetadata for $node_type {
            const ID_PREFIX: &'static str = $prefix;
        }
    };
}

impl_node_type_metadata!(Input, "input");
impl_node_type_metadata!(Constant, "constant");
impl_node_type_metadata!(Derived, "derived");
impl_node_type_metadata!(Setting, "setting");
impl_node_type_metadata!(Finding, "finding");

/// A node whose concrete type determines its stable ID prefix and stored data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node<T: NodeTypeMetadata> {
    id: NodeId,
    metadata: NodeMetadata,
    node_type: T,
}

impl<T: NodeTypeMetadata> Node<T> {
    pub fn new(suffix: NodeIdSuffix, metadata: NodeMetadata, node_type: T) -> Self {
        Self {
            id: NodeId::from_parts(T::ID_PREFIX, suffix),
            metadata,
            node_type,
        }
    }

    pub fn id(&self) -> &NodeId {
        &self.id
    }

    pub fn metadata(&self) -> &NodeMetadata {
        &self.metadata
    }

    pub fn node_type(&self) -> &T {
        &self.node_type
    }
}

/// A type-erased node suitable for heterogeneous graph storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnyNode {
    Input(Node<Input>),
    Constant(Node<Constant>),
    Derived(Node<Derived>),
    Setting(Node<Setting>),
    Finding(Node<Finding>),
}

impl AnyNode {
    pub fn id(&self) -> &NodeId {
        match self {
            Self::Input(node) => node.id(),
            Self::Constant(node) => node.id(),
            Self::Derived(node) => node.id(),
            Self::Setting(node) => node.id(),
            Self::Finding(node) => node.id(),
        }
    }

    pub fn metadata(&self) -> &NodeMetadata {
        match self {
            Self::Input(node) => node.metadata(),
            Self::Constant(node) => node.metadata(),
            Self::Derived(node) => node.metadata(),
            Self::Setting(node) => node.metadata(),
            Self::Finding(node) => node.metadata(),
        }
    }
}

macro_rules! impl_any_node_from {
    ($node_type:ty, $variant:ident) => {
        impl From<Node<$node_type>> for AnyNode {
            fn from(node: Node<$node_type>) -> Self {
                Self::$variant(node)
            }
        }
    };
}

impl_any_node_from!(Input, Input);
impl_any_node_from!(Constant, Constant);
impl_any_node_from!(Derived, Derived);
impl_any_node_from!(Setting, Setting);
impl_any_node_from!(Finding, Finding);

fn validate_identifier(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::Empty);
    }

    for (segment_index, segment) in value.split('.').enumerate() {
        let mut characters = segment.chars();
        let Some(first) = characters.next() else {
            return Err(IdentifierError::EmptySegment {
                segment: segment_index,
            });
        };
        if !first.is_ascii_lowercase() {
            return Err(IdentifierError::InvalidSegmentStart {
                segment: segment_index,
                character: first,
            });
        }

        for (character_index, character) in characters.enumerate() {
            if !(character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '_' | '-'))
            {
                return Err(IdentifierError::InvalidCharacter {
                    segment: segment_index,
                    character: character_index + 1,
                    value: character,
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use googletest::prelude::*;

    use super::*;
    use crate::{DataSize, DataUnit, ExactDecimal, Expression, Operand, Reference};

    #[googletest::test]
    fn node_id_accepts_canonical_identifiers() {
        let id = NodeId::new(String::from("setting.producer.queue_buffering-max_kbytes"))
            .expect("canonical identifier should be accepted");

        assert_that!(
            id.as_str(),
            eq("setting.producer.queue_buffering-max_kbytes")
        );
        assert_that!(
            id.to_string(),
            eq("setting.producer.queue_buffering-max_kbytes")
        );
        assert_that!(
            NodeId::from_str("input.message.value_bytes"),
            eq(&Ok(
                NodeId::new("input.message.value_bytes").expect("valid identifier")
            ))
        );
    }

    #[googletest::test]
    fn node_id_suffix_accepts_canonical_identifiers_and_rejects_invalid_ones() {
        let suffix = NodeIdSuffix::from_str("producer.queue_buffering-max_kbytes")
            .expect("canonical suffix should be accepted");

        assert_that!(suffix.as_str(), eq("producer.queue_buffering-max_kbytes"));
        assert_that!(
            suffix.to_string(),
            eq("producer.queue_buffering-max_kbytes")
        );
        assert_that!(
            NodeIdSuffix::new("producer..queue"),
            err(eq(&IdentifierError::EmptySegment { segment: 1 }))
        );
    }

    #[googletest::test]
    fn citation_id_uses_the_same_canonical_syntax() {
        let id = CitationId::from_str("librdkafka.configuration.v2_12")
            .expect("canonical citation identifier should be accepted");

        assert_that!(id.as_str(), eq("librdkafka.configuration.v2_12"));
        assert_that!(id.to_string(), eq("librdkafka.configuration.v2_12"));
        assert_that!(CitationId::new(""), err(eq(&IdentifierError::Empty)));
    }

    #[googletest::test]
    fn identifier_rejects_an_empty_value() {
        let error = NodeId::new("").expect_err("empty identifier should be rejected");

        assert_that!(error, eq(&IdentifierError::Empty));
        assert_that!(error.to_string(), eq("identifier must not be empty"));
    }

    #[googletest::test]
    fn identifier_rejects_an_empty_segment() {
        let error = NodeId::new("input..bytes")
            .expect_err("identifier with an empty segment should be rejected");

        assert_that!(error, eq(&IdentifierError::EmptySegment { segment: 1 }));
        assert_that!(
            error.to_string(),
            eq("identifier segment 1 must not be empty")
        );
    }

    #[googletest::test]
    fn identifier_rejects_an_invalid_segment_start() {
        let error = NodeId::new("input.2bytes")
            .expect_err("segment starting with a digit should be rejected");

        assert_that!(
            error,
            eq(&IdentifierError::InvalidSegmentStart {
                segment: 1,
                character: '2',
            })
        );
        assert_that!(
            error.to_string(),
            eq("identifier segment 1 must start with a lowercase ASCII letter, found `2`")
        );
    }

    #[googletest::test]
    fn identifier_rejects_an_invalid_character() {
        let error = NodeId::new("input.byTes")
            .expect_err("uppercase character within a segment should be rejected");

        assert_that!(
            error,
            eq(&IdentifierError::InvalidCharacter {
                segment: 1,
                character: 2,
                value: 'T',
            })
        );
        assert_that!(
            error.to_string(),
            eq("identifier segment 1 contains invalid character `T` at position 2")
        );
    }

    #[googletest::test]
    fn citation_exposes_its_metadata() {
        let id = CitationId::new("librdkafka.configuration")
            .expect("citation identifier should be valid");
        let citation = Citation::new(
            id.clone(),
            String::from("librdkafka configuration properties"),
            String::from("https://example.com/configuration"),
            String::from("Defines the producer and consumer queue settings."),
        );

        assert_that!(citation.id(), eq(&id));
        assert_that!(citation.title(), eq("librdkafka configuration properties"));
        assert_that!(citation.url(), eq("https://example.com/configuration"));
        assert_that!(
            citation.summary(),
            eq("Defines the producer and consumer queue settings.")
        );
    }

    #[googletest::test]
    fn citation_claim_links_a_claim_to_its_source() {
        let citation_id =
            CitationId::new("librdkafka.queue_limit").expect("citation identifier should be valid");
        let claim = CitationClaim::new(
            citation_id.clone(),
            String::from("The producer queue is shared by all partitions."),
        );

        assert_that!(claim.citation_id(), eq(&citation_id));
        assert_that!(
            claim.claim(),
            eq("The producer queue is shared by all partitions.")
        );
    }

    #[googletest::test]
    fn constant_exposes_its_fixed_value_origin_and_rationale() {
        let value = Value::DataSize(DataSize::new(
            ExactDecimal::from_str("1024").expect("decimal should be valid"),
            DataUnit::Bytes,
        ));
        let rationale =
            String::from("librdkafka interprets producer configuration KBytes as 1,024 bytes.");
        let constant = Constant::new(value, ConstantOrigin::UnitDefinition, rationale.clone());

        assert_that!(constant.value(), eq(value));
        assert_that!(constant.origin(), eq(ConstantOrigin::UnitDefinition));
        assert_that!(constant.rationale(), eq(rationale.as_str()));

        let metadata = NodeMetadata::new(
            String::from("Producer configuration KByte divisor"),
            String::from("Number of bytes represented by one producer configuration KByte."),
            vec![],
        );
        let node = Node::new(
            NodeIdSuffix::new("producer.config_kbyte_bytes")
                .expect("node identifier suffix should be valid"),
            metadata,
            constant.clone(),
        );

        assert_that!(
            node.id().as_str(),
            eq("constant.producer.config_kbyte_bytes")
        );
        assert_that!(node.node_type(), eq(&constant));
    }

    #[googletest::test]
    fn constant_origins_cover_every_fixed_value_source() {
        let origins = [
            ConstantOrigin::UnitDefinition,
            ConstantOrigin::ClientDefault,
            ConstantOrigin::ClientConstraint,
            ConstantOrigin::CalculatorPolicy,
        ];

        for origin in origins {
            let constant = Constant::new(Value::MessageCount(1), origin, String::from("Reason"));

            assert_that!(constant.origin(), eq(origin));
        }
    }

    #[googletest::test]
    fn input_exposes_its_declaration() {
        let default = Value::MessageCount(100_000);
        let constraints = vec![
            InputConstraint::MinimumExclusive(Value::MessageCount(0)),
            InputConstraint::MaximumInclusive(Value::MessageCount(1_000_000)),
        ];
        let input = Input::new(ValueType::MessageCount, Some(default), constraints.clone())
            .expect("matching default and constraints should be accepted");

        assert_that!(input.value_type(), eq(ValueType::MessageCount));
        assert_that!(input.default(), eq(Some(default)));
        assert_that!(input.hard_constraints(), eq(constraints.as_slice()));
        assert_that!(constraints[0].value(), eq(Value::MessageCount(0)));

        let metadata = NodeMetadata::new(
            String::from("Producer queue message count"),
            String::from("Target number of messages retained by the producer queue."),
            vec![],
        );
        let node = Node::new(
            NodeIdSuffix::new("producer.queue_message_count")
                .expect("node identifier suffix should be valid"),
            metadata,
            input.clone(),
        );

        assert_that!(node.id().as_str(), eq("input.producer.queue_message_count"));
        assert_that!(node.node_type(), eq(&input));
    }

    #[googletest::test]
    fn input_accepts_a_required_value_without_constraints() {
        let input = Input::new(ValueType::Ratio, None, vec![])
            .expect("required unconstrained input should be accepted");

        assert_that!(input.default(), none());
        assert_that!(input.hard_constraints().is_empty(), eq(true));
    }

    #[googletest::test]
    fn input_rejects_a_default_with_the_wrong_type() {
        let error = Input::new(
            ValueType::MessageCount,
            Some(Value::Scalar(
                ExactDecimal::from_str("10").expect("decimal should be valid"),
            )),
            vec![],
        )
        .expect_err("mismatched default should be rejected");

        assert_that!(
            error,
            eq(&InputDefinitionError::DefaultTypeMismatch {
                expected: ValueType::MessageCount,
                actual: ValueType::Scalar,
            })
        );
        assert_that!(
            error.to_string(),
            eq("input default has type Scalar, expected MessageCount")
        );
    }

    #[googletest::test]
    fn input_rejects_a_constraint_with_the_wrong_type() {
        let error = Input::new(
            ValueType::MessageCount,
            None,
            vec![
                InputConstraint::MinimumInclusive(Value::MessageCount(1)),
                InputConstraint::MaximumExclusive(Value::Ratio(
                    ExactDecimal::from_str("2").expect("decimal should be valid"),
                )),
            ],
        )
        .expect_err("mismatched constraint should be rejected");

        assert_that!(
            error,
            eq(&InputDefinitionError::ConstraintTypeMismatch {
                index: 1,
                expected: ValueType::MessageCount,
                actual: ValueType::Ratio,
            })
        );
        assert_that!(
            error.to_string(),
            eq("input constraint 1 has type Ratio, expected MessageCount")
        );
    }

    #[googletest::test]
    fn derived_stores_an_unevaluated_expression_in_a_node() {
        let source = Operand::new(
            NodeId::new("input.message.maximum_size").expect("node identifier should be valid"),
            String::from("maximum message size"),
        );
        let expression = AnyExpression::from(Expression::<Reference>::new(source));
        let derived = Derived::new(expression.clone());
        let metadata = NodeMetadata::new(
            String::from("Safe message size"),
            String::from("Message size used for downstream queue calculations."),
            vec![],
        );
        let node = Node::new(
            NodeIdSuffix::new("message.safe_size").expect("node identifier suffix should be valid"),
            metadata,
            derived.clone(),
        );

        assert_that!(derived.expression(), eq(&expression));
        assert_that!(node.id().as_str(), eq("derived.message.safe_size"));
        assert_that!(node.node_type(), eq(&derived));
    }

    #[googletest::test]
    fn node_metadata_exposes_common_graph_metadata() {
        let citation_id =
            CitationId::new("librdkafka.queue_limit").expect("citation identifier should be valid");
        let claim = CitationClaim::new(
            citation_id,
            String::from("The setting limits queued producer messages."),
        );
        let metadata = NodeMetadata::new(
            String::from("Producer queue message limit"),
            String::from("Maximum number of messages held in the producer queue."),
            vec![claim.clone()],
        );

        assert_that!(metadata.label(), eq("Producer queue message limit"));
        assert_that!(
            metadata.description(),
            eq("Maximum number of messages held in the producer queue.")
        );
        assert_that!(metadata.citation_claims(), eq([claim].as_slice()));
    }

    #[googletest::test]
    fn typed_nodes_derive_every_id_prefix_and_erase_into_matching_variants() {
        let operand = Operand::new(
            NodeId::new("input.source").expect("node identifier should be valid"),
            String::from("source"),
        );
        let expression = AnyExpression::from(Expression::<Reference>::new(operand));
        let metadata = || {
            NodeMetadata::new(
                String::from("Node label"),
                String::from("Node description"),
                vec![],
            )
        };
        let suffix =
            || NodeIdSuffix::new("example").expect("node identifier suffix should be valid");
        let nodes = [
            AnyNode::from(Node::new(
                suffix(),
                metadata(),
                Input::new(ValueType::Scalar, None, vec![]).expect("input should be valid"),
            )),
            AnyNode::from(Node::new(
                suffix(),
                metadata(),
                Constant::new(
                    Value::MessageCount(1),
                    ConstantOrigin::CalculatorPolicy,
                    String::from("Test policy"),
                ),
            )),
            AnyNode::from(Node::new(
                suffix(),
                metadata(),
                Derived::new(expression.clone()),
            )),
            AnyNode::from(Node::new(
                suffix(),
                metadata(),
                Setting::new(
                    String::from("queue.buffering.max.messages"),
                    SettingScope::Producer,
                    SettingUnit::Messages,
                    expression,
                ),
            )),
            AnyNode::from(Node::new(
                suffix(),
                metadata(),
                Finding::new(FindingSeverity::Warning, FindingCondition::Always),
            )),
        ];
        let expected_ids = [
            "input.example",
            "constant.example",
            "derived.example",
            "setting.example",
            "finding.example",
        ];

        assert_that!(matches!(nodes[0], AnyNode::Input(_)), eq(true));
        assert_that!(matches!(nodes[1], AnyNode::Constant(_)), eq(true));
        assert_that!(matches!(nodes[2], AnyNode::Derived(_)), eq(true));
        assert_that!(matches!(nodes[3], AnyNode::Setting(_)), eq(true));
        assert_that!(matches!(nodes[4], AnyNode::Finding(_)), eq(true));
        for (node, expected_id) in nodes.iter().zip(expected_ids) {
            assert_that!(node.id().as_str(), eq(expected_id));
            assert_that!(node.metadata().label(), eq("Node label"));
        }
    }
}
