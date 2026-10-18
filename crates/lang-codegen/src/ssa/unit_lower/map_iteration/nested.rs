//! Structural Map nesting: every provider retains its own source-qualified plan.
mod guard;
use super::*;
pub(super) use guard::guard as qualify_guard;

pub(super) fn relationship(
    parsed: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    statement: UnitStatementId,
) -> Result<Option<QualifiedNested>, LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(statement.statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let span = node.span();
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let body = loop_body_span(parsed, node.payload()).ok_or_else(bad)?;
    let mut parent = None;
    let mut child = false;
    for (id, candidate) in parsed.ast().statements().iter() {
        if id == statement.statement()
            || !matches!(
                candidate.payload(),
                Statement::For { .. } | Statement::While { .. } | Statement::Loop { .. }
            )
        {
            continue;
        }
        let candidate_body = loop_body_span(parsed, candidate.payload()).ok_or_else(bad)?;
        // A provider exists only while executing its body, after source acquisition.
        let ancestor = contains(candidate_body, span);
        let descendant = contains(body, candidate.span());
        if (!ancestor && !descendant) || !same_callable(parsed, span, candidate.span()) {
            continue;
        }
        // A lambda is a distinct callable, never an enclosing/child provider frame.
        if parsed.ast().expressions().iter().any(|(_, expression)| {
            matches!(expression.payload(), Expression::Lambda { .. })
                && ((ancestor
                    && contains(candidate.span(), expression.span())
                    && contains(expression.span(), span))
                    || (descendant
                        && contains(span, expression.span())
                        && contains(expression.span(), candidate.span())))
        }) {
            continue;
        }
        let id = UnitStatementId::new(statement.source_unit(), id);
        if !matches!(candidate.payload(), Statement::For { .. })
            || typed.map_iteration(id).is_none()
        {
            return Err(unsupported());
        }
        let descriptor = typed.map_iteration(id).ok_or_else(bad)?;
        if owned
            .map_iteration(id)
            .is_none_or(|plan| plan.descriptor() != descriptor)
        {
            return Err(unsupported());
        }
        if ancestor
            && parent.is_none_or(|(_, at): (UnitStatementId, Span)| contains(at, candidate_body))
        {
            parent = Some((id, candidate_body));
        }
        child |= descendant;
    }
    Ok((parent.is_some() || child).then_some(QualifiedNested {
        parent: parent.map(|(id, _)| id),
    }))
}

pub(super) fn ancestor(
    parsed: &ParsedFile,
    outer: UnitStatementId,
    inner: UnitStatementId,
) -> bool {
    outer != inner
        && outer.source_unit() == inner.source_unit()
        && parsed
            .ast()
            .statements()
            .get(outer.statement())
            .ok()
            .zip(parsed.ast().statements().get(inner.statement()).ok())
            .is_some_and(|(outer, inner)| {
                matches!(outer.payload(), Statement::For { .. })
                    && loop_body_span(parsed, outer.payload()).is_some_and(|body| {
                        contains(body, inner.span())
                            && !parsed.ast().expressions().iter().any(|(_, expression)| {
                                matches!(expression.payload(), Expression::Lambda { .. })
                                    && contains(body, expression.span())
                                    && contains(expression.span(), inner.span())
                            })
                    })
                    && same_callable(parsed, outer.span(), inner.span())
            })
}

fn same_callable(parsed: &ParsedFile, left: Span, right: Span) -> bool {
    let enclosing = |span| {
        parsed
            .ast()
            .items()
            .iter()
            .filter(|(_, node)| {
                matches!(node.payload(), Item::Function { .. } | Item::Deinit { .. })
                    && contains(node.span(), span)
            })
            .min_by_key(|(_, node)| node.span().end() - node.span().start())
            .map(|(id, _)| id)
    };
    let left = enclosing(left);
    left.is_some() && left == enclosing(right)
}

fn loop_body_span(parsed: &ParsedFile, statement: &Statement) -> Option<Span> {
    let (Statement::For { body, .. }
    | Statement::While { body, .. }
    | Statement::Loop { body, .. }) = statement
    else {
        return None;
    };
    parsed
        .ast()
        .statements()
        .get(*body)
        .ok()
        .map(|body| body.span())
}
