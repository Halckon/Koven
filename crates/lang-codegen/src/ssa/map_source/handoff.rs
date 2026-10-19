//! Original Return/local-Move syntax selects a terminal; consumers prove ownership and SSA.
use super::*;
use lang_frontend::{
    parser::{Item, NameMarker, VariableKind},
    source::Span,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Handoff<R> {
    Return {
        expression: ExpressionId,
        operand: ExpressionId,
    },
    Binding {
        root: R,
        initializer: ExpressionId,
        marker: Span,
    },
}

pub(in super::super) fn same_callable(parsed: &ParsedFile, left: Span, right: Span) -> bool {
    let enclosing = |at| {
        parsed
            .ast()
            .items()
            .iter()
            .filter(|(_, n)| {
                matches!(n.payload(), Item::Function { .. } | Item::Deinit { .. })
                    && contains(n.span(), at)
            })
            .min_by_key(|(_, n)| n.span().end() - n.span().start())
            .map(|(id, _)| id)
    };
    enclosing(left).is_some()
        && enclosing(left) == enclosing(right)
        && !parsed.ast().expressions().iter().any(|(_, n)| {
            matches!(n.payload(), Expression::Lambda { .. }) && contains(n.span(), right)
        })
}

pub(in super::super) fn find<R: Copy + Eq, T: Copy + Eq>(
    parsed: &ParsedFile,
    span: Span,
    source: R,
    storage: T,
    mut reference: impl FnMut(Span) -> Result<R, LoweringError>,
    mut declaration: impl FnMut(Span) -> Result<R, LoweringError>,
    mut typed: impl FnMut(ExpressionId) -> Option<(T, lang_frontend::type_checking::ExpressionCategory)>,
) -> Result<Option<Handoff<R>>, LoweringError> {
    use lang_frontend::type_checking::ExpressionCategory;
    let bad = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(span),
    };
    let mut candidates = Vec::new();
    let mut operand = |id| -> Result<bool, LoweringError> {
        let id = transparent_expression(parsed, id, &mut typed)?;
        let node = parsed.ast().expressions().get(id).map_err(|_| bad())?;
        Ok(node.payload() == &Expression::Name
            && reference(node.span())? == source
            && typed(id) == Some((storage, ExpressionCategory::Place)))
    };
    for (expression, node) in parsed.ast().expressions().iter() {
        if node.span().start() <= span.end() || !same_callable(parsed, span, node.span()) {
            continue;
        }
        if let Expression::Return {
            value: Some(value), ..
        } = node.payload()
            && operand(*value)?
        {
            candidates.push(Handoff::Return {
                expression,
                operand: *value,
            });
        }
    }
    for (_, node) in parsed.ast().items().iter() {
        if node.span().start() <= span.end() || !same_callable(parsed, span, node.span()) {
            continue;
        }
        if let Item::Variable {
            kind: VariableKind::Val | VariableKind::Var,
            name: NameMarker::Present(marker),
            initializer,
            ..
        } = node.payload()
            && operand(*initializer)?
        {
            candidates.push(Handoff::Binding {
                root: declaration(*marker)?,
                initializer: *initializer,
                marker: *marker,
            });
        }
    }
    match candidates.as_slice() {
        [] => Ok(None),
        [candidate] => Ok(Some(*candidate)),
        _ => Err(LoweringError {
            kind: LoweringErrorKind::UnsupportedNode,
            span: Some(span),
        }),
    }
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
