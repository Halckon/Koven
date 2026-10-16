//! Deterministic scalar operation names for the SSA debug renderer.
use super::super::model::{
    BinaryOperator, CheckedArithmeticOperator, ComparisonOperator, IntegerBitwiseOperator,
};

pub(super) const fn binary_name(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "add",
        BinaryOperator::Subtract => "sub",
        BinaryOperator::Multiply => "mul",
        BinaryOperator::Equal => "eq",
        BinaryOperator::LessThan => "lt",
    }
}

pub(super) const fn integer_bitwise_name(operator: IntegerBitwiseOperator) -> &'static str {
    match operator {
        IntegerBitwiseOperator::And => "and",
        IntegerBitwiseOperator::Or => "or",
        IntegerBitwiseOperator::Xor => "xor",
        IntegerBitwiseOperator::Shl => "shl",
        IntegerBitwiseOperator::Shr => "shr",
        IntegerBitwiseOperator::Ushr => "ushr",
    }
}

pub(super) const fn checked_arithmetic_name(operator: CheckedArithmeticOperator) -> &'static str {
    match operator {
        CheckedArithmeticOperator::Add => "add",
        CheckedArithmeticOperator::Subtract => "sub",
        CheckedArithmeticOperator::Multiply => "mul",
        CheckedArithmeticOperator::Divide => "div",
        CheckedArithmeticOperator::Remainder => "rem",
    }
}

pub(super) const fn comparison_name(operator: ComparisonOperator) -> &'static str {
    match operator {
        ComparisonOperator::Equal => "eq",
        ComparisonOperator::NotEqual => "ne",
        ComparisonOperator::LessThan => "lt",
        ComparisonOperator::LessThanOrEqual => "le",
        ComparisonOperator::GreaterThan => "gt",
        ComparisonOperator::GreaterThanOrEqual => "ge",
    }
}
