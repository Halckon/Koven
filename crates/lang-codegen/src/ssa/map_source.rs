//! Transparent source syntax does not change the validated type, category or owner.
mod formation;
use super::{LoweringError, LoweringErrorKind};
pub(super) use formation::{Formation, named_formation};
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

/// A replacement RHS finishes while the old binding still owns its value.
/// Its cleanup belongs to that old epoch; the new epoch begins after this point.
pub(super) fn in_formation<R>(
    parsed: &ParsedFile,
    formation: &Formation<R>,
    point: lang_frontend::source::Span,
) -> Result<bool, LoweringError> {
    let bad = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(point),
    };
    if point.source_id() != formation.binding.source_id() {
        return Err(bad());
    }
    let end = formation
        .replacement
        .map(|id| {
            parsed
                .ast()
                .expressions()
                .get(id)
                .map(|n| n.span().end())
                .map_err(|_| bad())
        })
        .transpose()?;
    Ok(point.end() > formation.binding.end() && end.is_none_or(|end| point.end() <= end))
}

pub(super) fn drop_span(
    parsed: &ParsedFile,
    point: lang_frontend::ownership_checking::DropPoint,
) -> Option<lang_frontend::source::Span> {
    use lang_frontend::ownership_checking::DropPoint as P;
    match point {
        P::AfterExpression(e) | P::CallReturn(e) | P::ControlTransfer(e) => {
            parsed.ast().expressions().get(e).ok().map(|n| n.span())
        }
        P::AfterStatement(s) | P::LoopExit(s) => {
            parsed.ast().statements().get(s).ok().map(|n| n.span())
        }
        _ => None,
    }
}

pub(super) fn unit_drop_span(
    parsed: &ParsedFile,
    point: lang_frontend::ownership_checking::UnitDropPoint,
) -> Option<lang_frontend::source::Span> {
    use lang_frontend::ownership_checking::UnitDropPoint as P;
    match point {
        P::AfterExpression(e) | P::CallReturn(e) | P::ControlTransfer(e) => parsed
            .ast()
            .expressions()
            .get(e.expression())
            .ok()
            .map(|n| n.span()),
        P::AfterStatement(s) | P::LoopExit(s) => parsed
            .ast()
            .statements()
            .get(s.statement())
            .ok()
            .map(|n| n.span()),
        _ => None,
    }
}

/// Every root Drop belongs to exactly one actual initializer/replacement event.
/// Strict completion ordering keeps cleanup of the replaced value before installing the RHS.
pub(super) fn epoch_at<'a, R>(
    parsed: &ParsedFile,
    formation: &'a Formation<R>,
    point: lang_frontend::source::Span,
) -> Result<&'a formation::BindingEpoch, LoweringError> {
    let bad = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(point),
    };
    let mut selected = None;
    let mut end = None;
    for (index, epoch) in formation.history.iter().enumerate() {
        let at = parsed
            .ast()
            .expressions()
            .get(epoch.rhs)
            .map_err(|_| bad())?
            .span();
        if at.source_id() != point.source_id() {
            return Err(bad());
        }
        // An unused initializer drops at its declaration boundary, which may
        // share the RHS end. Replacement RHS cleanup still precedes installation.
        if (at.end() < point.end() || (index == 0 && at.end() == point.end()))
            && end.is_none_or(|end| at.end() > end)
        {
            selected = Some(epoch);
            end = Some(at.end());
        }
    }
    selected.ok_or_else(bad)
}

/// Canonical identity only; the actual source/type/ownership validators precede this check.
pub(super) fn epoch_call(
    parsed: &ParsedFile,
    rhs: ExpressionId,
) -> Result<ExpressionId, LoweringError> {
    let mut id = rhs;
    loop {
        let node = parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload() {
            Expression::Group { expression } => id = *expression,
            Expression::Call { .. } => return Ok(id),
            _ => {
                return Err(LoweringError {
                    kind: LoweringErrorKind::UnsupportedNode,
                    span: Some(node.span()),
                });
            }
        }
    }
}
