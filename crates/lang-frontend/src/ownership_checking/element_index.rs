//! Shared syntax-only logical index identity; caller-specific place/source gates stay upstream.
use super::{ElementIndexIdentity, OwnershipCheckingError};
use crate::{
    ast::ExpressionId,
    parser::{Expression, LiteralKind, PrefixOperator},
};

pub(super) fn element_index_identity(
    sources: &crate::source::SourceMap,
    parsed: &crate::parser::ParsedFile,
    expression: ExpressionId,
) -> Result<ElementIndexIdentity, OwnershipCheckingError> {
    let node = parsed.ast().expressions().get(expression)?;
    let value = match node.payload() {
        Expression::Literal(LiteralKind::Integer(kind)) => {
            crate::type_checking::integer_literal_magnitude(sources.slice(node.span())?, *kind)
                .and_then(|value| i128::try_from(value).ok())
        }
        Expression::Group { expression } => {
            return element_index_identity(sources, parsed, *expression);
        }
        Expression::Prefix {
            operator: PrefixOperator::Minus,
            operand,
            ..
        } => match element_index_identity(sources, parsed, *operand)? {
            ElementIndexIdentity::Known(value) => value.checked_neg(),
            ElementIndexIdentity::Unknown => None,
        },
        Expression::Prefix {
            operator: PrefixOperator::Plus,
            operand,
            ..
        } => match element_index_identity(sources, parsed, *operand)? {
            ElementIndexIdentity::Known(value) => Some(value),
            ElementIndexIdentity::Unknown => None,
        },
        _ => None,
    };
    Ok(value.map_or(ElementIndexIdentity::Unknown, ElementIndexIdentity::Known))
}

#[cfg(test)]
mod tests;
