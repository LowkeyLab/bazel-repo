use thiserror::Error;

use crate::{DataUnit, NodeId, ValueType};

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

/// Descriptor for a node-reference expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reference;

/// Descriptor for an addition expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Add;

/// Descriptor for a multiplication expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Multiply;

/// Descriptor for an upward-rounding expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ceiling;

/// Descriptor for an upward-rounding whole-value division expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CeilingDivide;

/// Descriptor for a minimum-selection expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Minimum;

/// Descriptor for a maximum-selection expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Maximum;

/// Descriptor for a data-size unit conversion expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConvertDataSize {
    target_unit: DataUnit,
}

/// An expression whose operation and valid API are selected by descriptor `K`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression<K> {
    operands: Vec<Operand>,
    operation: K,
}

/// A type-erased expression suitable for storage in heterogeneous graph nodes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnyExpression {
    Reference(Expression<Reference>),
    Add(Expression<Add>),
    Multiply(Expression<Multiply>),
    Ceiling(Expression<Ceiling>),
    CeilingDivide(Expression<CeilingDivide>),
    Minimum(Expression<Minimum>),
    Maximum(Expression<Maximum>),
    ConvertDataSize(Expression<ConvertDataSize>),
}

impl AnyExpression {
    /// Returns the operands in deterministic construction order.
    pub fn operands(&self) -> &[Operand] {
        match self {
            Self::Reference(expression) => &expression.operands,
            Self::Add(expression) => &expression.operands,
            Self::Multiply(expression) => &expression.operands,
            Self::Ceiling(expression) => &expression.operands,
            Self::CeilingDivide(expression) => &expression.operands,
            Self::Minimum(expression) => &expression.operands,
            Self::Maximum(expression) => &expression.operands,
            Self::ConvertDataSize(expression) => &expression.operands,
        }
    }

    /// Applies the underlying operation's static type contract to resolved operand types.
    pub fn result_type(&self, operand_types: &[ValueType]) -> Result<ValueType, ExpressionError> {
        match self {
            Self::Reference(expression) => {
                ensure_resolved_type_count("reference", 1, operand_types.len())?;
                Ok(expression.result_type(operand_types[0]))
            }
            Self::Add(expression) => expression.result_type(operand_types),
            Self::Multiply(expression) => expression.result_type(operand_types),
            Self::Ceiling(expression) => {
                ensure_resolved_type_count("ceiling", 1, operand_types.len())?;
                expression.result_type(operand_types[0])
            }
            Self::CeilingDivide(expression) => {
                ensure_resolved_type_count("ceiling divide", 2, operand_types.len())?;
                expression.result_type(operand_types[0], operand_types[1])
            }
            Self::Minimum(expression) => expression.result_type(operand_types),
            Self::Maximum(expression) => expression.result_type(operand_types),
            Self::ConvertDataSize(expression) => {
                ensure_resolved_type_count("convert data size", 1, operand_types.len())?;
                expression.result_type(operand_types[0])
            }
        }
    }
}

macro_rules! impl_any_expression_from {
    ($marker:ty, $variant:ident) => {
        impl From<Expression<$marker>> for AnyExpression {
            fn from(expression: Expression<$marker>) -> Self {
                Self::$variant(expression)
            }
        }
    };
}

impl_any_expression_from!(Reference, Reference);
impl_any_expression_from!(Add, Add);
impl_any_expression_from!(Multiply, Multiply);
impl_any_expression_from!(Ceiling, Ceiling);
impl_any_expression_from!(CeilingDivide, CeilingDivide);
impl_any_expression_from!(Minimum, Minimum);
impl_any_expression_from!(Maximum, Maximum);
impl_any_expression_from!(ConvertDataSize, ConvertDataSize);

impl Expression<Reference> {
    pub fn new(source: Operand) -> Self {
        Self::from_operation(vec![source], Reference)
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
        Self::from_operation(vec![left, right], Add)
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
        Self::from_operation(vec![left, right], Multiply)
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
        Self::from_operation(vec![value], Ceiling)
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
        Self::from_operation(vec![dividend, divisor], CeilingDivide)
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
        Self::from_operation(vec![left, right], Minimum)
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
        Self::from_operation(vec![left, right], Maximum)
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

impl Expression<ConvertDataSize> {
    pub fn new(source: Operand, target_unit: DataUnit) -> Self {
        Self::from_operation(vec![source], ConvertDataSize { target_unit })
    }

    pub fn source(&self) -> &Operand {
        &self.operands[0]
    }

    pub fn target_unit(&self) -> DataUnit {
        self.operation.target_unit
    }

    /// Requires a data-size source and preserves its broad category.
    pub fn result_type(&self, source_type: ValueType) -> Result<ValueType, ExpressionError> {
        match source_type {
            ValueType::DataSize => Ok(ValueType::DataSize),
            _ => Err(ExpressionError::UnsupportedOperandTypes {
                operation: "convert data size",
                operand_types: vec![source_type],
            }),
        }
    }
}

impl<K> Expression<K> {
    fn from_operation(operands: Vec<Operand>, operation: K) -> Self {
        Self {
            operands,
            operation,
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
        let conversion = Expression::<ConvertDataSize>::new(source.clone(), DataUnit::Mebibytes);

        assert_that!(source.node_id().as_str(), eq("input.source"));
        assert_that!(source.role(), eq("source value"));
        assert_that!(reference.source(), eq(&source));
        assert_that!(ceiling.value(), eq(&source));
        assert_that!(divide.dividend(), eq(&source));
        assert_that!(divide.divisor(), eq(&divisor));
        assert_that!(conversion.source(), eq(&source));
        assert_that!(conversion.target_unit(), eq(DataUnit::Mebibytes));
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

    #[googletest::test]
    fn convert_data_size_requires_a_data_size_source() {
        let conversion = Expression::<ConvertDataSize>::new(
            operand("input.message_size", "message size"),
            DataUnit::Bytes,
        );

        assert_that!(
            conversion.result_type(ValueType::DataSize),
            ok(eq(&ValueType::DataSize))
        );
        assert_that!(
            conversion.result_type(ValueType::Scalar),
            err(eq(&ExpressionError::UnsupportedOperandTypes {
                operation: "convert data size",
                operand_types: vec![ValueType::Scalar],
            }))
        );
    }

    #[googletest::test]
    fn typed_expressions_convert_to_the_matching_erased_variants() {
        let first = operand("input.first", "first");
        let second = operand("input.second", "second");
        let third = operand("input.third", "third");
        let expressions: Vec<AnyExpression> = vec![
            Expression::<Reference>::new(first.clone()).into(),
            Expression::<Add>::new(first.clone(), second.clone())
                .and(third.clone())
                .into(),
            Expression::<Multiply>::new(first.clone(), second.clone()).into(),
            Expression::<Ceiling>::new(first.clone()).into(),
            Expression::<CeilingDivide>::new(first.clone(), second.clone()).into(),
            Expression::<Minimum>::new(first.clone(), second.clone()).into(),
            Expression::<Maximum>::new(first.clone(), second.clone()).into(),
            Expression::<ConvertDataSize>::new(first.clone(), DataUnit::Bytes).into(),
        ];
        let expected_operands = [
            vec![first.clone()],
            vec![first.clone(), second.clone(), third.clone()],
            vec![first.clone(), second.clone()],
            vec![first.clone()],
            vec![first.clone(), second.clone()],
            vec![first.clone(), second.clone()],
            vec![first.clone(), second],
            vec![first],
        ];

        assert_that!(
            matches!(expressions[0], AnyExpression::Reference(_)),
            eq(true)
        );
        assert_that!(matches!(expressions[1], AnyExpression::Add(_)), eq(true));
        assert_that!(
            matches!(expressions[2], AnyExpression::Multiply(_)),
            eq(true)
        );
        assert_that!(
            matches!(expressions[3], AnyExpression::Ceiling(_)),
            eq(true)
        );
        assert_that!(
            matches!(expressions[4], AnyExpression::CeilingDivide(_)),
            eq(true)
        );
        assert_that!(
            matches!(expressions[5], AnyExpression::Minimum(_)),
            eq(true)
        );
        assert_that!(
            matches!(expressions[6], AnyExpression::Maximum(_)),
            eq(true)
        );
        assert_that!(
            matches!(expressions[7], AnyExpression::ConvertDataSize(_)),
            eq(true)
        );
        for (expression, expected) in expressions.iter().zip(expected_operands.iter()) {
            assert_that!(expression.operands(), eq(expected.as_slice()));
        }
    }

    #[googletest::test]
    fn erased_expressions_dispatch_every_static_type_contract() {
        let first = operand("input.first", "first");
        let second = operand("input.second", "second");
        let cases = [
            (
                AnyExpression::from(Expression::<Reference>::new(first.clone())),
                vec![ValueType::MessageCount],
                ValueType::MessageCount,
            ),
            (
                AnyExpression::from(Expression::<Add>::new(first.clone(), second.clone())),
                vec![ValueType::Ratio, ValueType::Ratio],
                ValueType::Ratio,
            ),
            (
                AnyExpression::from(Expression::<Multiply>::new(first.clone(), second.clone())),
                vec![ValueType::DataSize, ValueType::MessageCount],
                ValueType::DataSize,
            ),
            (
                AnyExpression::from(Expression::<Ceiling>::new(first.clone())),
                vec![ValueType::DataSize],
                ValueType::DataSize,
            ),
            (
                AnyExpression::from(Expression::<CeilingDivide>::new(
                    first.clone(),
                    second.clone(),
                )),
                vec![ValueType::DataSize, ValueType::DataSize],
                ValueType::Scalar,
            ),
            (
                AnyExpression::from(Expression::<Minimum>::new(first.clone(), second.clone())),
                vec![ValueType::Scalar, ValueType::Scalar],
                ValueType::Scalar,
            ),
            (
                AnyExpression::from(Expression::<Maximum>::new(first.clone(), second)),
                vec![ValueType::MessageCount, ValueType::MessageCount],
                ValueType::MessageCount,
            ),
            (
                AnyExpression::from(Expression::<ConvertDataSize>::new(
                    first,
                    DataUnit::Kibibytes,
                )),
                vec![ValueType::DataSize],
                ValueType::DataSize,
            ),
        ];

        for (expression, operand_types, expected) in cases {
            assert_that!(expression.result_type(&operand_types), ok(eq(&expected)));
        }
    }

    #[googletest::test]
    fn erased_fixed_arity_expressions_reject_resolved_type_count_mismatches() {
        let first = operand("input.first", "first");
        let second = operand("input.second", "second");
        let cases = [
            (
                AnyExpression::from(Expression::<Reference>::new(first.clone())),
                vec![],
                ExpressionError::ResolvedTypeCountMismatch {
                    operation: "reference",
                    expected: 1,
                    actual: 0,
                },
            ),
            (
                AnyExpression::from(Expression::<Ceiling>::new(first.clone())),
                vec![ValueType::Scalar, ValueType::Scalar],
                ExpressionError::ResolvedTypeCountMismatch {
                    operation: "ceiling",
                    expected: 1,
                    actual: 2,
                },
            ),
            (
                AnyExpression::from(Expression::<CeilingDivide>::new(first.clone(), second)),
                vec![ValueType::DataSize],
                ExpressionError::ResolvedTypeCountMismatch {
                    operation: "ceiling divide",
                    expected: 2,
                    actual: 1,
                },
            ),
            (
                AnyExpression::from(Expression::<ConvertDataSize>::new(
                    first,
                    DataUnit::Megabytes,
                )),
                vec![],
                ExpressionError::ResolvedTypeCountMismatch {
                    operation: "convert data size",
                    expected: 1,
                    actual: 0,
                },
            ),
        ];

        for (expression, operand_types, expected) in cases {
            assert_that!(expression.result_type(&operand_types), err(eq(&expected)));
        }
    }
}
