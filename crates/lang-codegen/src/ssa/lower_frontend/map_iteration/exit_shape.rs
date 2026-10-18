//! Bound the Map body to normal straight-line execution or one direct tail transfer.
//! This describes capability only; the exit/action/owner facts still come from Phase 3.
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    parser::NameMarker,
    type_checking::{CallableResultSource, CallableTarget, MapIterationDescriptor},
};

pub(super) fn body_exit(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    descriptor: MapIterationDescriptor,
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
    if [descriptor.key_type(), descriptor.value_type()]
        .iter()
        .any(|ty| builtin_type(typed, *ty) != Some(BuiltinType::String))
    {
        return Err(unsupported());
    }
    let node = parsed
        .ast()
        .expressions()
        .get(*expression)
        .map_err(|_| missing())?;
    match node.payload() {
        Expression::Continue { .. } => Ok(IterationExitKind::Continue(*expression)),
        Expression::Break { .. } => Ok(IterationExitKind::Break(*expression)),
        Expression::Return {
            value: Some(value), ..
        } => {
            return_operand(parsed, names, typed, descriptor, *value, span)?;
            Ok(IterationExitKind::Return(*expression))
        }
        _ => Err(unsupported()),
    }
}

pub(super) fn return_operand(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    descriptor: MapIterationDescriptor,
    value: ExpressionId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .expressions()
        .get(value)
        .map_err(|_| missing())?;
    let Expression::Call {
        callee, arguments, ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    let callee = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|_| missing())?;
    if callee.payload() != &Expression::Name || arguments.len() != 2 {
        return Err(unsupported());
    }
    let call = typed.call(value).ok_or_else(missing)?;
    let CallableTarget::Source(target) = call.target() else {
        return Err(unsupported());
    };
    let symbol = names.symbols().get(target.index()).ok_or_else(missing)?;
    let signature = unique(
        typed
            .callables()
            .iter()
            .filter(|signature| signature.symbol() == target),
    )
    .ok_or_else(missing)?;
    if symbol.kind() != SymbolKind::Function
        || !parsed.roots().iter().any(|root| {
            parsed.ast().items().get(*root).is_ok_and(|node| {
                matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                    if *name == symbol.span())
            })
        })
        || call.receiver().is_some()
        || call.aborts()
        || call.result_source() != CallableResultSource::Owned
        || !call.instance().type_arguments().is_empty()
        || signature.owner().is_some()
        || signature.receiver().is_some()
        || !signature.type_parameters().is_empty()
        || builtin_type(typed, call.return_type()) != Some(BuiltinType::Int)
    {
        return Err(unsupported());
    }
    if call.expression() != value
        || typed.expression_type(value) != Some(call.return_type())
        || call.arguments().len() != 2
        || signature.return_type() != call.return_type()
        || signature.parameters().len() != 2
    {
        return Err(missing());
    }
    for (index, argument) in arguments.iter().enumerate() {
        let node = parsed
            .ast()
            .expressions()
            .get(argument.value)
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
        if reference.target() != &ReferenceTarget::Symbol(descriptor.bindings()[index]) {
            return Err(unsupported());
        }
        let actual = call.arguments()[index];
        if actual.argument_index() != index
            || actual.parameter_index() != index
            || actual.mode() != ParameterMode::Borrow
            || actual.crosses_thread()
            || signature.parameters()[index].mode != ParameterMode::Borrow
            || signature.parameters()[index].ty != actual.parameter_type()
            || actual.parameter_type() != [descriptor.key_type(), descriptor.value_type()][index]
            || typed.expression_type(argument.value) != Some(actual.parameter_type())
        {
            return Err(missing());
        }
    }
    Ok(())
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
