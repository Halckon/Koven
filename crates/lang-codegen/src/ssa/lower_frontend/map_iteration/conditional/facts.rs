//! Complete plan exits and owner incoming provenance; no fixed arm or payload syntax.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupConditionId, CleanupOwnerValue, CleanupSelectorSource, DropFact, DropTarget,
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
    let target = source_target(flow.source);
    let facts = exit
        .actions()
        .iter()
        .filter_map(|action| match action {
            IterationCleanupAction::Drop(fact) if fact.target() == target => Some(*fact),
            _ => None,
        })
        .collect::<Vec<_>>();
    let selected = match facts.as_slice() {
        [] => None,
        [fact] if fact.point() == point && owned.drops().contains(fact) => Some(*fact),
        _ => return Err(missing()),
    };
    let published = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.point() == point && fact.target() == target)
        .collect::<Vec<_>>();
    if published != selected.into_iter().collect::<Vec<_>>() {
        return Err(missing());
    }
    Ok(selected)
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
    for binding in plan
        .closure_flow()
        .header()
        .iter()
        .chain(plan.closure_flow().exit())
    {
        if source_target(shape.source) != DropTarget::Named(binding.symbol())
            || !binding.origins().is_empty()
        {
            return Err(unsupported());
        }
    }
    let flow = source_owner::Flow {
        statement: shape.statement,
        source: shape.source,
        span,
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
        let mut suffix = Vec::new();
        if exit.kind() != IterationExitKind::Exhaustion {
            suffix.extend([
                IterationCleanupAction::EndBinding {
                    statement: shape.statement,
                    symbol: descriptor.value_binding(),
                },
                IterationCleanupAction::EndBinding {
                    statement: shape.statement,
                    symbol: descriptor.key_binding(),
                },
            ]);
        }
        if exit_shape::terminal(exit.kind()) {
            suffix.extend([
                IterationCleanupAction::FinishProvider(shape.statement),
                IterationCleanupAction::EndSource(shape.statement),
            ]);
            if let Some(fact) = source_drop {
                suffix.push(IterationCleanupAction::Drop(fact));
            }
        }
        if !exit.actions().ends_with(&suffix) {
            return Err(unsupported());
        }
        let prefix = &exit.actions()[..exit.actions().len() - suffix.len()];
        if exit.kind() == IterationExitKind::Exhaustion && !prefix.is_empty() {
            return Err(missing());
        }
        for action in prefix {
            let IterationCleanupAction::Drop(fact) = action else {
                return Err(unsupported());
            };
            if fact.target() == target
                || fact.point() != exit.point()
                || !owned.drops().contains(fact)
                || fact.capture_slot().is_some()
                || fact.instance_address().is_some()
            {
                return Err(missing());
            }
            // Body locals are proved by their own original Expression owner. Their
            // runtime SSA value may be transported through ordinary block parameters.
            let DropTarget::Named(root) = fact.target() else {
                return Err(unsupported());
            };
            let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
            let owner = fact.owner().ok_or_else(missing)?;
            let Some(CleanupOwnerValue::Expression { expression, origin }) =
                owned.cleanup_conditions().owner_value(owner)
            else {
                return Err(unsupported());
            };
            let producer = parsed
                .ast()
                .expressions()
                .get(*expression)
                .map_err(|_| missing())?;
            if symbol.kind() != SymbolKind::Variable
                || fact.value_origin() != symbol.span()
                || producer.span() != *origin
            {
                return Err(missing());
            }
            if fact.condition().is_some_and(|condition| {
                owned.cleanup_conditions().get(condition) != Some(&CleanupCondition::Always)
            }) {
                return Err(unsupported());
            }
        }
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
    let named_owned = plan.source_access() == MapIterationSourceAccess::Owned;
    if (!named_owned && !plan.closure_phis().is_empty())
        || (named_owned && plan.closure_phis().len() != 2)
    {
        return Err(unsupported());
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
            || (kind == IterationPhiIncomingKind::Entry
                && owned.cleanup_conditions().get(incoming.condition())
                    != Some(&CleanupCondition::Always))
        {
            return Err(missing());
        }
        condition(parsed, owned, incoming.condition(), span)?;
        if !named_owned {
            if !incoming.bindings().is_empty() {
                return Err(missing());
            }
            continue;
        }
        let DropTarget::Named(root) = target else {
            return Err(missing());
        };
        let origin = names
            .symbols()
            .get(root.index())
            .ok_or_else(missing)?
            .span();
        let phi = source_owner::phi(owned.cleanup_conditions(), plan, root, boundary, origin)?;
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
        condition(parsed, owned, binding.available_when(), span)?;
        match binding.values() {
            [] if owned.cleanup_conditions().get(binding.available_when())
                == Some(&CleanupCondition::Never) => {}
            [value] if value.condition() == binding.available_when() => {
                source_owner::flow_condition(parsed, names, owned, flow, value.source(), span)?;
            }
            _ => return Err(missing()),
        }
        if boundary == IterationPhiBoundary::Header && binding.values().is_empty() {
            return Err(unsupported());
        }
    }
    if plan.source_access() == MapIterationSourceAccess::Shared {
        if !source_facts.is_empty() {
            return Err(missing());
        }
        return Ok(None);
    }
    // A future Drop is selected only from the validated parameter provenance or
    // the formed-local epoch ledger. An absent plan Drop never invents an owner.
    let ledger = match shape.source {
        SourceIdentity::Temporary(source) => owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == DropTarget::Temporary(source))
            .collect::<Vec<_>>(),
        SourceIdentity::Parameter(root) => owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == DropTarget::Named(root))
            .collect::<Vec<_>>(),
        SourceIdentity::Local {
            symbol,
            initializer: producer,
        }
        | SourceIdentity::Formed { symbol, producer } => {
            let formation = formed::formation(
                parsed,
                names,
                typed,
                owned,
                symbol,
                descriptor.source_type(),
                span,
            )?;
            if formation.producer != producer {
                return Err(missing());
            }
            formed::drops(parsed, owned, symbol, &formation)?
        }
    };
    for fact in &source_facts {
        if !ledger.contains(fact) {
            return Err(missing());
        }
        validate_source_fact(parsed, names, owned, flow, *fact, span)?;
    }
    // Future-use CFG owners need a separate late-lifetime proof. Do not treat
    // absence of a source Drop at this provider as either death or permission.
    if ledger.iter().any(|fact| {
        !source_facts.contains(fact)
            && validate_source_fact(parsed, names, owned, flow, *fact, span).is_ok()
    }) {
        return Err(unsupported());
    }
    let drop = source_facts.first().copied().ok_or_else(unsupported)?;
    if matches!(shape.source, SourceIdentity::Temporary(_))
        && (ledger.len() != source_facts.len()
            || ledger
                .iter()
                .any(|fact| source_facts.iter().filter(|actual| *actual == fact).count() != 1))
    {
        return Err(missing());
    }
    Ok(Some(source_owner::Proof {
        flow: Some(flow),
        drop,
        transfer: None,
        conditional_return: None,
    }))
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
