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

/// A real ownership birth, before aliases and mutation produce the current owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Birth<R> {
    Call(ExpressionId),
    /// The original expression's owned result, independently witnessed by its consumer.
    Expression(ExpressionId),
    EntryParameter {
        root: R,
        reference: ExpressionId,
        origin: Span,
    },
}

#[derive(Clone, Copy, Debug)]
pub(in super::super) enum BindingDeclaration {
    Local {
        marker: Span,
        initializer: ExpressionId,
    },
    EntryParameter {
        marker: Span,
    },
}

/// Both IDs come from one actual typed Map mutation descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct OwnerUpdate {
    pub(in super::super) expression: ExpressionId,
    pub(in super::super) receiver: ExpressionId,
}

#[derive(Clone, Debug)]
pub(in super::super) struct Formation<R> {
    pub(in super::super) birth: Birth<R>,
    pub(in super::super) updates: Vec<OwnerUpdate>,
    /// The current root first, followed by roots consumed by its initializer chain.
    pub(in super::super) roots: Vec<R>,
    /// Original first-root declaration marker; single-file Drop provenance uses it.
    pub(in super::super) declaration: Span,
    /// Complete linear binding events of the first root, in actual source order.
    pub(in super::super) history: Vec<BindingEpoch>,
    /// Each original RHS is resolved by the consumer at its own Move point.
    pub(in super::super) births: Vec<(ExpressionId, R, Birth<R>)>,
    /// Completion boundary of the current root's actual initializer/replacement RHS.
    pub(in super::super) binding: Span,
    /// Current root declaration marker, or the complete assignment of its new epoch.
    pub(in super::super) origin: Span,
    /// First real replacement of this current root after the provider statement.
    pub(in super::super) replacement: Option<ExpressionId>,
}

impl<R: Eq> Formation<R> {
    /// Later mutations may change the SSA owner, but not its binding epoch.
    pub(in super::super) fn same_epoch(&self, other: &Self) -> bool {
        self.birth == other.birth
            && self.roots == other.roots
            && self.declaration == other.declaration
            && self.binding == other.binding
            && self.origin == other.origin
            && self
                .history
                .iter()
                .map(|e| (e.rhs, e.origin))
                .eq(other.history.iter().map(|e| (e.rhs, e.origin)))
    }
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
#[allow(clippy::too_many_arguments)]
pub(in super::super) fn named_formation<R: Copy + Eq, T: Copy + Eq>(
    parsed: &ParsedFile,
    statement: Span,
    root: R,
    mut declaration: impl FnMut(R) -> Result<BindingDeclaration, LoweringError>,
    mut reference: impl FnMut(Span) -> Result<R, LoweringError>,
    mut expression_type: impl FnMut(ExpressionId) -> Option<T>,
    mut birth: impl FnMut(ExpressionId) -> Result<Birth<R>, LoweringError>,
    updates: impl Iterator<Item = OwnerUpdate>,
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
    // Mutation history is separate from binding history: put/remove preserve the
    // root while replacing its SSA owner. The caller proves their actual edges.
    let updates = updates.collect::<Vec<_>>();
    let mut roots = Vec::new();
    let mut current = root;
    let mut cutoff = statement;
    let mut binding = None;
    let mut origin = None;
    let mut replacement = None;
    let mut root_declaration = None;
    let mut history = Vec::new();
    let mut selected_updates = Vec::new();
    let mut last_reference = None;
    loop {
        // Repeated symbols can describe a legal but different ownership graph;
        // this bounded alias proof must not turn that graph into MissingFact.
        if roots.contains(&current) {
            return Err(unsupported());
        }
        roots.push(current);
        let declared_binding = declaration(current)?;
        if let BindingDeclaration::EntryParameter { marker } = declared_binding {
            let parameter_reference = last_reference.ok_or_else(unsupported)?;
            collect_updates(
                parsed,
                &updates,
                &mut selected_updates,
                (marker.end(), cutoff, current, None),
                &same_callable,
                &mut expression_type,
                &mut reference,
            )?;
            return Ok(Formation {
                birth: Birth::EntryParameter {
                    root: current,
                    reference: parameter_reference,
                    origin: marker,
                },
                updates: sorted_updates(parsed, selected_updates)?,
                roots,
                declaration: root_declaration.ok_or_else(missing)?,
                history,
                births: Vec::new(),
                binding: binding.ok_or_else(missing)?,
                origin: origin.ok_or_else(missing)?,
                replacement,
            });
        }
        let BindingDeclaration::Local {
            marker,
            initializer: declared,
        } = declared_binding
        else {
            return Err(missing());
        };
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
            // Each non-current Name is validated by the caller at that exact
            // Move reference. Do not recursively scan its future history here.
            for epoch in &history {
                let canonical = transparent_expression(parsed, epoch.rhs, &mut expression_type)?;
                let node = parsed
                    .ast()
                    .expressions()
                    .get(canonical)
                    .map_err(|_| missing())?;
                if node.payload() != &Expression::Name {
                    birth(canonical)?;
                }
            }
        }
        collect_updates(
            parsed,
            &updates,
            &mut selected_updates,
            (outer.span().end(), cutoff, current, None),
            &same_callable,
            &mut expression_type,
            &mut reference,
        )?;
        let initializer = transparent_expression(parsed, initializer, &mut expression_type)?;
        let node = parsed
            .ast()
            .expressions()
            .get(initializer)
            .map_err(|_| missing())?;
        match node.payload() {
            Expression::Name => {
                // Resolve the consumed owner as it existed at this exact Move
                // reference. A later rebinding of that old root is another epoch.
                cutoff = node.span();
                last_reference = Some(initializer);
                current = reference(cutoff)?;
            }
            _ => {
                return Ok(Formation {
                    birth: birth(initializer)?,
                    updates: sorted_updates(parsed, selected_updates)?,
                    roots,
                    declaration: root_declaration.ok_or_else(missing)?,
                    history,
                    births: Vec::new(),
                    binding: binding.ok_or_else(missing)?,
                    origin: origin.ok_or_else(missing)?,
                    replacement,
                });
            }
        }
    }
}

/// Extend only completed mutations of the already proved binding epoch. A lexical
/// Block span starts before its locals, but its AfterStatement executes at the end.
pub(in super::super) fn continuation<R: Copy + Eq, T: Copy + Eq>(
    parsed: &ParsedFile,
    formation: &Formation<R>,
    event: DropEvent,
    acquisition: Span,
    mut expression_type: impl FnMut(ExpressionId) -> Option<T>,
    mut reference: impl FnMut(Span) -> Result<R, LoweringError>,
    updates: impl Iterator<Item = OwnerUpdate>,
) -> Result<Formation<R>, LoweringError> {
    let bad = || error(LoweringErrorKind::MissingFact, event.span);
    if acquisition.source_id() != event.span.source_id() || !in_formation(parsed, formation, event)?
    {
        return Err(bad());
    }
    let mut terminal = formation.clone();
    if event.span.end() > acquisition.end() {
        let root = *formation.roots.first().ok_or_else(bad)?;
        let updates = updates.collect::<Vec<_>>();
        collect_updates(
            parsed,
            &updates,
            &mut terminal.updates,
            (acquisition.end(), event.span, root, Some(event.span.end())),
            &|at| handoff::same_callable(parsed, acquisition, at),
            &mut expression_type,
            &mut reference,
        )?;
        terminal.updates = sorted_updates(parsed, terminal.updates)?;
    }
    Ok(terminal)
}

fn collect_updates<R: Copy + Eq, T: Copy + Eq>(
    parsed: &ParsedFile,
    updates: &[OwnerUpdate],
    selected: &mut Vec<OwnerUpdate>,
    window: (usize, Span, R, Option<usize>),
    same_callable: &impl Fn(Span) -> bool,
    typed: &mut impl FnMut(ExpressionId) -> Option<T>,
    reference: &mut impl FnMut(Span) -> Result<R, LoweringError>,
) -> Result<(), LoweringError> {
    let (start, cutoff, root, completed_end) = window;
    for &update in updates {
        let operation = parsed
            .ast()
            .expressions()
            .get(update.expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, cutoff))?;
        let at = operation.span();
        let completed =
            completed_end.map_or_else(|| at.end() < cutoff.start(), |end| at.end() <= end);
        if !same_callable(at) || start >= at.start() || !completed {
            continue;
        }
        let receiver = transparent_expression(parsed, update.receiver, &mut *typed)?;
        let node = parsed
            .ast()
            .expressions()
            .get(receiver)
            .map_err(|_| error(LoweringErrorKind::MissingFact, cutoff))?;
        if node.payload() == &Expression::Name && reference(node.span())? == root {
            selected.push(update);
        }
    }
    Ok(())
}

fn sorted_updates(
    parsed: &ParsedFile,
    mut updates: Vec<OwnerUpdate>,
) -> Result<Vec<OwnerUpdate>, LoweringError> {
    updates.sort_by_key(|u| {
        parsed
            .ast()
            .expressions()
            .get(u.expression)
            .ok()
            .map(|n| n.span().start())
    });
    for pair in updates.windows(2) {
        let a = parsed
            .ast()
            .expressions()
            .get(pair[0].expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?
            .span();
        let b = parsed
            .ast()
            .expressions()
            .get(pair[1].expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, a))?
            .span();
        if a.end() >= b.start() {
            return Err(error(LoweringErrorKind::UnsupportedNode, b));
        }
    }
    Ok(updates)
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
