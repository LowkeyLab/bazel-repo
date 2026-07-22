use std::marker::PhantomData;

use thiserror::Error;

use crate::{NodeId, ValueType};

/// A node used by an expression and the role it plays in the operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Operand {
    node_id: NodeId,
    role: String,
}

impl Operand {
    pub fn new(node_id: NodeId, role: String) -> Self {
        Self { node_id, role }
    }

    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    pub fn role(&self) -> &str {
        &self.role
    }
}

/// Error returned when resolved operand types violate an operation's static contract.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ExpressionError {
    /// The graph validator supplied a different number of types than the expression has operands.
    #[error(
        "{operation} received {actual} resolved operand types, but the expression has {expected} operands"
    )]
    ResolvedTypeCountMismatch {
        operation: &'static str,
        expected: usize,
        actual: usize,
    },
    /// An operand type does not satisfy the operation's static contract.
    #[error(
        "{operation} operand {operand} has type {actual:?}, but operand 0 has type {expected:?}"
    )]
    IncompatibleOperandType {
        operation: &'static str,
        operand: usize,
        expected: ValueType,
        actual: ValueType,
    },
    /// The complete operand-type combination is not dimensionally supported.
    #[error("{operation} does not support operand types {operand_types:?}")]
    UnsupportedOperandTypes {
        operation: &'static str,
        operand_types: Vec<ValueType>,
    },
}

/// Marker for a node-reference expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reference;

/// Marker for an addition expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Add;

/// Marker for a multiplication expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Multiply;

/// Marker for an upward-rounding expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ceiling;

/// Marker for an upward-rounding whole-value division expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CeilingDivide;

/// Marker for a minimum-selection expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Minimum;

/// Marker for a maximum-selection expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Maximum;

/// An expression whose operation and valid API are selected by marker `K`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression<K> {
    operands: Vec<Operand>,
    kind: PhantomData<K>,
}

impl Expression<Reference> {
    pub fn new(source: Operand) -> Self {
        Self::from_operands(vec![source])
    }

    pub fn source(&self) -> &Operand {
        &self.operands[0]
    }

    /// Returns the source category unchanged.
    pub fn result_type(&self, source_type: ValueType) -> ValueType {
        source_type
    }
}

impl Expression<Add> {
    pub fn new(left: Operand, right: Operand) -> Self {
        Self::from_operands(vec![left, right])
    }

    pub fn and(mut self, term: Operand) -> Self {
        self.operands.push(term);
        self
    }

    pub fn terms(&self) -> &[Operand] {
        &self.operands
    }

    /// Requires homogeneous term categories and preserves their category.
    pub fn result_type(&self, term_types: &[ValueType]) -> Result<ValueType, ExpressionError> {
        homogeneous_result_type("add", self.operands.len(), term_types)
    }
}

impl Expression<Multiply> {
    pub fn new(left: Operand, right: Operand) -> Self {
        Self::from_operands(vec![left, right])
    }

    pub fn and(mut self, factor: Operand) -> Self {
        self.operands.push(factor);
        self
    }

    pub fn factors(&self) -> &[Operand] {
        &self.operands
    }

    /// Determines the product category for the supported dimensional combinations.
    pub fn result_type(&self, factor_types: &[ValueType]) -> Result<ValueType, ExpressionError> {
        ensure_resolved_type_count("multiply", self.operands.len(), factor_types.len())?;

        if factor_types
            .iter()
            .all(|value_type| *value_type == ValueType::Scalar)
        {
            return Ok(ValueType::Scalar);
        }
        if factor_types
            .iter()
            .all(|value_type| *value_type == ValueType::Ratio)
        {
            return Ok(ValueType::Ratio);
        }

        let data_size_count = factor_types
            .iter()
            .filter(|value_type| **value_type == ValueType::DataSize)
            .count();
        if data_size_count == 1 {
            return Ok(ValueType::DataSize);
        }

        Err(ExpressionError::UnsupportedOperandTypes {
            operation: "multiply",
            operand_types: factor_types.to_vec(),
        })
    }
}

impl Expression<Ceiling> {
    pub fn new(value: Operand) -> Self {
        Self::from_operands(vec![value])
    }

    pub fn value(&self) -> &Operand {
        &self.operands[0]
    }

    /// Preserves a decimal quantity's broad category while making it whole at evaluation time.
    pub fn result_type(&self, value_type: ValueType) -> Result<ValueType, ExpressionError> {
        match value_type {
            ValueType::Scalar | ValueType::Ratio | ValueType::DataSize => Ok(value_type),
            ValueType::MessageCount => Err(ExpressionError::UnsupportedOperandTypes {
                operation: "ceiling",
                operand_types: vec![value_type],
            }),
        }
    }
}

impl Expression<CeilingDivide> {
    pub fn new(dividend: Operand, divisor: Operand) -> Self {
        Self::from_operands(vec![dividend, divisor])
    }

    pub fn dividend(&self) -> &Operand {
        &self.operands[0]
    }

    pub fn divisor(&self) -> &Operand {
        &self.operands[1]
    }

    /// Requires compatible quantity categories and produces a scalar quotient.
    pub fn result_type(
        &self,
        dividend_type: ValueType,
        divisor_type: ValueType,
    ) -> Result<ValueType, ExpressionError> {
        if dividend_type == divisor_type {
            Ok(ValueType::Scalar)
        } else {
            Err(ExpressionError::IncompatibleOperandType {
                operation: "ceiling divide",
                operand: 1,
                expected: dividend_type,
                actual: divisor_type,
            })
        }
    }
}

impl Expression<Minimum> {
    pub fn new(left: Operand, right: Operand) -> Self {
        Self::from_operands(vec![left, right])
    }

    pub fn and(mut self, candidate: Operand) -> Self {
        self.operands.push(candidate);
        self
    }

    pub fn candidates(&self) -> &[Operand] {
        &self.operands
    }

    /// Requires homogeneous candidate categories and preserves their category.
    pub fn result_type(&self, candidate_types: &[ValueType]) -> Result<ValueType, ExpressionError> {
        homogeneous_result_type("minimum", self.operands.len(), candidate_types)
    }
}

impl Expression<Maximum> {
    pub fn new(left: Operand, right: Operand) -> Self {
        Self::from_operands(vec![left, right])
    }

    pub fn and(mut self, candidate: Operand) -> Self {
        self.operands.push(candidate);
        self
    }

    pub fn candidates(&self) -> &[Operand] {
        &self.operands
    }

    /// Requires homogeneous candidate categories and preserves their category.
    pub fn result_type(&self, candidate_types: &[ValueType]) -> Result<ValueType, ExpressionError> {
        homogeneous_result_type("maximum", self.operands.len(), candidate_types)
    }
}

impl<K> Expression<K> {
    fn from_operands(operands: Vec<Operand>) -> Self {
        Self {
            operands,
            kind: PhantomData,
        }
    }
}

fn homogeneous_result_type(
    operation: &'static str,
    operand_count: usize,
    operand_types: &[ValueType],
) -> Result<ValueType, ExpressionError> {
    ensure_resolved_type_count(operation, operand_count, operand_types.len())?;

    let expected = operand_types[0];
    for (operand, actual) in operand_types.iter().copied().enumerate().skip(1) {
        if actual != expected {
            return Err(ExpressionError::IncompatibleOperandType {
                operation,
                operand,
                expected,
                actual,
            });
        }
    }

    Ok(expected)
}

fn ensure_resolved_type_count(
    operation: &'static str,
    expected: usize,
    actual: usize,
) -> Result<(), ExpressionError> {
    if actual == expected {
        Ok(())
    } else {
        Err(ExpressionError::ResolvedTypeCountMismatch {
            operation,
            expected,
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn operand(id: &str, role: &str) -> Operand {
        Operand::new(
            NodeId::new(id).expect("test node identifier should be valid"),
            role.to_owned(),
        )
    }

    #[googletest::test]
    fn operand_and_fixed_arity_expressions_expose_operation_specific_accessors() {
        let source = operand("input.source", "source value");
        let reference = Expression::<Reference>::new(source.clone());
        let ceiling = Expression::<Ceiling>::new(source.clone());
        let divisor = operand("constant.divisor", "configuration unit divisor");
        let divide = Expression::<CeilingDivide>::new(source.clone(), divisor.clone());

        assert_that!(source.node_id().as_str(), eq("input.source"));
        assert_that!(source.role(), eq("source value"));
        assert_that!(reference.source(), eq(&source));
        assert_that!(ceiling.value(), eq(&source));
        assert_that!(divide.dividend(), eq(&source));
        assert_that!(divide.divisor(), eq(&divisor));
    }

    #[googletest::test]
    fn variable_arity_expressions_preserve_operand_insertion_order() {
        let first = operand("input.first", "first");
        let second = operand("input.second", "second");
        let third = operand("input.third", "third");

        let addition = Expression::<Add>::new(first.clone(), second.clone()).and(third.clone());
        let multiplication =
            Expression::<Multiply>::new(first.clone(), second.clone()).and(third.clone());
        let minimum = Expression::<Minimum>::new(first.clone(), second.clone()).and(third.clone());
        let maximum = Expression::<Maximum>::new(first.clone(), second.clone()).and(third.clone());
        let expected = [first, second, third];

        assert_that!(addition.terms(), eq(expected.as_slice()));
        assert_that!(multiplication.factors(), eq(expected.as_slice()));
        assert_that!(minimum.candidates(), eq(expected.as_slice()));
        assert_that!(maximum.candidates(), eq(expected.as_slice()));
    }

    #[googletest::test]
    fn reference_preserves_every_source_type() {
        let reference = Expression::<Reference>::new(operand("input.source", "source"));

        for value_type in [
            ValueType::Scalar,
            ValueType::Ratio,
            ValueType::MessageCount,
            ValueType::DataSize,
        ] {
            assert_that!(reference.result_type(value_type), eq(value_type));
        }
    }

    #[googletest::test]
    fn homogeneous_operations_preserve_matching_types_and_reject_mismatches() {
        let first = operand("input.first", "first");
        let second = operand("input.second", "second");
        let third = operand("input.third", "third");
        let addition = Expression::<Add>::new(first.clone(), second.clone()).and(third.clone());
        let minimum = Expression::<Minimum>::new(first.clone(), second.clone()).and(third.clone());
        let maximum = Expression::<Maximum>::new(first, second).and(third);
        let matching = [
            ValueType::MessageCount,
            ValueType::MessageCount,
            ValueType::MessageCount,
        ];
        let mismatched = [
            ValueType::MessageCount,
            ValueType::MessageCount,
            ValueType::DataSize,
        ];

        assert_that!(
            addition.result_type(&matching),
            ok(eq(&ValueType::MessageCount))
        );
        assert_that!(
            minimum.result_type(&matching),
            ok(eq(&ValueType::MessageCount))
        );
        assert_that!(
            maximum.result_type(&matching),
            ok(eq(&ValueType::MessageCount))
        );
        assert_that!(
            addition.result_type(&mismatched),
            err(eq(&ExpressionError::IncompatibleOperandType {
                operation: "add",
                operand: 2,
                expected: ValueType::MessageCount,
                actual: ValueType::DataSize,
            }))
        );
        assert_that!(
            minimum.result_type(&mismatched),
            err(eq(&ExpressionError::IncompatibleOperandType {
                operation: "minimum",
                operand: 2,
                expected: ValueType::MessageCount,
                actual: ValueType::DataSize,
            }))
        );
        assert_that!(
            maximum.result_type(&mismatched),
            err(eq(&ExpressionError::IncompatibleOperandType {
                operation: "maximum",
                operand: 2,
                expected: ValueType::MessageCount,
                actual: ValueType::DataSize,
            }))
        );
    }

    #[googletest::test]
    fn variable_arity_contracts_reject_a_resolved_type_count_mismatch() {
        let addition = Expression::<Add>::new(
            operand("input.left", "left term"),
            operand("input.right", "right term"),
        )
        .and(operand("input.extra", "additional term"));

        assert_that!(
            addition.result_type(&[ValueType::Scalar, ValueType::Scalar]),
            err(eq(&ExpressionError::ResolvedTypeCountMismatch {
                operation: "add",
                expected: 3,
                actual: 2,
            }))
        );
    }

    #[googletest::test]
    fn multiply_accepts_supported_dimensional_combinations() {
        let multiplication = Expression::<Multiply>::new(
            operand("input.first", "first factor"),
            operand("input.second", "second factor"),
        )
        .and(operand("input.third", "third factor"));

        assert_that!(
            multiplication.result_type(&[ValueType::Scalar, ValueType::Scalar, ValueType::Scalar,]),
            ok(eq(&ValueType::Scalar))
        );
        assert_that!(
            multiplication.result_type(&[ValueType::Ratio, ValueType::Ratio, ValueType::Ratio,]),
            ok(eq(&ValueType::Ratio))
        );
        assert_that!(
            multiplication.result_type(&[
                ValueType::Ratio,
                ValueType::DataSize,
                ValueType::MessageCount,
            ]),
            ok(eq(&ValueType::DataSize))
        );
    }

    #[googletest::test]
    fn multiply_rejects_dimensionally_unsupported_combinations() {
        let multiplication = Expression::<Multiply>::new(
            operand("input.first", "first factor"),
            operand("input.second", "second factor"),
        );

        for unsupported in [
            [ValueType::DataSize, ValueType::DataSize],
            [ValueType::Scalar, ValueType::Ratio],
            [ValueType::MessageCount, ValueType::MessageCount],
        ] {
            assert_that!(
                multiplication.result_type(&unsupported),
                err(eq(&ExpressionError::UnsupportedOperandTypes {
                    operation: "multiply",
                    operand_types: unsupported.to_vec(),
                }))
            );
        }
    }

    #[googletest::test]
    fn ceiling_accepts_decimal_categories_and_rejects_message_counts() {
        let ceiling = Expression::<Ceiling>::new(operand("input.value", "value to round"));

        for value_type in [ValueType::Scalar, ValueType::Ratio, ValueType::DataSize] {
            assert_that!(ceiling.result_type(value_type), ok(eq(&value_type)));
        }
        assert_that!(
            ceiling.result_type(ValueType::MessageCount),
            err(eq(&ExpressionError::UnsupportedOperandTypes {
                operation: "ceiling",
                operand_types: vec![ValueType::MessageCount],
            }))
        );
    }

    #[googletest::test]
    fn ceiling_divide_requires_compatible_types_and_returns_a_scalar() {
        let division = Expression::<CeilingDivide>::new(
            operand("derived.queue_bytes", "queue bytes"),
            operand("constant.config_unit_bytes", "configuration unit bytes"),
        );

        assert_that!(
            division.result_type(ValueType::DataSize, ValueType::DataSize),
            ok(eq(&ValueType::Scalar))
        );
        assert_that!(
            division.result_type(ValueType::DataSize, ValueType::Scalar),
            err(eq(&ExpressionError::IncompatibleOperandType {
                operation: "ceiling divide",
                operand: 1,
                expected: ValueType::DataSize,
                actual: ValueType::Scalar,
            }))
        );
    }
}
