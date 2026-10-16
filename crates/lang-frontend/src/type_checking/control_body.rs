//! Pure AST selection for the narrow null context of a joined control branch.
use crate::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, LiteralKind, ParsedFile, Statement},
};

use super::TypeCheckingError;

pub(super) fn ends_in_null(
    parsed: &ParsedFile,
    statement: StatementId,
) -> Result<bool, TypeCheckingError> {
    match parsed.ast().statements().get(statement)?.payload() {
        Statement::ControlBody { elements } => match elements.last() {
            Some(last) => ends_in_null(parsed, *last),
            None => Ok(false),
        },
        Statement::Expression { expression } => null_expression(parsed, *expression),
        _ => Ok(false),
    }
}

fn null_expression(
    parsed: &ParsedFile,
    expression: ExpressionId,
) -> Result<bool, TypeCheckingError> {
    match parsed.ast().expressions().get(expression)?.payload() {
        Expression::Group { expression } => null_expression(parsed, *expression),
        Expression::Literal(LiteralKind::Null) => Ok(true),
        _ => Ok(false),
    }
}
