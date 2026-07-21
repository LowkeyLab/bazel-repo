use std::{fmt, str::FromStr};

use thiserror::Error;

use crate::{Value, ValueType};

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

/// Common, client-independent metadata attached to a graph node definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeMetadata {
    id: NodeId,
    label: String,
    description: String,
    citation_claims: Vec<CitationClaim>,
}

impl NodeMetadata {
    pub fn new(
        id: NodeId,
        label: String,
        description: String,
        citation_claims: Vec<CitationClaim>,
    ) -> Self {
        Self {
            id,
            label,
            description,
            citation_claims,
        }
    }

    pub fn id(&self) -> &NodeId {
        &self.id
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

/// The kind-specific definition of a fixed graph node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstantDefinition {
    value: Value,
    origin: ConstantOrigin,
    rationale: String,
}

impl ConstantDefinition {
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

/// Kind-specific data attached to a graph node definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeKind {
    Input(InputDefinition),
    Constant(ConstantDefinition),
}

/// A graph node's common metadata and kind-specific definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeDefinition {
    metadata: NodeMetadata,
    kind: NodeKind,
}

impl NodeDefinition {
    pub fn new(metadata: NodeMetadata, kind: NodeKind) -> Self {
        Self { metadata, kind }
    }

    pub fn metadata(&self) -> &NodeMetadata {
        &self.metadata
    }

    pub fn kind(&self) -> &NodeKind {
        &self.kind
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

/// Error returned when an input definition contains incompatible values.
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

/// Type, optional default, and hard constraints declared by an input node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputDefinition {
    value_type: ValueType,
    default: Option<Value>,
    hard_constraints: Vec<InputConstraint>,
}

impl InputDefinition {
    /// Creates an input definition whose default and constraints match its declared type.
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
    use crate::{DataSize, DataUnit, ExactDecimal};

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
    fn constant_definition_exposes_its_fixed_value_origin_and_rationale() {
        let value = Value::DataSize(DataSize::new(
            ExactDecimal::from_str("1024").expect("decimal should be valid"),
            DataUnit::Bytes,
        ));
        let rationale =
            String::from("librdkafka interprets producer configuration KBytes as 1,024 bytes.");
        let definition =
            ConstantDefinition::new(value, ConstantOrigin::UnitDefinition, rationale.clone());

        assert_that!(definition.value(), eq(value));
        assert_that!(definition.origin(), eq(ConstantOrigin::UnitDefinition));
        assert_that!(definition.rationale(), eq(rationale.as_str()));

        let metadata = NodeMetadata::new(
            NodeId::new("constant.producer.config_kbyte_bytes")
                .expect("node identifier should be valid"),
            String::from("Producer configuration KByte divisor"),
            String::from("Number of bytes represented by one producer configuration KByte."),
            vec![],
        );
        let node = NodeDefinition::new(metadata, NodeKind::Constant(definition.clone()));

        assert_that!(
            node.metadata().id().as_str(),
            eq("constant.producer.config_kbyte_bytes")
        );
        assert_that!(node.kind(), eq(&NodeKind::Constant(definition)));
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
            let definition =
                ConstantDefinition::new(Value::MessageCount(1), origin, String::from("Reason"));

            assert_that!(definition.origin(), eq(origin));
        }
    }

    #[googletest::test]
    fn input_definition_exposes_its_declaration() {
        let default = Value::MessageCount(100_000);
        let constraints = vec![
            InputConstraint::MinimumExclusive(Value::MessageCount(0)),
            InputConstraint::MaximumInclusive(Value::MessageCount(1_000_000)),
        ];
        let definition =
            InputDefinition::new(ValueType::MessageCount, Some(default), constraints.clone())
                .expect("matching default and constraints should be accepted");

        assert_that!(definition.value_type(), eq(ValueType::MessageCount));
        assert_that!(definition.default(), eq(Some(default)));
        assert_that!(definition.hard_constraints(), eq(constraints.as_slice()));
        assert_that!(constraints[0].value(), eq(Value::MessageCount(0)));

        let metadata = NodeMetadata::new(
            NodeId::new("input.producer.queue_message_count")
                .expect("node identifier should be valid"),
            String::from("Producer queue message count"),
            String::from("Target number of messages retained by the producer queue."),
            vec![],
        );
        let node = NodeDefinition::new(metadata, NodeKind::Input(definition.clone()));

        assert_that!(
            node.metadata().id().as_str(),
            eq("input.producer.queue_message_count")
        );
        assert_that!(node.kind(), eq(&NodeKind::Input(definition)));
    }

    #[googletest::test]
    fn input_definition_accepts_a_required_input_without_constraints() {
        let definition = InputDefinition::new(ValueType::Ratio, None, vec![])
            .expect("required unconstrained input should be accepted");

        assert_that!(definition.default(), none());
        assert_that!(definition.hard_constraints().is_empty(), eq(true));
    }

    #[googletest::test]
    fn input_definition_rejects_a_default_with_the_wrong_type() {
        let error = InputDefinition::new(
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
    fn input_definition_rejects_a_constraint_with_the_wrong_type() {
        let error = InputDefinition::new(
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
    fn node_metadata_exposes_common_graph_metadata() {
        let citation_id =
            CitationId::new("librdkafka.queue_limit").expect("citation identifier should be valid");
        let claim = CitationClaim::new(
            citation_id,
            String::from("The setting limits queued producer messages."),
        );
        let id = NodeId::new("setting.producer.queue_messages")
            .expect("node identifier should be valid");
        let metadata = NodeMetadata::new(
            id.clone(),
            String::from("Producer queue message limit"),
            String::from("Maximum number of messages held in the producer queue."),
            vec![claim.clone()],
        );

        assert_that!(metadata.id(), eq(&id));
        assert_that!(metadata.label(), eq("Producer queue message limit"));
        assert_that!(
            metadata.description(),
            eq("Maximum number of messages held in the producer queue.")
        );
        assert_that!(metadata.citation_claims(), eq([claim].as_slice()));
    }
}
