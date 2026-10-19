//! Complete plan exits and owner incoming provenance; no fixed arm or payload syntax.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupConditionId, CleanupSelection, CleanupSelectorSource, DropFact, DropTarget,
    IterationPhiBoundary, IterationPhiIncomingKind, IterationPhiPresenceSource,
};
use std::collections::BTreeSet;

pub(in super::super) fn source_target(source: SourceIdentity) -> DropTarget {
    match source {
        SourceIdentity::Parameter(root)
        | SourceIdentity::Local { symbol: root, .. }
        | SourceIdentity::Formed { symbol: root, .. } => DropTarget::Named(root),
        SourceIdentity::Temporary(source) => DropTarget::Temporary(source),
    }
}

/// None is proved by the exact exit's complete actions, never a failed global search.
pub(in super::super) fn source_drop(
    owned: &OwnershipCheckedFile,
    flow: source_owner::Flow,
    point: DropPoint,
) -> Result<Option<DropFact>, LoweringError> {
    let plan = owned.map_iteration(flow.statement).ok_or(LoweringError {
        kind: LoweringErrorKind::MissingFact,
        span: Some(flow.span),
    })?;
    let missing = || error(LoweringErrorKind::MissingFact, flow.span);
    let exit =
        unique(plan.exits().iter().filter(|exit| exit.point() == point)).ok_or_else(missing)?;
    actions::source_drop(owned, plan, flow.source, exit, flow.span)
}

pub(in super::super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    shape: Shape,
    span: Span,
) -> Result<Option<source_owner::Proof>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let descriptor = plan.descriptor();
    if descriptor.statement() != shape.statement
        || owned.map_iteration(shape.statement) != Some(plan)
        || typed.map_iteration(shape.statement) != Some(descriptor)
    {
        return Err(missing());
    }
    if !plan.capture_graph().nodes().is_empty() {
        return Err(unsupported());
    }
    let symbols = plan
        .closure_flow()
        .header()
        .iter()
        .map(|binding| binding.symbol())
        .collect::<Vec<_>>();
    if plan
        .closure_flow()
        .header()
        .iter()
        .chain(plan.closure_flow().exit())
        .any(|binding| !binding.origins().is_empty())
        || symbols
            .iter()
            .enumerate()
            .any(|(index, symbol)| symbols[..index].contains(symbol))
        || plan
            .closure_flow()
            .exit()
            .iter()
            .map(|binding| binding.symbol())
            .collect::<Vec<_>>()
            != symbols
        || plan.closure_phis().len() != symbols.len() * 2
    {
        return Err(unsupported());
    }
    let flow = source_owner::Flow {
        statement: shape.statement,
        source: shape.source,
        span,
        late: None,
    };
    let target = source_target(shape.source);
    let mut source_facts = Vec::new();
    for exit in plan.exits() {
        if plan
            .exits()
            .iter()
            .filter(|other| other.kind() == exit.kind())
            .count()
            != 1
            || exit.point() != exit_shape::point(exit.kind(), shape.statement, shape.body)
            || owned
                .cleanup_steps()
                .iter()
                .filter_map(|(point, action)| (*point == exit.point()).then_some(*action))
                .collect::<Vec<_>>()
                != exit.actions()
        {
            return Err(missing());
        }
        condition(parsed, owned, exit.condition(), span)?;
        let source_drop = source_drop(owned, flow, exit.point())?;
        if let Some(fact) = source_drop {
            if !exit_shape::terminal(exit.kind()) {
                return Err(missing());
            }
            source_facts.push(fact);
        }
        actions::validate(
            (parsed, names, typed, owned),
            plan,
            shape,
            exit,
            source_drop,
            span,
        )?;
    }
    if plan
        .exits()
        .iter()
        .filter(|exit| exit.kind() == IterationExitKind::Exhaustion)
        .count()
        != 1
    {
        return Err(missing());
    }
    let mut expected = vec![(
        IterationPhiIncomingKind::Entry,
        IterationPhiBoundary::Header,
        DropPoint::AfterExpression(descriptor.source()),
        None,
    )];
    for exit in plan.exits() {
        let (kind, boundary) = match exit.kind() {
            IterationExitKind::Continue(e) => (
                IterationPhiIncomingKind::Continue(e),
                IterationPhiBoundary::Header,
            ),
            IterationExitKind::Fallthrough => (
                IterationPhiIncomingKind::Fallthrough,
                IterationPhiBoundary::Header,
            ),
            IterationExitKind::Break(e) => (
                IterationPhiIncomingKind::Break(e),
                IterationPhiBoundary::Exit,
            ),
            IterationExitKind::Exhaustion => (
                IterationPhiIncomingKind::Exhaustion,
                IterationPhiBoundary::Exit,
            ),
            IterationExitKind::Return(_) => continue,
        };
        expected.push((kind, boundary, exit.point(), Some(exit.condition())));
    }
    if plan.closure_phi_incomings().len() != expected.len() {
        return Err(missing());
    }
    for (kind, boundary, point, expected_condition) in expected {
        let incoming = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|incoming| incoming.kind() == kind),
        )
        .ok_or_else(missing)?;
        if incoming.point() != point
            || incoming.boundary() != boundary
            || expected_condition.is_some_and(|condition| incoming.condition() != condition)
        {
            return Err(missing());
        }
        condition(parsed, owned, incoming.condition(), span)?;
        if kind == IterationPhiIncomingKind::Entry
            && !entry_active(parsed, owned, incoming.condition(), span)?
        {
            return Err(missing());
        }
        if incoming.bindings().len() != symbols.len() {
            return Err(missing());
        }
        for root in &symbols {
            let phi = source_owner::phi(owned.cleanup_conditions(), plan, *root, boundary, span)?;
            let binding = unique(
                incoming
                    .bindings()
                    .iter()
                    .filter(|binding| binding.target() == phi.owner()),
            )
            .ok_or_else(missing)?;
            if binding.availability_selector() != phi.availability_selector()
                || binding.presence_source() != IterationPhiPresenceSource::StaticConditions
                || !binding.capture_slots_to_clear().is_empty()
                || !binding.root_sources().is_empty()
                || !binding.selector_writes().is_empty()
                || !binding.origins().is_empty()
            {
                return Err(missing());
            }
            condition(parsed, owned, binding.available_when(), span)?;
            match binding.values() {
                [] if owned.cleanup_conditions().get(binding.available_when())
                    == Some(&CleanupCondition::Never) => {}
                [value] if value.condition() == binding.available_when() => {
                    if target == DropTarget::Named(*root) {
                        source_owner::flow_condition(
                            parsed,
                            names,
                            owned,
                            flow,
                            value.source(),
                            span,
                        )?;
                    } else {
                        actions::binding_condition(
                            parsed,
                            names,
                            typed,
                            owned,
                            *root,
                            value.source(),
                            span,
                        )?;
                    }
                }
                _ => return Err(missing()),
            }
            if boundary == IterationPhiBoundary::Header && binding.values().is_empty() {
                return Err(unsupported());
            }
        }
    }

    // Non-provider roots may survive this loop under a different binding epoch.
    // Validate their complete published terminal facts before generic Drop support
    // can consume them; a malformed owner never falls through to staging.
    for fact in owned.drops().iter().copied() {
        formed::retained::proof((parsed, names, typed, owned), plan, fact, span)?;
    }

    if plan.source_access() == MapIterationSourceAccess::Shared {
        if !source_facts.is_empty() {
            return Err(missing());
        }
        return Ok(None);
    }
    // A future Drop is selected only from the validated parameter provenance or
    // the formed-local epoch ledger. An absent plan Drop never invents an owner.
    let (ledger, formation) = match shape.source {
        SourceIdentity::Temporary(source) => (
            owned
                .drops()
                .iter()
                .copied()
                .filter(|fact| fact.target() == DropTarget::Temporary(source))
                .collect::<Vec<_>>(),
            None,
        ),
        SourceIdentity::Parameter(root) => (
            owned
                .drops()
                .iter()
                .copied()
                .filter(|fact| fact.target() == DropTarget::Named(root))
                .collect::<Vec<_>>(),
            None,
        ),
        SourceIdentity::Local {
            symbol,
            initializer,
        } => {
            let formation = formed::formation(
                parsed,
                names,
                typed,
                owned,
                symbol,
                descriptor.source_type(),
                span,
            )?;
            if formation.birth != crate::ssa::map_source::Birth::Call(initializer) {
                return Err(missing());
            }
            (
                formed::drops(parsed, owned, symbol, &formation)?,
                Some(formation),
            )
        }
        SourceIdentity::Formed { symbol, birth } => {
            let formation = formed::formation(
                parsed,
                names,
                typed,
                owned,
                symbol,
                descriptor.source_type(),
                span,
            )?;
            if formation.birth != birth {
                return Err(missing());
            }
            (
                formed::drops(parsed, owned, symbol, &formation)?,
                Some(formation),
            )
        }
    };
    for fact in &source_facts {
        if !ledger.contains(fact) {
            return Err(missing());
        }
        validate_source_fact(parsed, names, owned, flow, *fact, span)?;
    }
    // Resource storage remains live across all non-return exits until its actual
    // lexical Drop. This is an explicit capability, not an absent-plan inference.
    let outside = ledger
        .iter()
        .copied()
        .filter(|fact| !source_facts.contains(fact))
        .collect::<Vec<_>>();
    let mut inherited = !outside.is_empty();
    for fact in &outside {
        // Inspect every fact, preserving malformed ancestor evidence as Missing
        // even if a different fact describes an unsupported future lifetime.
        let retained = retained::ancestor_fact((parsed, names, typed, owned), flow, *fact, span)?;
        if retained {
            validate_source_fact(parsed, names, owned, flow, *fact, span)?;
        }
        inherited &= retained;
    }
    let late = if inherited {
        let nonlocal = outside
            .iter()
            .copied()
            .filter(|fact| !plan.exits().iter().any(|exit| exit.point() == fact.point()))
            .collect::<Vec<_>>();
        match nonlocal.as_slice() {
            [] => None,
            [fact] => Some(*fact),
            _ => return Err(unsupported()),
        }
    } else {
        match outside.as_slice() {
            [] => None,
            [fact] => {
                validate_source_fact(parsed, names, owned, flow, *fact, span)?;
                let DropTarget::Named(root) = target else {
                    return Err(unsupported());
                };
                let origin = names
                    .symbols()
                    .get(root.index())
                    .ok_or_else(missing)?
                    .span();
                let resource = typed
                    .is_resource_type(descriptor.source_type())
                    .ok_or_else(missing)?;
                if resource {
                    let scope = resource::scope(parsed, shape.statement, origin, span)?;
                    if !resource::normal_point(parsed, scope, span, fact.point())
                        && !formed::replacement_point(formation.as_ref(), fact.point())
                    {
                        return Err(unsupported());
                    }
                } else {
                    let event = crate::ssa::map_source::drop_event(parsed, fact.point())
                        .ok_or_else(unsupported)?;
                    if !crate::ssa::map_source::future_reference(
                        parsed,
                        span,
                        event.span,
                        names.references().iter().filter_map(|reference| {
                            (reference.target()
                                == &lang_frontend::name_resolution::ReferenceTarget::Symbol(root))
                                .then_some(reference.span())
                        }),
                    ) {
                        return Err(unsupported());
                    }
                }
                let exit = source_owner::phi(
                    owned.cleanup_conditions(),
                    plan,
                    root,
                    IterationPhiBoundary::Exit,
                    span,
                )?;
                let live =
                    source_owner::flow_condition(parsed, names, owned, flow, exit.owner(), span)?;
                if resource
                    && (fact.owner() != Some(exit.owner()) || fact.condition() != Some(live))
                    && !super::successor::terminal(
                        (parsed, names, typed, owned),
                        flow,
                        *fact,
                        formation.as_ref(),
                        span,
                    )?
                {
                    return Err(missing());
                }
                if owned.cleanup_conditions().get(live) == Some(&CleanupCondition::Never) {
                    return Err(missing());
                }
                Some(*fact)
            }
            _ => return Err(unsupported()),
        }
    };
    let drop = late
        .or_else(|| source_facts.first().copied())
        .or_else(|| inherited.then(|| outside.first().copied()).flatten());
    let disposition = if let Some(drop) = drop {
        source_owner::Disposition::Drop(drop)
    } else {
        source_owner::Disposition::Delivery(source_owner::delivery::qualify(
            parsed, names, typed, owned, flow, span,
        )?)
    };
    if matches!(shape.source, SourceIdentity::Temporary(_))
        && (ledger.len() != source_facts.len()
            || ledger
                .iter()
                .any(|fact| source_facts.iter().filter(|actual| *actual == fact).count() != 1))
    {
        return Err(missing());
    }
    Ok(Some(source_owner::Proof {
        flow: Some(source_owner::Flow { late, ..flow }),
        disposition,
        transfer: None,
        conditional_return: None,
    }))
}

/// At provider entry the ordinary CFG has already selected these lexical arms.
/// The selector keeps its published identity: no source condition is evaluated
/// again, and no choice from an earlier control/owner instance is consulted.
/// `condition` has checked every child, including the unselected arms.
fn entry_active(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    condition: CleanupConditionId,
    span: Span,
) -> Result<bool, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let inside = |statement| -> Result<bool, LoweringError> {
        let branch = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| missing())?
            .span();
        Ok(branch.source_id() == span.source_id()
            && branch.start() <= span.start()
            && span.end() <= branch.end())
    };
    let (selector, branches) = match owned.cleanup_conditions().get(condition) {
        Some(CleanupCondition::Always) => return Ok(true),
        Some(CleanupCondition::Never) => return Ok(false),
        Some(CleanupCondition::Choice { selector, branches }) => (selector, branches),
        None => return Err(missing()),
    };
    let selector = owned
        .cleanup_conditions()
        .selector(*selector)
        .ok_or_else(missing)?;
    let CleanupSelectorSource::Control(control) = selector.source() else {
        return Ok(false);
    };
    let node = parsed
        .ast()
        .expressions()
        .get(control)
        .map_err(|_| missing())?;
    if selector.origin() != node.span() || selector.branch_count() != 2 || branches.len() != 2 {
        return Err(missing());
    }
    let branch = match (node.payload(), selector.selection()) {
        (
            Expression::If {
                then_branch,
                else_branch,
                ..
            },
            CleanupSelection::Branch,
        ) => {
            if inside(*then_branch)? {
                0
            } else if let Some(else_branch) = else_branch {
                if !inside(*else_branch)? {
                    return Ok(false);
                }
                1
            } else {
                return Ok(false);
            }
        }
        (
            Expression::When { entries, .. },
            CleanupSelection::WhenAlternative { entry, alternative },
        ) => {
            let selected = entries.get(entry).ok_or_else(missing)?;
            if selected.else_span.is_some() || alternative >= selected.conditions.len() {
                return Err(missing());
            }
            let mut containing = None;
            for (index, candidate) in entries.iter().enumerate() {
                if inside(candidate.body)? && containing.replace(index).is_some() {
                    return Err(missing());
                }
            }
            match containing {
                // A single-alternative body is entered only over its match edge.
                Some(index) if index == entry && selected.conditions.len() == 1 => 0,
                // A later entry is reached only over this alternative's next edge.
                Some(index) if index > entry => 1,
                _ => return Ok(false),
            }
        }
        _ => return Ok(false),
    };
    entry_active(parsed, owned, branches[branch], span)
}

fn validate_source_fact(
    parsed: &ParsedFile,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    flow: source_owner::Flow,
    fact: DropFact,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    if fact.target() != source_target(flow.source)
        || fact.capture_slot().is_some()
        || fact.instance_address().is_some()
    {
        return Err(missing());
    }
    if let Some(actual) = fact.condition() {
        condition(parsed, owned, actual, span)?;
    }
    let live = source_owner::flow_condition(
        parsed,
        names,
        owned,
        flow,
        fact.owner().ok_or_else(missing)?,
        span,
    )?;
    let plan = owned.map_iteration(flow.statement).ok_or_else(missing)?;
    let context = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.point() == fact.point()),
    )
    .map(|exit| exit.condition());
    let condition_matches = match (fact.condition(), context) {
        (Some(actual), Some(path)) => {
            source_owner::provenance::equal_on_path(owned.cleanup_conditions(), actual, live, path)
        }
        (Some(actual), None) => actual == live,
        (None, _) => owned.cleanup_conditions().get(live) == Some(&CleanupCondition::Always),
    };
    if !condition_matches {
        return Err(missing());
    }
    match flow.source {
        SourceIdentity::Parameter(root) => {
            if fact.value_origin()
                != names
                    .symbols()
                    .get(root.index())
                    .ok_or_else(missing)?
                    .span()
            {
                return Err(missing());
            }
        }
        SourceIdentity::Temporary(expression) => {
            if fact.value_origin()
                != parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .map_err(|_| missing())?
                    .span()
            {
                return Err(missing());
            }
        }
        SourceIdentity::Local { .. } | SourceIdentity::Formed { .. } => {}
    }
    Ok(())
}

fn condition(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    root: CleanupConditionId,
    span: Span,
) -> Result<(), LoweringError> {
    fn visit(
        parsed: &ParsedFile,
        owned: &OwnershipCheckedFile,
        id: CleanupConditionId,
        path: &mut BTreeSet<CleanupConditionId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        if !path.insert(id) {
            return Err(missing());
        }
        if let CleanupCondition::Choice { selector, branches } =
            owned.cleanup_conditions().get(id).ok_or_else(missing)?
        {
            let selector = owned
                .cleanup_conditions()
                .selector(*selector)
                .ok_or_else(missing)?;
            if selector.branch_count() != branches.len() || branches.is_empty() {
                return Err(missing());
            }
            match selector.source() {
                CleanupSelectorSource::Control(expression) => {
                    if parsed
                        .ast()
                        .expressions()
                        .get(expression)
                        .map_err(|_| missing())?
                        .span()
                        != selector.origin()
                    {
                        return Err(missing());
                    }
                }
                CleanupSelectorSource::IterationPhi { statement, .. } => {
                    if owned.map_iteration(statement).is_none() {
                        return Err(missing());
                    }
                }
                CleanupSelectorSource::CaptureLoan { .. } => {
                    return Err(error(LoweringErrorKind::UnsupportedNode, span));
                }
            }
            for child in branches {
                visit(parsed, owned, *child, path, span)?;
            }
        }
        path.remove(&id);
        Ok(())
    }
    visit(parsed, owned, root, &mut BTreeSet::new(), span)
}
