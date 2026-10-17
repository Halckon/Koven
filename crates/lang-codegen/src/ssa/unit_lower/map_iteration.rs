//! Narrow Map capability gate. This runs after the sealed frontend view, before Module creation.
use super::*;
use lang_frontend::{
    ownership_checking::{
        IterationSource, MapIterationComponent, OwnershipBindingKind, UnitDropFact, UnitDropTarget,
        UnitIterationCleanupAction as Action, UnitIterationExitKind, UnitIterationSourceAccess,
        UnitLoanTarget, UnitMapIterationOwnershipPlan,
    },
    parser::{ParsedFile, StringPart},
    type_checking::{
        CallableResultSource, IntrinsicTypeConstructor, MapIterationKind, UnitCallTarget,
        UnitMapIterationDescriptor,
    },
};

#[derive(Clone, Copy)]
pub(super) struct QualifiedMap {
    pub(super) statement: UnitStatementId,
    pub(super) root: UnitSymbolId,
    pub(super) parameter_slot: usize,
    pub(super) source_drop: Option<UnitDropFact>,
    pub(super) transfer_drop: Option<UnitDropFact>,
    pub(super) body_exit: UnitIterationExitKind,
    pub(super) body_point: UnitDropPoint,
}

pub(super) fn validate(
    parsed: &[&ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), LoweringError> {
    let mut descriptors = typed.map_iterations().iter().collect::<Vec<_>>();
    descriptors.sort_by_key(|descriptor| descriptor.statement());
    for descriptor in descriptors {
        let parsed = parsed
            .get(descriptor.statement().source_unit().index())
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        qualify(parsed, names, typed, owned, descriptor)?;
    }
    Ok(())
}

/// Only public UnitMapPlan/Drop facts participate. There is no unit closure-phi table.
pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
) -> Result<QualifiedMap, LoweringError> {
    let statement = descriptor.statement();
    let node = parsed
        .ast()
        .statements()
        .get(statement.statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let span = node.span();
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let Statement::For { source, body, .. } = node.payload() else {
        return Err(missing());
    };
    if descriptor.source() != UnitExpressionId::new(statement.source_unit(), *source)
        || typed.map_iteration(statement) != Some(descriptor)
        || typed.expression_type(descriptor.source()) != Some(descriptor.source_type())
    {
        return Err(missing());
    }
    // Validated source-return/dead-tail rows legitimately have no AcquireProvider plan.
    let plan = owned.map_iteration(statement).ok_or_else(unsupported)?;
    if plan.descriptor() != descriptor {
        return Err(missing());
    }
    let constructor = match descriptor.kind() {
        MapIterationKind::Map => IntrinsicTypeConstructor::Map,
        MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
    };
    if !matches!(typed.types().get(descriptor.source_type()),
        Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
            if *actual == constructor && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()])
    {
        return Err(missing());
    }
    let source_node = parsed
        .ast()
        .expressions()
        .get(*source)
        .map_err(|_| missing())?;
    if !matches!(source_node.payload(), Expression::Name) {
        return Err(unsupported());
    }
    let IterationSource::BorrowedSource(UnitLoanTarget::Place(place)) = plan.source_kind() else {
        return Err(unsupported());
    };
    if !place.is_root() {
        return Err(unsupported());
    }
    let root = place.root();
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == statement.source_unit()
                && reference.namespace() == Some(Namespace::Value)
                && reference.span() == source_node.span()
        })
        .collect::<Vec<_>>();
    if !matches!(references.as_slice(), [reference]
        if reference.target() == &UnitReferenceTarget::Symbol(root))
        || root.source_unit() != statement.source_unit()
        || typed.symbol_type(root) != Some(descriptor.source_type())
    {
        return Err(missing());
    }
    let symbol = names
        .names()
        .source_units()
        .get(root.source_unit().index())
        .and_then(|source| source.resolution().symbols().get(root.symbol().index()))
        .ok_or_else(missing)?;
    if symbol.kind() != SymbolKind::ValueParameter {
        return Err(unsupported());
    }
    let callables = typed
        .signatures()
        .declarations()
        .iter()
        .flat_map(|declaration| {
            declaration.callable().into_iter().chain(
                declaration.nominal().into_iter().flat_map(|nominal| {
                    nominal.members().iter().chain(nominal.companion_members())
                }),
            )
        });
    let parameters = callables
        .flat_map(|callable| {
            callable
                .parameters()
                .iter()
                .enumerate()
                .filter_map(move |(index, parameter)| {
                    (parameter.symbol() == Some(root)).then_some((
                        parameter,
                        index + usize::from(callable.receiver().is_some()),
                        callable.return_type(),
                    ))
                })
        })
        .collect::<Vec<_>>();
    let [(parameter, parameter_slot, return_type)] = parameters.as_slice() else {
        return Err(missing());
    };
    if parameter.ty() != descriptor.source_type() {
        return Err(missing());
    }
    let binding = owned.binding_kind(root).ok_or_else(missing)?;
    let (expected_binding, expected_access) = match parameter.mode() {
        ParameterMode::Value => (
            OwnershipBindingKind::Owned,
            UnitIterationSourceAccess::Owned,
        ),
        ParameterMode::Borrow => (
            OwnershipBindingKind::Shared,
            UnitIterationSourceAccess::Shared,
        ),
        ParameterMode::Inout => (
            OwnershipBindingKind::Exclusive,
            UnitIterationSourceAccess::Exclusive,
        ),
    };
    if binding != expected_binding || plan.source_access() != expected_access {
        return Err(missing());
    }
    // Check whole supported rows, never a cross-product of independently accepted fields.
    let supported = matches!(
        (
            descriptor.kind(),
            parameter.mode(),
            binding,
            plan.source_access()
        ),
        (
            MapIterationKind::Map,
            ParameterMode::Value,
            OwnershipBindingKind::Owned,
            UnitIterationSourceAccess::Owned
        ) | (
            MapIterationKind::MutableMap,
            ParameterMode::Borrow,
            OwnershipBindingKind::Shared,
            UnitIterationSourceAccess::Shared
        )
    );
    if !supported {
        return Err(unsupported());
    }
    let bindings = descriptor.bindings();
    if bindings[0] == bindings[1]
        || bindings.contains(&root)
        || bindings
            .iter()
            .any(|symbol| symbol.source_unit() != statement.source_unit())
        || typed.symbol_type(bindings[0]) != Some(descriptor.key_type())
        || typed.symbol_type(bindings[1]) != Some(descriptor.value_type())
    {
        return Err(missing());
    }
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = plan.bindings()[index];
        let loan = plan.loans().get(index).ok_or_else(missing)?;
        if binding.symbol() != bindings[index]
            || binding.kind() != OwnershipBindingKind::Shared
            || loan.component() != component
            || loan.binding() != bindings[index]
            || loan.statement() != statement
            || loan.source() != descriptor.source()
            || Some(loan.origin()) != plan.source()
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
        {
            return Err(missing());
        }
    }
    reject_nested(parsed, statement.statement(), span)?;
    let body_exit = qualify_body(parsed, names, typed, descriptor, *body, span)?;
    if matches!(body_exit, UnitIterationExitKind::Return(_))
        && builtin_type(typed, *return_type) != Some(BuiltinType::Int)
    {
        return Err(unsupported());
    }
    let body_span = parsed
        .ast()
        .statements()
        .get(*body)
        .map_err(|_| missing())?
        .span();
    for (&expression, &ty) in typed.expression_types() {
        if expression.source_unit() != statement.source_unit() {
            continue;
        }
        let node = parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| missing())?;
        if contains(body_span, node.span())
            && matches!(node.payload(), Expression::Call { .. })
            && builtin_type(typed, ty) == Some(BuiltinType::Nothing)
        {
            return Err(unsupported());
        }
    }
    let (source_drop, transfer_drop) =
        validate_cleanup(plan, owned, *body, body_exit, root, symbol.span(), span)?;
    let body_point = body_point(statement, *body, body_exit);
    Ok(QualifiedMap {
        statement,
        root,
        parameter_slot: *parameter_slot,
        source_drop,
        transfer_drop,
        body_exit,
        body_point,
    })
}

fn body_point(
    statement: UnitStatementId,
    body: StatementId,
    kind: UnitIterationExitKind,
) -> UnitDropPoint {
    match kind {
        UnitIterationExitKind::Continue(expression)
        | UnitIterationExitKind::Break(expression)
        | UnitIterationExitKind::Return(expression) => UnitDropPoint::ControlTransfer(expression),
        _ => UnitDropPoint::AfterStatement(UnitStatementId::new(statement.source_unit(), body)),
    }
}

fn validate_cleanup(
    plan: &UnitMapIterationOwnershipPlan,
    owned: &CompilationUnitOwnership,
    body: StatementId,
    body_exit: UnitIterationExitKind,
    root: UnitSymbolId,
    origin: Span,
    span: Span,
) -> Result<(Option<UnitDropFact>, Option<UnitDropFact>), LoweringError> {
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    let [body_plan, exhaustion] = plan.exits() else {
        return Err(missing());
    };
    if body_plan.kind() != body_exit
        || body_plan.point() != body_point(statement, body, body_exit)
        || exhaustion.kind() != UnitIterationExitKind::Exhaustion
        || exhaustion.point() != UnitDropPoint::LoopExit(statement)
        || owned.iteration_cleanup_at(body_plan.point()) != Some(body_plan.actions())
        || owned.iteration_cleanup_at(exhaustion.point()) != Some(exhaustion.actions())
    {
        return Err(missing());
    }
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let terminal_body = matches!(
        body_exit,
        UnitIterationExitKind::Break(_) | UnitIterationExitKind::Return(_)
    );
    let (source_drop, transfer_drop) = match plan.source_access() {
        UnitIterationSourceAccess::Owned => {
            if drops.iter().any(|drop| drop.value_origin() != origin) {
                return Err(missing());
            }
            // First-red unit facts publish one LoopExit drop for Continue,
            // and one independent drop per terminal path for Break/Return.
            // Match actual points, not global table order or fixture identities.
            let expected_count = 1 + usize::from(terminal_body);
            if drops.len() != expected_count
                || drops.iter().any(|drop| {
                    drop.point() != exhaustion.point()
                        && (!terminal_body || drop.point() != body_plan.point())
                })
            {
                // A validated source kept alive beyond either edge requires its
                // own lifetime proof; presence alone cannot authorize a Drop.
                return Err(unsupported());
            }
            let at = |point| -> Result<UnitDropFact, LoweringError> {
                let mut matches = drops.iter().copied().filter(|drop| drop.point() == point);
                let drop = matches.next().ok_or_else(missing)?;
                if matches.next().is_some() {
                    return Err(missing());
                }
                Ok(drop)
            };
            (
                Some(at(exhaustion.point())?),
                terminal_body.then(|| at(body_plan.point())).transpose()?,
            )
        }
        UnitIterationSourceAccess::Shared if drops.is_empty() => (None, None),
        UnitIterationSourceAccess::Shared => return Err(missing()),
        _ => return Err(unsupported()),
    };
    let terminal = [
        Action::FinishProvider(statement),
        Action::EndSource(statement),
    ];
    let mut exhaustion_actions = terminal.to_vec();
    if let Some(drop) = source_drop {
        exhaustion_actions.push(Action::Drop(drop));
    }
    if exhaustion.actions() != exhaustion_actions {
        return Err(missing());
    }
    let mut body_suffix = vec![
        Action::EndBinding {
            statement,
            symbol: descriptor.value_binding(),
        },
        Action::EndBinding {
            statement,
            symbol: descriptor.key_binding(),
        },
    ];
    if terminal_body {
        body_suffix.extend(terminal);
        if let Some(drop) = transfer_drop {
            body_suffix.push(Action::Drop(drop));
        }
    }
    if !body_plan.actions().ends_with(&body_suffix) {
        return Err(missing());
    }
    let prefix = &body_plan.actions()[..body_plan.actions().len() - body_suffix.len()];
    // Ordinary local owner drops are the only supported prefix; other provider/call
    // actions need a separate capability proof, not reordered cleanup here.
    if prefix.iter().any(|action| {
        !matches!(action, Action::Drop(fact) if fact.target() != UnitDropTarget::Named(root))
    }) { return Err(unsupported()); }
    Ok((source_drop, transfer_drop))
}

fn qualify_body(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    descriptor: &UnitMapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<UnitIterationExitKind, LoweringError> {
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed.ast().statements().get(body).map_err(|_| missing())?;
    let Statement::Block { elements } = node.payload() else {
        straight_statement(parsed, body, span)?;
        return Ok(UnitIterationExitKind::Fallthrough);
    };
    let Some((&last, prefix)) = elements.split_last() else {
        return Ok(UnitIterationExitKind::Fallthrough);
    };
    let last = parsed.ast().statements().get(last).map_err(|_| missing())?;
    let Statement::Expression { expression } = last.payload() else {
        straight_statement(parsed, body, span)?;
        return Ok(UnitIterationExitKind::Fallthrough);
    };
    let node = parsed
        .ast()
        .expressions()
        .get(*expression)
        .map_err(|_| missing())?;
    let expression_id = UnitExpressionId::new(descriptor.statement().source_unit(), *expression);
    let kind = match node.payload() {
        Expression::Continue { .. } => UnitIterationExitKind::Continue(expression_id),
        Expression::Break { .. } => UnitIterationExitKind::Break(expression_id),
        Expression::Return {
            value: Some(value), ..
        } => {
            qualify_return_operand(parsed, names, typed, descriptor, *value, span)?;
            UnitIterationExitKind::Return(expression_id)
        }
        _ => {
            straight_statement(parsed, body, span)?;
            return Ok(UnitIterationExitKind::Fallthrough);
        }
    };
    if builtin_type(typed, descriptor.key_type()) != Some(BuiltinType::String)
        || builtin_type(typed, descriptor.value_type()) != Some(BuiltinType::String)
    {
        return Err(unsupported());
    }
    for statement in prefix {
        straight_statement(parsed, *statement, span)?;
    }
    Ok(kind)
}

fn qualify_return_operand(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    descriptor: &UnitMapIterationDescriptor,
    operand: ExpressionId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .expressions()
        .get(operand)
        .map_err(|_| missing())?;
    let Expression::Call {
        callee, arguments, ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    if !matches!(
        parsed
            .ast()
            .expressions()
            .get(*callee)
            .map_err(|_| missing())?
            .payload(),
        Expression::Name
    ) || arguments.len() != 2
    {
        return Err(unsupported());
    }
    let call = typed
        .call(UnitExpressionId::new(
            descriptor.statement().source_unit(),
            operand,
        ))
        .ok_or_else(missing)?;
    let UnitCallTarget::Declaration(declaration) = call.target() else {
        return Err(unsupported());
    };
    let signature = typed
        .signatures()
        .declaration(declaration)
        .and_then(|item| item.callable())
        .ok_or_else(missing)?;
    if call.receiver().is_some()
        || call.aborts()
        || call.prints_line()
        || call.result_source() != CallableResultSource::Owned
        || builtin_type(typed, call.return_type()) != Some(BuiltinType::Int)
        || signature.receiver().is_some()
        || !signature.has_body()
        || !signature.type_parameters().is_empty()
        || !call.instance().type_arguments().is_empty()
    {
        return Err(unsupported());
    }
    if signature.target() != UnitCallableTarget::Declaration(declaration)
        || signature.return_type() != call.return_type()
        || signature.parameters().len() != 2
        || call.arguments().len() != 2
    {
        return Err(missing());
    }
    for (index, (argument, symbol)) in arguments.iter().zip(descriptor.bindings()).enumerate() {
        let argument_node = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .map_err(|_| missing())?;
        if !matches!(argument_node.payload(), Expression::Name) {
            return Err(unsupported());
        }
        let references = names
            .names()
            .references()
            .iter()
            .filter(|reference| {
                reference.source_unit() == descriptor.statement().source_unit()
                    && reference.namespace() == Some(Namespace::Value)
                    && reference.span() == argument_node.span()
            })
            .collect::<Vec<_>>();
        let [reference] = references.as_slice() else {
            return Err(missing());
        };
        if reference.target() != &UnitReferenceTarget::Symbol(symbol) {
            return Err(unsupported());
        }
        let parameter = &signature.parameters()[index];
        let mapping = call.arguments()[index];
        if parameter.mode() != ParameterMode::Borrow
            || mapping.mode() != ParameterMode::Borrow
            || mapping.argument_index() != index
            || mapping.parameter_index() != index
            || mapping.crosses_thread()
        {
            return Err(unsupported());
        }
        if builtin_type(typed, parameter.ty()) != Some(BuiltinType::String)
            || mapping.parameter_type() != parameter.ty()
            || typed.expression_type(UnitExpressionId::new(
                descriptor.statement().source_unit(),
                argument.value,
            )) != Some(parameter.ty())
        {
            return Err(missing());
        }
    }
    Ok(())
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn reject_nested(
    parsed: &ParsedFile,
    statement: StatementId,
    span: Span,
) -> Result<(), LoweringError> {
    for (id, node) in parsed.ast().statements().iter() {
        if id != statement
            && contains(node.span(), span)
            && matches!(
                node.payload(),
                Statement::For { .. } | Statement::While { .. } | Statement::Loop { .. }
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
    }
    for (_, node) in parsed.ast().expressions().iter() {
        if contains(node.span(), span)
            && matches!(
                node.payload(),
                Expression::If { .. } | Expression::When { .. } | Expression::Lambda { .. }
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
    }
    Ok(())
}

fn straight_statement(
    parsed: &ParsedFile,
    statement: StatementId,
    span: Span,
) -> Result<(), LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
    match node.payload() {
        Statement::Block { elements } => {
            for child in elements {
                straight_statement(parsed, *child, span)?;
            }
            Ok(())
        }
        Statement::Expression { expression } => straight_expression(parsed, *expression, span),
        Statement::LocalVariable { declaration } => {
            let (_, item, _) = unwrap_modified(parsed, *declaration)?;
            let Item::Variable { initializer, .. } = item else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            straight_expression(parsed, initializer, span)
        }
        _ => Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
    }
}

fn straight_expression(
    parsed: &ParsedFile,
    expression: ExpressionId,
    span: Span,
) -> Result<(), LoweringError> {
    let node = parsed
        .ast()
        .expressions()
        .get(expression)
        .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
    match node.payload() {
        Expression::Name | Expression::Literal(_) => Ok(()),
        Expression::String { parts }
            if parts.iter().all(|part| matches!(part, StringPart::Text(_))) =>
        {
            Ok(())
        }
        Expression::Call {
            callee, arguments, ..
        } => {
            let callee = parsed
                .ast()
                .expressions()
                .get(*callee)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(callee.payload(), Expression::Name) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            for argument in arguments {
                straight_expression(parsed, argument.value, span)?;
            }
            Ok(())
        }
        _ => Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
    }
}
