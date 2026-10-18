//! Exact shared cleanup lists retain each provider's own ordered frame segment.
use super::*;

fn exit_for<'a>(
    owned: &'a CompilationUnitOwnership,
    statement: UnitStatementId,
    shared: &UnitIterationExitPlan,
    span: Span,
) -> Result<(&'a UnitMapIterationOwnershipPlan, &'a UnitIterationExitPlan), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let plan = owned.map_iteration(statement).ok_or_else(bad)?;
    let mut exits = plan
        .exits()
        .iter()
        .filter(|exit| exit.point() == shared.point());
    let exit = exits.next().ok_or_else(bad)?;
    if exits.next().is_some()
        || exit.actions() != shared.actions()
        || owned.iteration_cleanup_at(shared.point()) != Some(shared.actions())
    {
        return Err(bad());
    }
    Ok((plan, exit))
}

fn target(plan: &UnitMapIterationOwnershipPlan) -> Option<UnitDropTarget> {
    match plan.source()? {
        UnitLoanTarget::Place(place) if place.is_root() => {
            Some(UnitDropTarget::Named(place.root()))
        }
        UnitLoanTarget::Temporary(expression) => Some(UnitDropTarget::Temporary(*expression)),
        _ => None,
    }
}

/// The source's segment begins at its own EndSource and ends at the next frame marker.
/// An equal root elsewhere in this shared list is never enough to claim its Drop.
fn owner_of(
    owned: &CompilationUnitOwnership,
    actions: &[Action],
    fact: UnitDropFact,
) -> Result<Option<UnitStatementId>, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, fact.value_origin());
    let mut ended = None;
    let mut selected = None;
    let mut occurrences = 0;
    for action in actions {
        match action {
            Action::EndSource(statement) => ended = Some(*statement),
            Action::EndBinding { .. } | Action::FinishProvider(_) | Action::EndElement(_) => {
                ended = None
            }
            Action::Drop(actual) if *actual == fact => {
                occurrences += 1;
                if let Some(statement) = ended {
                    let plan = owned.map_iteration(statement).ok_or_else(bad)?;
                    if target(plan) == Some(fact.target()) {
                        if !matches!(
                            plan.source_access(),
                            UnitIterationSourceAccess::Owned | UnitIterationSourceAccess::Temporary
                        ) {
                            return Err(bad());
                        }
                        let mut exits = plan
                            .exits()
                            .iter()
                            .filter(|exit| exit.point() == fact.point());
                        let exit = exits.next().ok_or_else(bad)?;
                        if exits.next().is_some() || exit.actions() != actions {
                            return Err(bad());
                        }
                        selected = Some(statement);
                    }
                }
            }
            _ => {}
        }
    }
    if occurrences != 1 {
        return Err(bad());
    }
    Ok(selected)
}

pub(super) fn source_drop(
    owned: &CompilationUnitOwnership,
    statement: UnitStatementId,
    exit: &UnitIterationExitPlan,
) -> Result<Option<UnitDropFact>, LoweringError> {
    let mut found = None;
    for action in exit.actions() {
        if let Action::Drop(fact) = action
            && owner_of(owned, exit.actions(), *fact)? == Some(statement)
            && found.replace(*fact).is_some()
        {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                fact.value_origin(),
            ));
        }
    }
    Ok(found)
}

pub(super) fn retained_by_ancestor(
    parsed: &ParsedFile,
    owned: &CompilationUnitOwnership,
    statement: UnitStatementId,
    fact: UnitDropFact,
) -> Result<bool, LoweringError> {
    let Some(actions) = owned.iteration_cleanup_at(fact.point()) else {
        return Ok(false);
    };
    let Some(owner) = owner_of(owned, actions, fact)? else {
        return Ok(false);
    };
    Ok(nested::ancestor(parsed, owner, statement))
}

fn required(plan: &UnitMapIterationOwnershipPlan, exit: &UnitIterationExitPlan) -> Vec<Action> {
    let d = plan.descriptor();
    let statement = d.statement();
    let mut actions = Vec::new();
    if exit.kind() != UnitIterationExitKind::Exhaustion {
        actions.extend([
            Action::EndBinding {
                statement,
                symbol: d.value_binding(),
            },
            Action::EndBinding {
                statement,
                symbol: d.key_binding(),
            },
        ]);
    }
    if matches!(
        exit.kind(),
        UnitIterationExitKind::Break(_)
            | UnitIterationExitKind::Return(_)
            | UnitIterationExitKind::Exhaustion
    ) {
        actions.extend([
            Action::FinishProvider(statement),
            Action::EndSource(statement),
        ]);
    }
    actions
}

pub(super) fn validate_exit(
    parsed: &ParsedFile,
    owned: &CompilationUnitOwnership,
    plan: &UnitMapIterationOwnershipPlan,
    shape: QualifiedConditional,
    exit: &UnitIterationExitPlan,
    facts: &[UnitDropFact],
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let statement = plan.descriptor().statement();
    let drop = shape.drop_at(owned, shape.source, exit.point())?;
    if drop.is_some_and(|fact| !facts.contains(&fact)) {
        return Err(bad());
    }
    let mut frames = Vec::new();
    let mut active: Option<(UnitStatementId, Vec<Action>, usize)> = None;
    for action in exit.actions() {
        match action {
            Action::Drop(fact) => {
                if fact.point() != exit.point() || !owned.drops().contains(fact) {
                    return Err(bad());
                }
                let owner = owner_of(owned, exit.actions(), *fact)?;
                if fact.target() == shape.source.drop_target()
                    && owner != Some(statement)
                    && owner.is_none_or(|owner| !nested::ancestor(parsed, owner, statement))
                {
                    return Err(bad());
                }
                if owner == Some(statement)
                    && (Some(*fact) != drop
                        || !facts.contains(fact)
                        || active.as_ref().is_none_or(|(id, expected, at)| {
                            *id != statement || *at != expected.len()
                        }))
                {
                    return Err(bad());
                }
            }
            Action::EndCallLoan(_) | Action::EndReceiverLoan(_) => {}
            Action::EndBinding { statement: id, .. }
            | Action::FinishProvider(id)
            | Action::EndSource(id) => {
                if active.as_ref().is_none_or(|(current, _, _)| current != id) {
                    if let Some((previous, expected, at)) = &active
                        && (*at != expected.len() || !nested::ancestor(parsed, *id, *previous))
                    {
                        return Err(bad());
                    }
                    if frames.contains(id) {
                        return Err(bad());
                    }
                    if *id != statement
                        && !nested::ancestor(parsed, *id, statement)
                        && !nested::ancestor(parsed, statement, *id)
                    {
                        return Err(bad());
                    }
                    let (other, other_exit) = exit_for(owned, *id, exit, span)?;
                    frames.push(*id);
                    active = Some((*id, required(other, other_exit), 0));
                }
                let (_, expected, at) = active.as_mut().ok_or_else(bad)?;
                if expected.get(*at) != Some(action) {
                    return Err(bad());
                }
                *at += 1;
            }
            _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        }
    }
    if !frames.contains(&statement)
        || active
            .as_ref()
            .is_none_or(|(_, expected, at)| *at != expected.len())
    {
        return Err(bad());
    }
    for fact in facts
        .iter()
        .copied()
        .filter(|fact| fact.point() == exit.point())
    {
        if Some(fact) != drop && !retained_by_ancestor(parsed, owned, statement, fact)? {
            return Err(bad());
        }
    }
    let listed = exit
        .actions()
        .iter()
        .filter_map(|action| match action {
            Action::Drop(fact) => Some(*fact),
            _ => None,
        })
        .collect::<Vec<_>>();
    let actual = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.point() == exit.point())
        .collect::<Vec<_>>();
    if actual.len() != listed.len()
        || actual
            .iter()
            .any(|fact| listed.iter().filter(|listed| *listed == fact).count() != 1)
    {
        return Err(bad());
    }
    Ok(())
}
