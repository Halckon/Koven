//! Shared control-body source ranges and tail expression selection.
use crate::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, LiteralKind, ParsedFile, Statement},
    source::{SourceMap, Span},
};

use super::TypeCheckingError;

pub(super) fn function_end_span(
    sources: &SourceMap,
    parsed: &ParsedFile,
    body: StatementId,
) -> Result<Span, TypeCheckingError> {
    let span = parsed.ast().statements().get(body)?.span();
    if let Some(recovery) = parsed.block_closer_recovery(body) {
        return Ok(recovery);
    }
    let start = if sources.slice(span)?.ends_with('}') {
        span.end() - 1
    } else {
        span.end()
    };
    Ok(sources.span(span.source_id(), start, span.end())?)
}

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
