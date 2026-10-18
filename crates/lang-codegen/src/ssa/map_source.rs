//! Transparent source syntax does not change the validated type, category or owner.
use super::{LoweringError, LoweringErrorKind};
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, ParsedFile},
};

/// Return the canonical expression of a pure Group chain. Consumers keep the
/// original descriptor/loan source ID and independently verify its published root.
pub(super) fn transparent_expression<T: Copy + Eq>(
    parsed: &ParsedFile,
    outer: ExpressionId,
    mut typed: impl FnMut(ExpressionId) -> Option<T>,
) -> Result<ExpressionId, LoweringError> {
    let node = parsed
        .ast()
        .expressions()
        .get(outer)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let missing = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(node.span()),
    };
    let expected = typed(outer).ok_or_else(missing)?;
    let mut current = outer;
    let mut seen = Vec::new();
    loop {
        if seen.contains(&current) || typed(current) != Some(expected) {
            return Err(missing());
        }
        seen.push(current);
        let node = parsed
            .ast()
            .expressions()
            .get(current)
            .map_err(|_| missing())?;
        match node.payload() {
            Expression::Group { expression } => current = *expression,
            _ => return Ok(current),
        }
    }
}

/// A published later Drop must contain a real future reference in the same callable.
/// This checks scope/identity, never adjacency or a whitelist of continuation shapes.
pub(super) fn future_reference(
    parsed: &ParsedFile,
    current: lang_frontend::source::Span,
    point: lang_frontend::source::Span,
    references: impl Iterator<Item = lang_frontend::source::Span>,
) -> bool {
    use lang_frontend::parser::{FunctionBody, FunctionForm, Item};
    let contains = |outer: lang_frontend::source::Span, inner: lang_frontend::source::Span| {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && outer.end() >= inner.end()
    };
    let scope = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| match node.payload() {
            Item::Function {
                form:
                    FunctionForm::ImplicitUnitBlock(body)
                    | FunctionForm::Explicit {
                        body: FunctionBody::Block(body),
                        ..
                    },
                ..
            } if contains(node.span(), current) => {
                parsed.ast().statements().get(*body).ok().map(|b| b.span())
            }
            _ => None,
        })
        .min_by_key(|s| s.end() - s.start());
    scope.is_some_and(|scope| contains(scope, point))
        && point.start() >= current.end()
        && references
            .into_iter()
            .any(|reference| contains(point, reference))
        && !parsed.ast().expressions().iter().any(|(_, node)| {
            matches!(node.payload(), Expression::Lambda { .. }) && contains(node.span(), point)
        })
}
