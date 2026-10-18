//! A finite inner tail and two real Resource owners, before any SSA is allocated.
mod guards;
mod operand;

use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    ownership_checking::{
        CleanupConditionId, CleanupOwnerValueId, CleanupSelectorId, ClosureCaptureSource, DropFact,
        DropTarget, IterationPhiBoundary,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Local {
    pub(in super::super) symbol: SymbolId,
    pub(in super::super) initializer: ExpressionId,
    pub(in super::super) initializer_span: Span,
    pub(in super::super) origin: Span,
    pub(in super::super) ty: TypeId,
    pub(in super::super) deinit: lang_frontend::ast::ItemId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Boundary {
    pub(in super::super) boundary: IterationPhiBoundary,
    pub(in super::super) owner: CleanupOwnerValueId,
    pub(in super::super) selector: CleanupSelectorId,
    pub(in super::super) condition: CleanupConditionId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct OwnerProof {
    pub(in super::super) seed: CleanupOwnerValueId,
    pub(in super::super) header: Boundary,
    pub(in super::super) exit: Boundary,
}

/// The original conditional DropFact is retained; Choice is never rewritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Shape {
    pub(in super::super) kind: IterationExitKind,
    pub(in super::super) outer_body: StatementId,
    pub(in super::super) inner_body: StatementId,
    pub(in super::super) outer_guard: Local,
    pub(in super::super) inner_guard: Local,
    pub(in super::super) operand: Option<ExpressionId>,
    pub(in super::super) inner_owner: CleanupOwnerValueId,
    pub(in super::super) inner_drop: DropFact,
    pub(in super::super) outer_owner: OwnerProof,
    pub(in super::super) outer_fallthrough_drop: DropFact,
    pub(in super::super) outer_return_drop: Option<DropFact>,
}

pub(in super::super) fn shape(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    outer: StatementId,
    inner: StatementId,
) -> Result<Shape, LoweringError> {
    let outer_node = parsed
        .ast()
        .statements()
        .get(outer)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let span = outer_node.span();
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    // Complete FE validation permits unacquired/dead-tail descriptors. Such a
    // descriptor is outside this capability; do not demand nonexistent locals.
    let outer_plan = owned.map_iteration(outer).ok_or_else(unsupported)?;
    let inner_plan = owned.map_iteration(inner).ok_or_else(unsupported)?;
    let (outer_body, outer_elements) = body(parsed, outer, span)?;
    let (inner_body, inner_elements) = body(parsed, inner, span)?;
    let Some((&outer_first, outer_rest)) = outer_elements.split_first() else {
        return Err(unsupported());
    };
    let Some((&inner_first, inner_rest)) = inner_elements.split_first() else {
        return Err(unsupported());
    };
    let Some((&last, inner_prefix)) = inner_rest.split_last() else {
        return Err(unsupported());
    };
    if outer_rest.iter().filter(|&&id| id == inner).count() != 1 {
        return Err(unsupported());
    }
    for &statement in outer_rest.iter().chain(inner_prefix) {
        if statement != inner && !super::qualify::straight(parsed, typed, statement)? {
            return Err(unsupported());
        }
    }
    let tail = parsed.ast().statements().get(last).map_err(|_| missing())?;
    let Statement::Expression { expression } = tail.payload() else {
        return Err(unsupported());
    };
    let expression_node = parsed
        .ast()
        .expressions()
        .get(*expression)
        .map_err(|_| missing())?;
    let (kind, operand) = match expression_node.payload() {
        Expression::Continue { .. } => (IterationExitKind::Continue(*expression), None),
        Expression::Break { .. } => (IterationExitKind::Break(*expression), None),
        Expression::Return {
            value: Some(value), ..
        } => (IterationExitKind::Return(*expression), Some(*value)),
        _ => return Err(unsupported()),
    };
    // Bound source capability from AST/name identities before inspecting plans.
    // A damaged plan cannot turn an admitted same-root source into a new shape.
    let outer_descriptor = typed.map_iteration(outer).ok_or_else(missing)?;
    let inner_descriptor = typed.map_iteration(inner).ok_or_else(missing)?;
    let mut roots = Vec::new();
    for descriptor in [outer_descriptor, inner_descriptor] {
        let node = parsed
            .ast()
            .statements()
            .get(descriptor.statement())
            .map_err(|_| missing())?;
        let Statement::For { source, .. } = node.payload() else {
            return Err(missing());
        };
        if *source != descriptor.source() {
            return Err(missing());
        }
        let node = parsed
            .ast()
            .expressions()
            .get(*source)
            .map_err(|_| missing())?;
        if node.payload() != &Expression::Name {
            return Err(unsupported());
        }
        let reference = unique(
            names
                .references()
                .iter()
                .filter(|r| r.span() == node.span()),
        )
        .ok_or_else(missing)?;
        let ReferenceTarget::Symbol(root) = reference.target() else {
            return Err(unsupported());
        };
        let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
        if symbol.kind() != SymbolKind::ValueParameter
            || typed.parameter_mode(*root).ok_or_else(missing)? != ParameterMode::Borrow
        {
            return Err(unsupported());
        }
        roots.push(*root);
    }
    if roots[0] != roots[1]
        || outer_descriptor.kind() != inner_descriptor.kind()
        || outer_descriptor.source_type() != inner_descriptor.source_type()
        || [
            outer_descriptor.key_type(),
            outer_descriptor.value_type(),
            inner_descriptor.key_type(),
            inner_descriptor.value_type(),
        ]
        .into_iter()
        .any(|ty| builtin_type(typed, ty) != Some(BuiltinType::String))
    {
        return Err(unsupported());
    }
    let outer_guard = guards::local(parsed, names, typed, owned, outer_first, span)?;
    let inner_guard = guards::local(parsed, names, typed, owned, inner_first, span)?;
    if outer_guard.symbol == inner_guard.symbol
        || outer_guard.initializer == inner_guard.initializer
    {
        return Err(missing());
    }
    // These owners are lexical sentinels only: no alias, call operand, move,
    // assignment, or closure capture may read either binding.
    if names.references().iter().any(|reference| {
        [outer_guard.symbol, inner_guard.symbol]
            .iter()
            .any(|&symbol| reference.target() == &ReferenceTarget::Symbol(symbol))
    }) {
        return Err(unsupported());
    }
    for (expression, node) in parsed.ast().expressions().iter() {
        if node.span().source_id() == span.source_id()
            && node.span().start() >= span.start()
            && node.span().end() <= span.end()
            && matches!(node.payload(), Expression::Call { .. })
            && expression != outer_guard.initializer
            && expression != inner_guard.initializer
        {
            let ty = typed.expression_type(expression).ok_or_else(missing)?;
            if typed.is_resource_type(ty).ok_or_else(missing)? {
                return Err(unsupported());
            }
        }
    }
    if owned.captures().iter().any(|capture| {
        [outer_guard.symbol, inner_guard.symbol]
            .iter()
            .any(|&symbol| capture.source() == ClosureCaptureSource::Symbol(symbol))
    }) || owned
        .drops()
        .iter()
        .any(|fact| fact.target() == DropTarget::Named(roots[0]))
    {
        return Err(missing());
    }
    if let Some(value) = operand {
        operand::validate(
            parsed,
            names,
            typed,
            *outer_descriptor,
            *inner_descriptor,
            value,
            span,
        )?;
    }
    let (inner_owner, inner_drop, outer_owner, outer_fallthrough_drop, outer_return_drop) =
        guards::prove(guards::ProofInput {
            parsed,
            owned,
            plans: [outer_plan, inner_plan],
            outer_body,
            guards: [outer_guard, inner_guard],
            kind,
            span,
        })?;
    let shape = Shape {
        kind,
        outer_body,
        inner_body,
        outer_guard,
        inner_guard,
        operand,
        inner_owner,
        inner_drop,
        outer_owner,
        outer_fallthrough_drop,
        outer_return_drop,
    };
    validate_cleanup(
        owned,
        Pair {
            outer,
            inner,
            tail: Some(shape),
        },
        shape,
    )?;
    Ok(shape)
}

fn body(
    parsed: &ParsedFile,
    statement: StatementId,
    span: Span,
) -> Result<(StatementId, &[StatementId]), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| missing())?;
    let Statement::For { body, .. } = node.payload() else {
        return Err(missing());
    };
    let node = parsed
        .ast()
        .statements()
        .get(*body)
        .map_err(|_| missing())?;
    let Statement::Block { elements } = node.payload() else {
        return Err(error(LoweringErrorKind::UnsupportedNode, span));
    };
    Ok((*body, elements))
}

/// Validate the complete public event once. Both Return plan references must
/// equal this same ten-action event; this function never executes an action.
pub(in super::super) fn validate_cleanup(
    owned: &OwnershipCheckedFile,
    pair: Pair,
    shape: Shape,
) -> Result<(), LoweringError> {
    let span = shape.outer_guard.origin;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let outer = owned.map_iteration(pair.outer).ok_or_else(missing)?;
    let inner = owned.map_iteration(pair.inner).ok_or_else(missing)?;
    if pair.tail != Some(shape)
        || outer.exits().len()
            != 2 + usize::from(matches!(shape.kind, IterationExitKind::Return(_)))
        || inner.exits().len() != 2
    {
        return Err(missing());
    }
    let end_bindings = |plan: &MapIterationOwnershipPlan| {
        let descriptor = plan.descriptor();
        [
            IterationCleanupAction::EndBinding {
                statement: descriptor.statement(),
                symbol: descriptor.value_binding(),
            },
            IterationCleanupAction::EndBinding {
                statement: descriptor.statement(),
                symbol: descriptor.key_binding(),
            },
        ]
    };
    let finish = |statement| {
        [
            IterationCleanupAction::FinishProvider(statement),
            IterationCleanupAction::EndSource(statement),
        ]
    };
    let mut transfer = vec![IterationCleanupAction::Drop(shape.inner_drop)];
    transfer.extend(end_bindings(inner));
    if !matches!(shape.kind, IterationExitKind::Continue(_)) {
        transfer.extend(finish(pair.inner))
    }
    if matches!(shape.kind, IterationExitKind::Return(_)) {
        transfer.push(IterationCleanupAction::Drop(
            shape.outer_return_drop.ok_or_else(missing)?,
        ));
        transfer.extend(end_bindings(outer));
        transfer.extend(finish(pair.outer));
    } else if shape.outer_return_drop.is_some() {
        return Err(missing());
    }
    let mut fallthrough = vec![IterationCleanupAction::Drop(shape.outer_fallthrough_drop)];
    fallthrough.extend(end_bindings(outer));
    let inner_finish = finish(pair.inner);
    let outer_finish = finish(pair.outer);
    let transfer_point = exit_shape::point(shape.kind, pair.inner, shape.inner_body);
    for (plan, kind, point, actions) in [
        (inner, shape.kind, transfer_point, transfer.as_slice()),
        (
            inner,
            IterationExitKind::Exhaustion,
            DropPoint::LoopExit(pair.inner),
            inner_finish.as_slice(),
        ),
        (
            outer,
            IterationExitKind::Fallthrough,
            DropPoint::AfterStatement(shape.outer_body),
            fallthrough.as_slice(),
        ),
        (
            outer,
            IterationExitKind::Exhaustion,
            DropPoint::LoopExit(pair.outer),
            outer_finish.as_slice(),
        ),
    ] {
        event(owned, plan, kind, point, actions, span)?;
    }
    if matches!(shape.kind, IterationExitKind::Return(_)) {
        event(owned, outer, shape.kind, transfer_point, &transfer, span)?;
    }
    Ok(())
}

fn event(
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    kind: IterationExitKind,
    point: DropPoint,
    actions: &[IterationCleanupAction],
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let exit =
        unique(plan.exits().iter().filter(|exit| exit.kind() == kind)).ok_or_else(missing)?;
    if exit.point() != point
        || owned.cleanup_conditions().get(exit.condition()) != Some(&CleanupCondition::Always)
        || exit.actions() != actions
        || owned
            .cleanup_steps()
            .iter()
            .filter_map(|(p, action)| (*p == point).then_some(*action))
            .collect::<Vec<_>>()
            != actions
    {
        return Err(missing());
    }
    Ok(())
}

/// The pair is the admission unit. Return can name two plans, and outer normal
/// cleanup is an AfterStatement point, so no unique-descriptor shortcut applies.
pub(in super::super) fn supported_drop(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> bool {
    typed.map_iterations().iter().any(|descriptor| {
        super::super::qualify::qualify(parsed, names, typed, owned, *descriptor)
            .ok()
            .and_then(|qualified| qualified.nested)
            .filter(|pair| pair.outer == descriptor.statement())
            .and_then(|pair| pair.tail)
            .is_some_and(|shape| {
                fact == shape.inner_drop
                    || fact == shape.outer_fallthrough_drop
                    || Some(fact) == shape.outer_return_drop
            })
    })
}
