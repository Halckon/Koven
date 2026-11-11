//! Shared mapping from the frontend's closed integer and integer-range operators into typed SSA.
#[cfg(test)]
use super::model::{Operation, ValueId};
use super::model::{CheckedArithmeticOperator, ComparisonOperator, IntegerBitwiseOperator};
use lang_frontend::{
    ast::ExpressionId,
    parser::{AssignmentOperator, BinaryOperator, Expression, ParsedFile},
    type_checking::{BuiltinType, IntegerRangeOperationKind},
};

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
    parsed: &ParsedFile,
    expression: ExpressionId,
    receiver: ExpressionId,
) -> bool {
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

pub(super) fn matches_range_fact(
    parsed: &ParsedFile,
    expression: ExpressionId,
    left: ExpressionId,
    right: ExpressionId,
    kind: IntegerRangeOperationKind,
    types: [Option<BuiltinType>; 3],
) -> bool {
    use IntegerRangeOperationKind as K;
    let Ok(node) = parsed.ast().expressions().get(expression) else {
        return false;
    };
    let Expression::Binary {
        left: actual_left,
        right: actual_right,
        operator,
        ..
    } = node.payload()
    else {
        return false;
    };
    let expected = match kind {
        K::Inclusive => BinaryOperator::InclusiveRange,
        K::Exclusive => BinaryOperator::ExclusiveRange,
        K::Contains => BinaryOperator::In,
        K::NotContains => BinaryOperator::NotIn,
        K::Equal => BinaryOperator::Equal,
        K::NotEqual => BinaryOperator::NotEqual,
    };
    if *actual_left != left || *actual_right != right || *operator != expected {
        return false;
    }
    let endpoint = |range| match range {
        Some(BuiltinType::IntRange) => Some(BuiltinType::Int),
        Some(BuiltinType::LongRange) => Some(BuiltinType::Long),
        _ => None,
    };
    match kind {
        K::Inclusive | K::Exclusive => {
            endpoint(types[2]).is_some() && types[0] == endpoint(types[2]) && types[1] == types[0]
        }
        K::Contains | K::NotContains => {
            endpoint(types[1]).is_some()
                && types[0] == endpoint(types[1])
                && types[2] == Some(BuiltinType::Boolean)
        }
        K::Equal | K::NotEqual => {
            endpoint(types[0]).is_some()
                && types[1] == types[0]
                && types[2] == Some(BuiltinType::Boolean)
        }
    }
}

#[cfg(test)]
pub(super) fn range_operation(
    kind: IntegerRangeOperationKind,
    left: ValueId,
    right: ValueId,
) -> (Operation, bool) {
    use IntegerRangeOperationKind as K;
    match kind {
        K::Inclusive | K::Exclusive => (
            Operation::IntegerRangeConstruct {
                first: left,
                bound: right,
                inclusive: kind == K::Inclusive,
            },
            false,
        ),
        K::Contains | K::NotContains => (
            Operation::IntegerRangeContains {
                value: left,
                range: right,
            },
            kind == K::NotContains,
        ),
        K::Equal | K::NotEqual => (
            Operation::IntegerRangeEqual { left, right },
            kind == K::NotEqual,
        ),
    }
}
