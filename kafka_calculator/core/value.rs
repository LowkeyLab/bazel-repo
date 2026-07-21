use std::{fmt, str::FromStr};

use rust_decimal::Decimal;
use thiserror::Error;

/// Error returned when constructing an exact calculator value.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ValueError {
    /// The supplied text is not an ordinary exact decimal.
    #[error("`{value}` is not a valid exact decimal")]
    InvalidDecimal { value: String },
    /// Calculator values cannot be negative.
    #[error("decimal value must not be negative, found `{value}`")]
    NegativeDecimal { value: Decimal },
}

/// An exact, non-negative decimal value used by calculator quantities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactDecimal(Decimal);

impl ExactDecimal {
    /// Creates an exact decimal after enforcing the calculator's non-negative domain.
    pub fn new(value: Decimal) -> Result<Self, ValueError> {
        if value.is_sign_negative() {
            return Err(ValueError::NegativeDecimal { value });
        }

        Ok(Self(value))
    }

    /// Returns the underlying exact decimal.
    pub fn value(self) -> Decimal {
        self.0
    }
}

impl fmt::Display for ExactDecimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for ExactDecimal {
    type Err = ValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let decimal = Decimal::from_str_exact(value).map_err(|_| ValueError::InvalidDecimal {
            value: value.to_owned(),
        })?;
        Self::new(decimal)
    }
}

/// Unit attached to a user-supplied or evaluated data-size quantity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DataUnit {
    Bytes,
    Kilobytes,
    Kibibytes,
    Megabytes,
    Mebibytes,
}

/// An exact data-size quantity that retains its declared SI or IEC unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataSize {
    amount: ExactDecimal,
    unit: DataUnit,
}

impl DataSize {
    pub fn new(amount: ExactDecimal, unit: DataUnit) -> Self {
        Self { amount, unit }
    }

    pub fn amount(self) -> ExactDecimal {
        self.amount
    }

    pub fn unit(self) -> DataUnit {
        self.unit
    }
}

/// Static category expected or produced by a graph node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueType {
    Scalar,
    Ratio,
    MessageCount,
    DataSize,
}

/// A typed value accepted by an input node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Scalar(ExactDecimal),
    Ratio(ExactDecimal),
    MessageCount(u128),
    DataSize(DataSize),
}

impl Value {
    /// Returns the static category of this value.
    pub fn value_type(self) -> ValueType {
        match self {
            Self::Scalar(_) => ValueType::Scalar,
            Self::Ratio(_) => ValueType::Ratio,
            Self::MessageCount(_) => ValueType::MessageCount,
            Self::DataSize(_) => ValueType::DataSize,
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use rust_decimal::Decimal;

    use super::*;

    #[googletest::test]
    fn exact_decimal_parses_ordinary_non_negative_values() {
        let value = ExactDecimal::from_str("1024.500")
            .expect("ordinary non-negative decimal should be accepted");

        assert_that!(value.to_string(), eq("1024.500"));
        assert_that!(value.value(), eq(Decimal::new(1_024_500, 3)));
    }

    #[googletest::test]
    fn exact_decimal_rejects_unsupported_syntax() {
        assert_that!(
            ExactDecimal::from_str("1e3"),
            err(eq(&ValueError::InvalidDecimal {
                value: String::from("1e3")
            }))
        );
        assert_that!(
            ExactDecimal::from_str("NaN"),
            err(eq(&ValueError::InvalidDecimal {
                value: String::from("NaN")
            }))
        );
    }

    #[googletest::test]
    fn exact_decimal_rejects_negative_values() {
        let error = ExactDecimal::from_str("-0.5")
            .expect_err("negative calculator value should be rejected");

        assert_that!(
            error,
            eq(&ValueError::NegativeDecimal {
                value: Decimal::new(-5, 1)
            })
        );
        assert_that!(
            error.to_string(),
            eq("decimal value must not be negative, found `-0.5`")
        );
    }

    #[googletest::test]
    fn data_size_retains_its_exact_amount_and_unit() {
        let amount = ExactDecimal::from_str("1.25").expect("decimal should be valid");
        let size = DataSize::new(amount, DataUnit::Mebibytes);

        assert_that!(size.amount(), eq(amount));
        assert_that!(size.unit(), eq(DataUnit::Mebibytes));
    }

    #[googletest::test]
    fn values_report_their_static_types() {
        let decimal = ExactDecimal::from_str("1.5").expect("decimal should be valid");
        let cases = [
            (Value::Scalar(decimal), ValueType::Scalar),
            (Value::Ratio(decimal), ValueType::Ratio),
            (Value::MessageCount(10), ValueType::MessageCount),
            (
                Value::DataSize(DataSize::new(decimal, DataUnit::Kilobytes)),
                ValueType::DataSize,
            ),
        ];

        for (value, expected_type) in cases {
            assert_that!(value.value_type(), eq(expected_type));
        }
    }
}
