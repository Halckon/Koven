//! A named owner keeps the original producer through ordinary local Move bindings.
use super::*;
use lang_frontend::{
    parser::{FunctionBody, FunctionForm, Item},
    source::Span,
};

#[derive(Clone, Debug)]
pub(in super::super) struct Formation<R> {
    pub(in super::super) producer: ExpressionId,
    /// The current root first, followed by roots consumed by its initializer chain.
    pub(in super::super) roots: Vec<R>,
}

/// Declaration and reference callbacks consume the caller's actual name/type/ownership facts.
/// No statement adjacency or continuation syntax determines the owner's lifetime.
pub(in super::super) fn named_formation<R: Copy + Eq, T: Copy + Eq>(
    parsed: &ParsedFile,
    statement: Span,
    root: R,
    mut declaration: impl FnMut(R) -> Result<(Span, ExpressionId), LoweringError>,
    mut reference: impl FnMut(Span) -> Result<R, LoweringError>,
    mut expression_type: impl FnMut(ExpressionId) -> Option<T>,
    updates: impl Iterator<Item = ExpressionId>,
) -> Result<Formation<R>, LoweringError> {
    let missing = || LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(statement),
    };
    let unsupported = || LoweringError {
        kind: LoweringErrorKind::UnsupportedNode,
        span: Some(statement),
    };
    let contains = |outer: Span, inner: Span| {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && inner.end() <= outer.end()
    };
    let scope = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, node)| {
            let Item::Function { form, .. } = node.payload() else {
                return None;
            };
            let body = match form {
                FunctionForm::ImplicitUnitBlock(body)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(body),
                    ..
                } => *body,
                _ => return None,
            };
            let body = parsed.ast().statements().get(body).ok()?.span();
            contains(body, statement).then_some(body)
        })
        .min_by_key(|span| span.end() - span.start())
        .ok_or_else(unsupported)?;
    // These real writes invalidate declaration-only producer history. Their current
    // assignment/mutation producer needs a separate ownership proof, not MissingFact.
    let mut updates = updates.collect::<Vec<_>>();
    updates.extend(parsed.ast().expressions().iter().filter_map(
        |(_, node)| match node.payload() {
            Expression::Assignment { target, .. } => Some(*target),
            _ => None,
        },
    ));
    let mut roots = Vec::new();
    let mut current = root;
    loop {
        if roots.contains(&current) {
            return Err(missing());
        }
        roots.push(current);
        let (marker, initializer) = declaration(current)?;
        let outer = parsed
            .ast()
            .expressions()
            .get(initializer)
            .map_err(|_| missing())?;
        if !contains(scope, marker)
            || !contains(scope, outer.span())
            || marker.end() > outer.span().start()
            || outer.span().end() >= statement.start()
        {
            return Err(unsupported());
        }
        for &update in &updates {
            let outer = parsed
                .ast()
                .expressions()
                .get(update)
                .map_err(|_| missing())?;
            if !contains(scope, outer.span())
                || marker.end() >= outer.span().start()
                || outer.span().end() >= statement.start()
            {
                continue;
            }
            let update = transparent_expression(parsed, update, &mut expression_type)?;
            let node = parsed
                .ast()
                .expressions()
                .get(update)
                .map_err(|_| missing())?;
            if matches!(node.payload(), Expression::Name) && reference(node.span())? == current {
                return Err(unsupported());
            }
        }
        let initializer = transparent_expression(parsed, initializer, &mut expression_type)?;
        let node = parsed
            .ast()
            .expressions()
            .get(initializer)
            .map_err(|_| missing())?;
        match node.payload() {
            Expression::Call { .. } => {
                return Ok(Formation {
                    producer: initializer,
                    roots,
                });
            }
            Expression::Name => current = reference(node.span())?,
            _ => return Err(unsupported()),
        }
    }
}
