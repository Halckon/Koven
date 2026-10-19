//! Transparent source syntax does not change the validated type, category or owner.
mod formation;
pub(super) mod transport;
use super::{LoweringError, LoweringErrorKind};
pub(super) use formation::{BindingDeclaration, Birth, Formation, OwnerUpdate, named_formation};
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

/// Preserve the actual FE event: finishing an RHS and finishing its enclosing
/// assignment have the same end offset, but install different binding epochs.
#[derive(Clone, Copy)]
pub(super) struct DropEvent {
    pub span: lang_frontend::source::Span,
    after_expression: Option<ExpressionId>,
}
impl DropEvent {
    fn installs(self, parsed: &ParsedFile, epoch: &formation::BindingEpoch) -> bool {
        self.after_expression.is_some_and(|id| {
            parsed.ast().expressions().get(id).is_ok_and(|node| {
                node.span() == epoch.origin
                    && matches!(node.payload(), Expression::Assignment { value, .. } if *value == epoch.rhs)
            })
        })
    }
}

/// Select the actual epoch first; span equality alone cannot distinguish the
/// replaced owner's RHS cleanup from the new owner's immediate ASAP cleanup.
pub(super) fn in_formation<R>(
    parsed: &ParsedFile,
    formation: &Formation<R>,
    event: DropEvent,
) -> Result<bool, LoweringError> {
    let epoch = epoch_at(parsed, formation, event)?;
    let rhs = parsed
        .ast()
        .expressions()
        .get(epoch.rhs)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: Some(event.span),
        })?;
    Ok(rhs.span() == formation.binding
        && (event.span.end() > formation.binding.end() || event.installs(parsed, epoch)))
}

pub(super) fn drop_event(
    parsed: &ParsedFile,
    point: lang_frontend::ownership_checking::DropPoint,
) -> Option<DropEvent> {
    use lang_frontend::ownership_checking::DropPoint as P;
    let (span, after_expression) = match point {
        P::AfterExpression(e) => (parsed.ast().expressions().get(e).ok()?.span(), Some(e)),
        P::CallReturn(e) | P::ControlTransfer(e) => {
            (parsed.ast().expressions().get(e).ok()?.span(), None)
        }
        P::AfterStatement(s) | P::LoopExit(s) => {
            (parsed.ast().statements().get(s).ok()?.span(), None)
        }
        _ => return None,
    };
    Some(DropEvent {
        span,
        after_expression,
    })
}

pub(super) fn unit_drop_event(
    parsed: &ParsedFile,
    point: lang_frontend::ownership_checking::UnitDropPoint,
) -> Option<DropEvent> {
    use lang_frontend::ownership_checking::UnitDropPoint as P;
    let (span, after_expression) = match point {
        P::AfterExpression(e) => (
            parsed.ast().expressions().get(e.expression()).ok()?.span(),
            Some(e.expression()),
        ),
        P::CallReturn(e) | P::ControlTransfer(e) => (
            parsed.ast().expressions().get(e.expression()).ok()?.span(),
            None,
        ),
        P::AfterStatement(s) | P::LoopExit(s) => (
            parsed.ast().statements().get(s.statement()).ok()?.span(),
            None,
        ),
        _ => return None,
    };
    Some(DropEvent {
        span,
        after_expression,
    })
}

/// Every root Drop belongs to exactly one actual initializer/replacement event.
/// Strict completion ordering keeps cleanup of the replaced value before installing the RHS.
pub(super) fn epoch_at<'a, R>(
    parsed: &ParsedFile,
    formation: &'a Formation<R>,
    event: DropEvent,
) -> Result<&'a formation::BindingEpoch, LoweringError> {
    let bad = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(event.span),
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
        if at.source_id() != event.span.source_id() {
            return Err(bad());
        }
        // AfterExpression(assignment) proves installation. AfterExpression(rhs)
        // still owns the old value even though both events have the same end.
        if (at.end() < event.span.end()
            || (at.end() == event.span.end() && (index == 0 || event.installs(parsed, epoch))))
            && end.is_none_or(|end| at.end() > end)
        {
            selected = Some(epoch);
            end = Some(at.end());
        }
    }
    selected.ok_or_else(bad)
}
