//! Resolve actual producer epochs through ordinary formation, replacement and local Move names.
use super::*;
use lang_frontend::{
    parser::{AssignmentOperator, FunctionBody, FunctionForm, Item, Statement},
    source::Span,
};

#[derive(Clone, Copy, Debug)]
pub(in super::super) struct BindingEpoch {
    /// The original outer RHS ID, including any validated transparent Groups.
    pub(in super::super) rhs: ExpressionId,
    /// Declaration marker for the first event, full assignment span thereafter.
    pub(in super::super) origin: Span,
}

#[derive(Clone, Debug)]
pub(in super::super) struct Formation<R> {
    pub(in super::super) producer: ExpressionId,
    /// The current root first, followed by roots consumed by its initializer chain.
    pub(in super::super) roots: Vec<R>,
    /// Original first-root declaration marker; single-file Drop provenance uses it.
    pub(in super::super) declaration: Span,
    /// Complete linear binding events of the first root, in actual source order.
    pub(in super::super) history: Vec<BindingEpoch>,
    /// Completion boundary of the current root's actual initializer/replacement RHS.
    pub(in super::super) binding: Span,
    /// Current root declaration marker, or the complete assignment of its new epoch.
    pub(in super::super) origin: Span,
    /// First real replacement of this current root after the provider statement.
    pub(in super::super) replacement: Option<ExpressionId>,
}

#[derive(Clone, Copy)]
struct Write {
    span: Span,
    target: ExpressionId,
    value: ExpressionId,
    operator: AssignmentOperator,
}

/// Resolve formation only. The caller still proves the complete FE current-owner
/// graph and the actual SSA Call/transport identity before publishing capability.
pub(in super::super) fn named_formation<R: Copy + Eq, T: Copy + Eq>(
    parsed: &ParsedFile,
    statement: Span,
    root: R,
    mut declaration: impl FnMut(R) -> Result<(Span, ExpressionId), LoweringError>,
    mut reference: impl FnMut(Span) -> Result<R, LoweringError>,
    mut expression_type: impl FnMut(ExpressionId) -> Option<T>,
    updates: impl Iterator<Item = ExpressionId>,
) -> Result<Formation<R>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, statement);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, statement);
    let callable_bodies = parsed
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
            parsed.ast().statements().get(body).ok().map(|n| n.span())
        })
        .collect::<Vec<_>>();
    let scope = callable_bodies
        .iter()
        .copied()
        .filter(|&body| contains(body, statement))
        .min_by_key(|span| span.end() - span.start())
        .ok_or_else(unsupported)?;
    let same_callable = |span| {
        contains(scope, span)
            && callable_bodies
                .iter()
                .filter(|&&body| contains(body, span))
                .min_by_key(|body| body.end() - body.start())
                == Some(&scope)
            && !parsed.ast().expressions().iter().any(|(_, node)| {
                matches!(node.payload(), Expression::Lambda { .. }) && contains(node.span(), span)
            })
    };
    let writes = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            let Expression::Assignment {
                target,
                value,
                operator,
                ..
            } = node.payload()
            else {
                return None;
            };
            same_callable(node.span()).then_some(Write {
                span: node.span(),
                target: *target,
                value: *value,
                operator: *operator,
            })
        })
        .collect::<Vec<_>>();
    // These remain actual mutation receivers, never assignment targets. Their
    // existing Unsupported boundary does not claim mutation history is proved.
    let updates = updates.collect::<Vec<_>>();
    let mut roots = Vec::new();
    let mut current = root;
    let mut cutoff = statement;
    let mut binding = None;
    let mut origin = None;
    let mut replacement = None;
    let mut root_declaration = None;
    let mut history = Vec::new();
    loop {
        // Repeated symbols can describe a legal but different ownership graph;
        // this bounded alias proof must not turn that graph into MissingFact.
        if roots.contains(&current) {
            return Err(unsupported());
        }
        roots.push(current);
        let (marker, declared) = declaration(current)?;
        let declared_node = parsed
            .ast()
            .expressions()
            .get(declared)
            .map_err(|_| missing())?;
        if !same_callable(marker)
            || !same_callable(declared_node.span())
            || marker.end() > declared_node.span().start()
            || declared_node.span().end() >= cutoff.start()
        {
            return Err(unsupported());
        }
        let first_root = roots.len() == 1;
        let mut matching = Vec::new();
        for &write in &writes {
            if write.span.start() <= declared_node.span().end()
                || (!first_root && write.span.end() >= cutoff.start())
            {
                continue;
            }
            let target = transparent_expression(parsed, write.target, &mut expression_type)?;
            let target = parsed
                .ast()
                .expressions()
                .get(target)
                .map_err(|_| missing())?;
            if target.payload() != &Expression::Name || reference(target.span())? != current {
                continue;
            }
            if write.operator != AssignmentOperator::Assign || conditional_write(parsed, write.span)
            {
                return Err(unsupported());
            }
            matching.push(write);
        }
        matching.sort_by_key(|write| (write.span.start(), write.span.end()));
        // Nested same-root writes commit in RHS evaluation order, not target
        // source order. This linear history does not pretend to prove them.
        if matching.iter().enumerate().any(|(index, write)| {
            matching[..index].iter().any(|previous| {
                contains(previous.span, write.span) || contains(write.span, previous.span)
            })
        }) {
            return Err(unsupported());
        }
        let prior = matching
            .iter()
            .copied()
            .filter(|write| write.span.end() < cutoff.start())
            .max_by_key(|write| write.span.end());
        let next = if first_root {
            matching
                .iter()
                .copied()
                .find(|write| write.span.start() > statement.end())
        } else {
            None
        };
        let initializer = prior.map_or(declared, |write| write.value);
        let outer = parsed
            .ast()
            .expressions()
            .get(initializer)
            .map_err(|_| missing())?;
        if !same_callable(outer.span()) || outer.span().end() >= cutoff.start() {
            return Err(unsupported());
        }
        if first_root {
            root_declaration = Some(marker);
            binding = Some(outer.span());
            origin = Some(prior.map_or(marker, |write| write.span));
            replacement = next.map(|write| write.value);
            history.push(BindingEpoch {
                rhs: declared,
                origin: marker,
            });
            history.extend(matching.iter().map(|write| BindingEpoch {
                rhs: write.value,
                origin: write.span,
            }));
            // Other epochs with Name RHS require their own Move graph proof.
            // Keep those untested histories explicitly Unsupported; the current
            // Name initializer still resolves through the actual alias chain.
            for epoch in &history {
                let canonical = transparent_expression(parsed, epoch.rhs, &mut expression_type)?;
                let node = parsed
                    .ast()
                    .expressions()
                    .get(canonical)
                    .map_err(|_| missing())?;
                if !matches!(node.payload(), Expression::Call { .. })
                    && !(epoch.rhs == initializer && node.payload() == &Expression::Name)
                {
                    return Err(unsupported());
                }
            }
        }
        let binding_end = outer.span().end();
        for &update in &updates {
            let outer = parsed
                .ast()
                .expressions()
                .get(update)
                .map_err(|_| missing())?;
            if !same_callable(outer.span())
                || binding_end >= outer.span().start()
                || outer.span().end() >= cutoff.start()
            {
                continue;
            }
            let update = transparent_expression(parsed, update, &mut expression_type)?;
            let node = parsed
                .ast()
                .expressions()
                .get(update)
                .map_err(|_| missing())?;
            if node.payload() == &Expression::Name && reference(node.span())? == current {
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
                    declaration: root_declaration.ok_or_else(missing)?,
                    history,
                    binding: binding.ok_or_else(missing)?,
                    origin: origin.ok_or_else(missing)?,
                    replacement,
                });
            }
            Expression::Name => {
                // Resolve the consumed owner as it existed at this exact Move
                // reference. A later rebinding of that old root is another epoch.
                cutoff = node.span();
                current = reference(cutoff)?;
            }
            _ => return Err(unsupported()),
        }
    }
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn conditional_write(parsed: &ParsedFile, span: Span) -> bool {
    parsed.ast().statements().iter().any(|(_, node)| {
        contains(node.span(), span)
            && matches!(
                node.payload(),
                Statement::For { .. } | Statement::While { .. } | Statement::Loop { .. }
            )
    }) || parsed.ast().expressions().iter().any(|(_, node)| {
        contains(node.span(), span)
            && matches!(
                node.payload(),
                Expression::If { .. } | Expression::When { .. }
            )
    })
}

fn error(kind: LoweringErrorKind, span: Span) -> LoweringError {
    LoweringError {
        kind,
        span: Some(span),
    }
}
