//! A temporary provider and its sole Guard have two complete, disjoint exit sequences.
use super::*;

pub(super) fn qualify(
    owned: &CompilationUnitOwnership,
    plan: &UnitMapIterationOwnershipPlan,
    body_point: UnitDropPoint,
    body_exit: UnitIterationExitKind,
    guard: QualifiedGuard,
    source_origin: Span,
    span: Span,
) -> Result<(UnitDropFact, Option<UnitDropFact>), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    let terminal = match body_exit {
        UnitIterationExitKind::Continue(_) => false,
        UnitIterationExitKind::Break(_) | UnitIterationExitKind::Return(_) => true,
        _ => return Err(bad()),
    };
    let [body, exhaustion] = plan.exits() else {
        return Err(bad());
    };
    if body.kind() != body_exit
        || body.point() != body_point
        || exhaustion.kind() != UnitIterationExitKind::Exhaustion
        || exhaustion.point() != UnitDropPoint::LoopExit(statement)
        || owned.iteration_cleanup_at(body_point) != Some(body.actions())
        || owned.iteration_cleanup_at(exhaustion.point()) != Some(exhaustion.actions())
    {
        return Err(bad());
    }
    let sources = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == UnitDropTarget::Temporary(descriptor.source()))
        .collect::<Vec<_>>();
    if sources.len() != 1 + usize::from(terminal)
        || sources
            .iter()
            .any(|fact| fact.value_origin() != source_origin)
    {
        return Err(bad());
    }
    let source_at = |point| {
        let facts = sources
            .iter()
            .copied()
            .filter(|fact| fact.point() == point)
            .collect::<Vec<_>>();
        match facts.as_slice() {
            [fact] => Ok(*fact),
            _ => Err(bad()),
        }
    };
    let source_drop = source_at(exhaustion.point())?;
    let transfer_drop = terminal.then(|| source_at(body_point)).transpose()?;
    let guards = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == UnitDropTarget::Named(guard.symbol))
        .collect::<Vec<_>>();
    let [guard_drop] = guards.as_slice() else {
        return Err(bad());
    };
    if guard_drop.point() != body_point || guard_drop.value_origin() != guard.origin {
        return Err(bad());
    }
    let finish = [
        Action::FinishProvider(statement),
        Action::EndSource(statement),
    ];
    let mut exhaustion_actions = finish.to_vec();
    exhaustion_actions.push(Action::Drop(source_drop));
    let mut body_actions = vec![
        Action::Drop(*guard_drop),
        Action::EndBinding {
            statement,
            symbol: descriptor.value_binding(),
        },
        Action::EndBinding {
            statement,
            symbol: descriptor.key_binding(),
        },
    ];
    if let Some(drop) = transfer_drop {
        body_actions.extend(finish);
        body_actions.push(Action::Drop(drop));
    }
    if body.actions() != body_actions || exhaustion.actions() != exhaustion_actions {
        return Err(bad());
    }
    // The public Drop table must agree with every Drop in both whole-point sequences.
    for exit in [body, exhaustion] {
        let expected = exit
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
        if actual.len() != expected.len()
            || expected
                .iter()
                .any(|fact| actual.iter().filter(|actual| *actual == fact).count() != 1)
            || owned
                .conditional_receiver_drops()
                .iter()
                .any(|fact| fact.point() == exit.point())
        {
            return Err(bad());
        }
    }
    Ok((source_drop, transfer_drop))
}
