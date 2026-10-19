//! Bound the Map body to normal straight-line execution or one direct tail transfer.
//! This describes capability only; the exit/action/owner facts still come from Phase 3.
use super::*;
use lang_frontend::type_checking::MapIterationDescriptor;

pub(super) fn body_exit(
    parsed: &ParsedFile,
    _names: &NameResolution,
    typed: &TypedFile,
    _descriptor: MapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<IterationExitKind, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    if qualify::straight_statement(parsed, typed, body)? {
        return Ok(IterationExitKind::Fallthrough);
    }
    let node = parsed.ast().statements().get(body).map_err(|_| missing())?;
    let Statement::Block { elements } = node.payload() else {
        return Err(unsupported());
    };
    let Some((last, prefix)) = elements.split_last() else {
        return Err(unsupported());
    };
    for statement in prefix {
        if !qualify::straight_statement(parsed, typed, *statement)? {
            return Err(unsupported());
        }
    }
    let node = parsed
        .ast()
        .statements()
        .get(*last)
        .map_err(|_| missing())?;
    let Statement::Expression { expression } = node.payload() else {
        return Err(unsupported());
    };
    let node = parsed
        .ast()
        .expressions()
        .get(*expression)
        .map_err(|_| missing())?;

    match node.payload() {
        Expression::Continue { .. } => Ok(IterationExitKind::Continue(*expression)),
        Expression::Break { .. } => Ok(IterationExitKind::Break(*expression)),
        // Operand evaluation and concrete return ABI belong to ordinary lower_return.
        // Complete FE has already checked the return contract and ownership delivery.
        Expression::Return { .. } => Ok(IterationExitKind::Return(*expression)),
        _ => Err(unsupported()),
    }
}

pub(super) fn point(
    kind: IterationExitKind,
    statement: StatementId,
    body: StatementId,
) -> DropPoint {
    match kind {
        IterationExitKind::Fallthrough => DropPoint::AfterStatement(body),
        IterationExitKind::Continue(expression)
        | IterationExitKind::Break(expression)
        | IterationExitKind::Return(expression) => DropPoint::ControlTransfer(expression),
        IterationExitKind::Exhaustion => DropPoint::LoopExit(statement),
    }
}

pub(super) fn terminal(kind: IterationExitKind) -> bool {
    matches!(
        kind,
        IterationExitKind::Break(_) | IterationExitKind::Return(_) | IterationExitKind::Exhaustion
    )
}

pub(super) fn cleanup(
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    body: StatementId,
    kind: IterationExitKind,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    if plan.exits().len() != 2 {
        return Err(missing());
    }
    for expected_kind in [kind, IterationExitKind::Exhaustion] {
        let exit = unique(
            plan.exits()
                .iter()
                .filter(|exit| exit.kind() == expected_kind),
        )
        .ok_or_else(missing)?;
        if exit.point() != point(expected_kind, statement, body)
            || owned.cleanup_conditions().get(exit.condition()) != Some(&CleanupCondition::Always)
            || owned
                .cleanup_steps()
                .iter()
                .filter_map(|(point, action)| (*point == exit.point()).then_some(*action))
                .collect::<Vec<_>>()
                != exit.actions()
        {
            return Err(missing());
        }
        let map_actions = exit
            .actions()
            .iter()
            .filter(|action| !matches!(action, IterationCleanupAction::Drop(_)))
            .copied()
            .collect::<Vec<_>>();
        let mut expected = Vec::new();
        if expected_kind != IterationExitKind::Exhaustion {
            expected.extend([
                IterationCleanupAction::EndBinding {
                    statement,
                    symbol: descriptor.value_binding(),
                },
                IterationCleanupAction::EndBinding {
                    statement,
                    symbol: descriptor.key_binding(),
                },
            ]);
        }
        if terminal(expected_kind) {
            expected.extend([
                IterationCleanupAction::FinishProvider(statement),
                IterationCleanupAction::EndSource(statement),
            ]);
        }
        if map_actions != expected {
            return Err(missing());
        }
        // Local drops form one prefix. The Map sequence is contiguous, and
        // only the source owner may follow EndSource in this bounded shape.
        let start = exit
            .actions()
            .iter()
            .position(|action| expected.first() == Some(action))
            .ok_or_else(missing)?;
        if exit.actions()[..start]
            .iter()
            .any(|action| !matches!(action, IterationCleanupAction::Drop(_)))
            || exit.actions().get(start..start + expected.len()) != Some(expected.as_slice())
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let tail = &exit.actions()[start + expected.len()..];
        if !terminal(expected_kind) && !tail.is_empty() {
            return Err(missing());
        }
        if plan.source_access() == MapIterationSourceAccess::Shared && !tail.is_empty() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
    }
    Ok(())
}
