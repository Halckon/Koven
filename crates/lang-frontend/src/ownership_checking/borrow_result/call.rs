//! Group-only call lookup; each checker retains its typed borrow-source predicate.
use crate::{
    ast::ExpressionId,
    parser::{Expression, ParsedFile},
};

pub(crate) fn borrow_result_call(
    parsed: &ParsedFile,
    mut expression: ExpressionId,
    mut has_borrow_source: impl FnMut(ExpressionId) -> bool,
) -> Option<ExpressionId> {
    loop {
        match parsed.ast().expressions().get(expression).ok()?.payload() {
            Expression::Group { expression: inner } => expression = *inner,
            Expression::Call { .. } if has_borrow_source(expression) => return Some(expression),
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests;
