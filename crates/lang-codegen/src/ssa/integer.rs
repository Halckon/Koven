//! Shared mapping from the frontend's closed integer operators into typed SSA.
use super::model::{CheckedArithmeticOperator, ComparisonOperator, IntegerBitwiseOperator};
use lang_frontend::parser::{AssignmentOperator, BinaryOperator};

pub(super) const fn checked_operator(
    operator: BinaryOperator,
) -> Option<CheckedArithmeticOperator> {
    match operator {
        BinaryOperator::Add => Some(CheckedArithmeticOperator::Add),
        BinaryOperator::Subtract => Some(CheckedArithmeticOperator::Subtract),
        BinaryOperator::Multiply => Some(CheckedArithmeticOperator::Multiply),
        BinaryOperator::Divide => Some(CheckedArithmeticOperator::Divide),
        BinaryOperator::Remainder => Some(CheckedArithmeticOperator::Remainder),
        _ => None,
    }
}

pub(super) const fn assignment_operator(
    operator: AssignmentOperator,
) -> Option<CheckedArithmeticOperator> {
    match operator {
        AssignmentOperator::AddAssign => Some(CheckedArithmeticOperator::Add),
        AssignmentOperator::SubtractAssign => Some(CheckedArithmeticOperator::Subtract),
        AssignmentOperator::MultiplyAssign => Some(CheckedArithmeticOperator::Multiply),
        AssignmentOperator::DivideAssign => Some(CheckedArithmeticOperator::Divide),
        AssignmentOperator::RemainderAssign => Some(CheckedArithmeticOperator::Remainder),
        AssignmentOperator::Assign => None,
    }
}

pub(super) const fn comparison_operator(operator: BinaryOperator) -> Option<ComparisonOperator> {
    match operator {
        BinaryOperator::Equal => Some(ComparisonOperator::Equal),
        BinaryOperator::NotEqual => Some(ComparisonOperator::NotEqual),
        BinaryOperator::Less => Some(ComparisonOperator::LessThan),
        BinaryOperator::LessEqual => Some(ComparisonOperator::LessThanOrEqual),
        BinaryOperator::Greater => Some(ComparisonOperator::GreaterThan),
        BinaryOperator::GreaterEqual => Some(ComparisonOperator::GreaterThanOrEqual),
        _ => None,
    }
}

pub(super) const fn bitwise_operator(operator: BinaryOperator) -> Option<IntegerBitwiseOperator> {
    match operator {
        BinaryOperator::BitwiseAnd => Some(IntegerBitwiseOperator::And),
        BinaryOperator::BitwiseOr => Some(IntegerBitwiseOperator::Or),
        BinaryOperator::BitwiseXor => Some(IntegerBitwiseOperator::Xor),
        BinaryOperator::Shl => Some(IntegerBitwiseOperator::Shl),
        BinaryOperator::Shr => Some(IntegerBitwiseOperator::Shr),
        BinaryOperator::Ushr => Some(IntegerBitwiseOperator::Ushr),
        _ => None,
    }
}

/// Recheck fact-to-AST identity, without interpreting a source member name.
pub(super) fn intrinsic_receiver_matches(
    parsed: &lang_frontend::parser::ParsedFile,
    expression: lang_frontend::ast::ExpressionId,
    receiver: lang_frontend::ast::ExpressionId,
) -> bool {
    use lang_frontend::parser::Expression;
    let Ok(call) = parsed.ast().expressions().get(expression) else {
        return false;
    };
    let Expression::Call {
        callee,
        type_arguments,
        arguments,
        ..
    } = call.payload()
    else {
        return false;
    };
    if !type_arguments.is_empty() || !arguments.is_empty() {
        return false;
    }
    let Ok(callee) = parsed.ast().expressions().get(*callee) else {
        return false;
    };
    matches!(callee.payload(), Expression::Member { receiver: actual, safe: false, .. } if *actual == receiver)
}
