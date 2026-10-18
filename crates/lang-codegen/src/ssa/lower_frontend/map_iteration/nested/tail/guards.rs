//! Public FE identities for real constructors, their owners, and all GO inputs.
use super::*;
use lang_frontend::{
    ownership_checking::{
        CleanupConditions, CleanupOwnerValue, CleanupSelection, CleanupSelectorSource, DropTarget,
        IterationClosurePhiBinding, IterationPhiIncomingBinding, IterationPhiIncomingKind,
        IterationPhiPresenceSource,
    },
    parser::{NameMarker, VariableKind},
    type_checking::{ConstructionTarget, NominalKind},
};

pub(super) fn local(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    statement: StatementId,
    span: Span,
) -> Result<Local, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| missing())?;
    let Statement::LocalVariable { declaration } = node.payload() else {
        return Err(unsupported());
    };
    let item = parsed
        .ast()
        .items()
        .get(*declaration)
        .map_err(|_| missing())?;
    let Item::Variable {
        kind: VariableKind::Val,
        name: NameMarker::Present(origin),
        initializer,
        ..
    } = item.payload()
    else {
        return Err(unsupported());
    };
    let symbol = unique(
        names
            .symbols()
            .iter()
            .filter(|symbol| symbol.span() == *origin && symbol.kind() == SymbolKind::Variable),
    )
    .ok_or_else(missing)?;
    let ty = typed.symbol_type(symbol.id()).ok_or_else(missing)?;
    let TypeKind::Nominal { nominal, arguments } = typed.types().get(ty).ok_or_else(missing)?
    else {
        return Err(unsupported());
    };
    let nominal =
        unique(typed.nominals().iter().filter(|item| item.id() == *nominal)).ok_or_else(missing)?;
    if nominal.kind() != NominalKind::Class
        || !arguments.is_empty()
        || !nominal.type_parameters().is_empty()
        || !nominal.fields().is_empty()
        || !nominal.variants().is_empty()
        || !nominal.interfaces().is_empty()
    {
        return Err(unsupported());
    }
    match typed.is_resource_type(ty) {
        Some(true) => {}
        Some(false) => return Err(unsupported()),
        None => return Err(missing()),
    }
    let deinit = nominal.deinit().ok_or_else(missing)?;
    let init = parsed
        .ast()
        .expressions()
        .get(*initializer)
        .map_err(|_| missing())?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = init.payload()
    else {
        return Err(unsupported());
    };
    if !arguments.is_empty() || !type_arguments.is_empty() {
        return Err(unsupported());
    }
    let callee = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|_| missing())?;
    if callee.payload() != &Expression::Name {
        return Err(unsupported());
    }
    let reference = unique(
        names
            .references()
            .iter()
            .filter(|r| r.span() == callee.span()),
    )
    .ok_or_else(missing)?;
    if reference.target() != &ReferenceTarget::Symbol(nominal.id().symbol()) {
        return Err(missing());
    }
    let construction = typed.construction(*initializer).ok_or_else(missing)?;
    if construction.expression() != *initializer
        || construction.target() != ConstructionTarget::Nominal(nominal.id())
        || construction.result_type() != ty
        || typed.expression_type(*initializer) != Some(ty)
        || !construction.instance().type_arguments().is_empty()
        || !construction.arguments().is_empty()
        || symbol.is_synthetic()
        || !owned.formed_owned_locals().contains(&symbol.id())
        || deinit.owner() != nominal.id()
        || deinit.receiver_type() != ty
        || deinit.receiver_mode() != ParameterMode::Borrow
    {
        return Err(missing());
    }
    let deinit_node = parsed
        .ast()
        .items()
        .get(deinit.item())
        .map_err(|_| missing())?;
    if !matches!(deinit_node.payload(), Item::Deinit { body, .. } if *body == deinit.body())
        || !matches!(
            parsed
                .ast()
                .statements()
                .get(deinit.body())
                .map_err(|_| missing())?
                .payload(),
            Statement::Block { .. }
        )
    {
        return Err(missing());
    }
    let nominal_symbol = names
        .symbols()
        .get(nominal.id().symbol().index())
        .ok_or_else(missing)?;
    let declaration = unique(parsed.ast().items().iter().filter_map(
        |(_, item)| match item.payload() {
            Item::Classifier(declaration)
                if declaration.name == NameMarker::Present(nominal_symbol.span()) =>
            {
                Some(declaration)
            }
            _ => None,
        },
    ))
    .ok_or_else(missing)?;
    if !declaration
        .body
        .as_ref()
        .is_some_and(|body| body.members.contains(&deinit.item()))
    {
        return Err(missing());
    }
    Ok(Local {
        symbol: symbol.id(),
        initializer: *initializer,
        initializer_span: init.span(),
        origin: *origin,
        ty,
        deinit: deinit.item(),
    })
}

type GuardProof = (
    CleanupOwnerValueId,
    DropFact,
    OwnerProof,
    DropFact,
    Option<DropFact>,
);

/// One immutable pair's public Guard evidence, before any SSA state exists.
pub(super) struct ProofInput<'a> {
    pub(super) parsed: &'a ParsedFile,
    pub(super) owned: &'a OwnershipCheckedFile,
    pub(super) plans: [&'a MapIterationOwnershipPlan; 2],
    pub(super) outer_body: StatementId,
    pub(super) guards: [Local; 2],
    pub(super) kind: IterationExitKind,
    pub(super) span: Span,
}

pub(super) fn prove(input: ProofInput<'_>) -> Result<GuardProof, LoweringError> {
    let ProofInput {
        parsed,
        owned,
        plans: [outer, inner],
        outer_body,
        guards: [outer_guard, inner_guard],
        kind,
        span,
    } = input;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let conditions = owned.cleanup_conditions();
    let inner_span = parsed
        .ast()
        .statements()
        .get(inner.descriptor().statement())
        .map_err(|_| missing())?
        .span();
    // Empty closure layouts plus StaticConditions on every actual input prove
    // that GO needs no captured-instance presence read. No private FE bit is read.
    if inner.closure_phis().len() != 2
        || !inner.capture_graph().nodes().is_empty()
        || !outer.closure_phis().is_empty()
        || !outer.capture_graph().nodes().is_empty()
        || !outer.closure_flow().header().is_empty()
        || !outer.closure_flow().exit().is_empty()
    {
        return Err(missing());
    }
    for bindings in [inner.closure_flow().header(), inner.closure_flow().exit()] {
        let [binding] = bindings else {
            return Err(missing());
        };
        if binding.symbol() != outer_guard.symbol || !binding.origins().is_empty() {
            return Err(missing());
        }
    }
    let header = phi(
        conditions,
        inner,
        outer_guard,
        IterationPhiBoundary::Header,
        inner_span,
    )?;
    let exit = phi(
        conditions,
        inner,
        outer_guard,
        IterationPhiBoundary::Exit,
        inner_span,
    )?;
    if header.owner() == exit.owner()
        || header.availability_selector() == exit.availability_selector()
        || header.availability_condition() == exit.availability_condition()
    {
        return Err(missing());
    }
    let expected_count = 2 + usize::from(!matches!(kind, IterationExitKind::Return(_)));
    if inner.closure_phi_incomings().len() != expected_count {
        return Err(missing());
    }
    let entry = incoming(
        conditions,
        inner,
        header,
        IterationPhiIncomingKind::Entry,
        DropPoint::AfterExpression(inner.descriptor().source()),
        span,
    )?;
    let [seed] = entry.values() else {
        return Err(missing());
    };
    if conditions.get(entry.available_when()) != Some(&CleanupCondition::Always)
        || seed.condition() != entry.available_when()
        || !expression_owner(conditions, seed.source(), outer_guard)
        || seed.source() == header.owner()
        || seed.source() == exit.owner()
    {
        return Err(missing());
    }
    match kind {
        IterationExitKind::Continue(expression) => {
            preserved(
                conditions,
                inner,
                header,
                header,
                IterationPhiIncomingKind::Continue(expression),
                DropPoint::ControlTransfer(expression),
                span,
            )?;
        }
        IterationExitKind::Break(expression) => {
            preserved(
                conditions,
                inner,
                exit,
                header,
                IterationPhiIncomingKind::Break(expression),
                DropPoint::ControlTransfer(expression),
                span,
            )?;
        }
        IterationExitKind::Return(_) => {}
        _ => return Err(missing()),
    }
    preserved(
        conditions,
        inner,
        exit,
        header,
        IterationPhiIncomingKind::Exhaustion,
        DropPoint::LoopExit(inner.descriptor().statement()),
        span,
    )?;
    outer_incomings(conditions, outer, outer_body, span)?;
    let transfer = match kind {
        IterationExitKind::Continue(expression)
        | IterationExitKind::Break(expression)
        | IterationExitKind::Return(expression) => DropPoint::ControlTransfer(expression),
        _ => return Err(missing()),
    };
    let inner_drops = drops(owned, inner_guard.symbol);
    let [inner_drop] = inner_drops.as_slice() else {
        return Err(missing());
    };
    let inner_drop = *inner_drop;
    let inner_owner = inner_drop.owner().ok_or_else(missing)?;
    if !expression_owner(conditions, inner_owner, inner_guard)
        || inner_drop.condition().is_some()
        || inner_drop.point() != transfer
        || !ordinary_drop(inner_drop, inner_guard)
        || [seed.source(), header.owner(), exit.owner()].contains(&inner_owner)
    {
        return Err(missing());
    }
    let outer_drops = drops(owned, outer_guard.symbol);
    if outer_drops.len() != 1 + usize::from(matches!(kind, IterationExitKind::Return(_))) {
        return Err(missing());
    }
    let fallthrough = unique(
        outer_drops
            .iter()
            .copied()
            .filter(|fact| fact.point() == DropPoint::AfterStatement(outer_body)),
    )
    .ok_or_else(missing)?;
    check_drop(fallthrough, outer_guard, exit, span)?;
    let returned = if matches!(kind, IterationExitKind::Return(_)) {
        let returned = unique(
            outer_drops
                .iter()
                .copied()
                .filter(|fact| fact.point() == transfer),
        )
        .ok_or_else(missing)?;
        check_drop(returned, outer_guard, header, span)?;
        Some(returned)
    } else {
        None
    };
    // Include foreign targets sharing any admitted owner, not just named drops.
    for (owners, expected) in [
        (vec![inner_owner], inner_drops),
        (
            vec![seed.source(), header.owner(), exit.owner()],
            outer_drops,
        ),
    ] {
        let related = owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.owner().is_some_and(|owner| owners.contains(&owner)))
            .collect::<Vec<_>>();
        if related != expected {
            return Err(missing());
        }
    }
    Ok((
        inner_owner,
        inner_drop,
        OwnerProof {
            seed: seed.source(),
            header: boundary(header),
            exit: boundary(exit),
        },
        fallthrough,
        returned,
    ))
}

fn boundary(phi: &IterationClosurePhiBinding) -> Boundary {
    Boundary {
        boundary: phi.boundary(),
        owner: phi.owner(),
        selector: phi.availability_selector(),
        condition: phi.availability_condition(),
    }
}

fn expression_owner(
    conditions: &CleanupConditions,
    owner: CleanupOwnerValueId,
    local: Local,
) -> bool {
    matches!(conditions.owner_value(owner), Some(CleanupOwnerValue::Expression { expression, origin })
        if *expression == local.initializer && *origin == local.initializer_span)
}

fn drops(owned: &OwnershipCheckedFile, symbol: SymbolId) -> Vec<DropFact> {
    owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == DropTarget::Named(symbol))
        .collect()
}

fn ordinary_drop(fact: DropFact, local: Local) -> bool {
    fact.target() == DropTarget::Named(local.symbol)
        && fact.value_origin() == local.origin
        && fact.capture_slot().is_none()
        && fact.instance_address().is_none()
}

fn check_drop(
    fact: DropFact,
    local: Local,
    phi: &IterationClosurePhiBinding,
    span: Span,
) -> Result<(), LoweringError> {
    if !ordinary_drop(fact, local)
        || fact.owner() != Some(phi.owner())
        || fact.condition() != Some(phi.availability_condition())
    {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    Ok(())
}

fn phi<'a>(
    conditions: &CleanupConditions,
    plan: &'a MapIterationOwnershipPlan,
    local: Local,
    boundary: IterationPhiBoundary,
    span: Span,
) -> Result<&'a IterationClosurePhiBinding, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let statement = plan.descriptor().statement();
    let phi = unique(
        plan.closure_phis()
            .iter()
            .filter(|phi| phi.symbol() == local.symbol && phi.boundary() == boundary),
    )
    .ok_or_else(missing)?;
    if !matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
        statement: actual, boundary: actual_boundary, symbol, origin,
    }) if *actual == statement && *actual_boundary == boundary && *symbol == local.symbol && *origin == span)
        || !phi.root_nodes().is_empty()
        || !phi.capture_layout().is_empty()
        || !phi.origins().is_empty()
    {
        return Err(missing());
    }
    let selector = conditions
        .selector(phi.availability_selector())
        .ok_or_else(missing)?;
    // Each boundary has exactly this one binding and no capture layout, so its
    // unique availability slot is the first slot, independent of table IDs.
    if selector.source()
        != (CleanupSelectorSource::IterationPhi {
            statement,
            boundary,
            slot: 0,
        })
        || selector.selection() != CleanupSelection::IterationPresence
        || selector.origin() != span
        || selector.branch_count() != 2
    {
        return Err(missing());
    }
    let Some(CleanupCondition::Choice { selector, branches }) =
        conditions.get(phi.availability_condition())
    else {
        return Err(missing());
    };
    let [absent, present] = branches.as_slice() else {
        return Err(missing());
    };
    if *selector != phi.availability_selector()
        || conditions.get(*absent) != Some(&CleanupCondition::Never)
        || conditions.get(*present) != Some(&CleanupCondition::Always)
    {
        return Err(missing());
    }
    Ok(phi)
}

fn incoming<'a>(
    conditions: &CleanupConditions,
    plan: &'a MapIterationOwnershipPlan,
    phi: &IterationClosurePhiBinding,
    kind: IterationPhiIncomingKind,
    point: DropPoint,
    span: Span,
) -> Result<&'a IterationPhiIncomingBinding, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let incoming = unique(
        plan.closure_phi_incomings()
            .iter()
            .filter(|incoming| incoming.kind() == kind),
    )
    .ok_or_else(missing)?;
    if incoming.point() != point
        || incoming.boundary() != phi.boundary()
        || conditions.get(incoming.condition()) != Some(&CleanupCondition::Always)
    {
        return Err(missing());
    }
    let [binding] = incoming.bindings() else {
        return Err(missing());
    };
    if binding.target() != phi.owner()
        || binding.availability_selector() != phi.availability_selector()
        || binding.presence_source() != IterationPhiPresenceSource::StaticConditions
        || !binding.capture_slots_to_clear().is_empty()
        || !binding.root_sources().is_empty()
        || !binding.selector_writes().is_empty()
        || !binding.origins().is_empty()
    {
        return Err(missing());
    }
    Ok(binding)
}

fn preserved(
    conditions: &CleanupConditions,
    plan: &MapIterationOwnershipPlan,
    target: &IterationClosurePhiBinding,
    header: &IterationClosurePhiBinding,
    kind: IterationPhiIncomingKind,
    point: DropPoint,
    span: Span,
) -> Result<(), LoweringError> {
    let binding = incoming(conditions, plan, target, kind, point, span)?;
    let [value] = binding.values() else {
        return Err(error(LoweringErrorKind::MissingFact, span));
    };
    if binding.available_when() != header.availability_condition()
        || value.source() != header.owner()
        || value.condition() != header.availability_condition()
    {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    Ok(())
}

fn outer_incomings(
    conditions: &CleanupConditions,
    plan: &MapIterationOwnershipPlan,
    body: StatementId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    if plan.closure_phi_incomings().len() != 3 {
        return Err(missing());
    }
    for (kind, point, boundary) in [
        (
            IterationPhiIncomingKind::Entry,
            DropPoint::AfterExpression(plan.descriptor().source()),
            IterationPhiBoundary::Header,
        ),
        (
            IterationPhiIncomingKind::Fallthrough,
            DropPoint::AfterStatement(body),
            IterationPhiBoundary::Header,
        ),
        (
            IterationPhiIncomingKind::Exhaustion,
            DropPoint::LoopExit(plan.descriptor().statement()),
            IterationPhiBoundary::Exit,
        ),
    ] {
        let incoming = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|incoming| incoming.kind() == kind),
        )
        .ok_or_else(missing)?;
        if incoming.point() != point
            || incoming.boundary() != boundary
            || !incoming.bindings().is_empty()
            || conditions.get(incoming.condition()) != Some(&CleanupCondition::Always)
        {
            return Err(missing());
        }
    }
    Ok(())
}
