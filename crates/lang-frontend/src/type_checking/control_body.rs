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
    match tail_expression(parsed, statement)? {
        Some(expression) => null_expression(parsed, expression),
        None => Ok(false),
    }
}

pub(super) fn tail_expression(
    parsed: &ParsedFile,
    mut statement: StatementId,
) -> Result<Option<ExpressionId>, TypeCheckingError> {
    loop {
        match parsed.ast().statements().get(statement)?.payload() {
            Statement::ControlBody { elements } => match elements.last() {
                Some(&last) => statement = last,
                None => return Ok(None),
            },
            Statement::Expression { expression } => return Ok(Some(*expression)),
            _ => return Ok(None),
        }
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
