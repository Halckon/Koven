//! Shared mapping from the frontend's closed integer operators into typed SSA.
use super::model::IntegerBitwiseOperator;
use lang_frontend::parser::BinaryOperator;

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
