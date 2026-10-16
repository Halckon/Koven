//! Cross-entry validation of concrete numeric Range facts and their AST roles.
use super::model::{Operation, ValueId};
use lang_frontend::{
    ast::ExpressionId,
    parser::{BinaryOperator, Expression, ParsedFile},
    type_checking::{BuiltinType, IntegerRangeOperationKind},
};

pub(super) fn matches_fact(
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

pub(super) fn operation(
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
