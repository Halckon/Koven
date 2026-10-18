//! Unit temporary qualification uses only source-qualified descriptor, plan and Drop facts.
mod cleanup;
use super::*;
use lang_frontend::{ownership_checking::UnitClosureCaptureSource, parser::ParameterModeMarker};

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<QualifiedMap, LoweringError> {
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let statement = descriptor.statement();
    let source = UnitExpressionId::new(
        statement.source_unit(),
        crate::ssa::map_source::transparent_expression(
            parsed,
            descriptor.source().expression(),
            |id| {
                let id = UnitExpressionId::new(statement.source_unit(), id);
                typed.expression_type(id).zip(typed.expression_category(id))
            },
        )?,
    );
    let source_node = parsed
        .ast()
        .expressions()
        .get(source.expression())
        .map_err(|_| missing())?;
    let Expression::Call {
        callee, arguments, ..
    } = source_node.payload()
    else {
        return Err(unsupported());
    };
    let callee = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|_| missing())?;
    if !matches!(callee.payload(), Expression::Name) {
        return Err(unsupported());
    }
    let call = typed.call(source).ok_or_else(missing)?;
    let UnitCallTarget::Declaration(factory) = call.target() else {
        return Err(unsupported());
    };
    let signature = typed
        .signatures()
        .declaration(factory)
        .and_then(|item| item.callable())
        .ok_or_else(missing)?;
    if signature.target() != UnitCallableTarget::Declaration(factory)
        || signature.return_type() != call.return_type()
        || signature.result_source() != call.result_source()
    {
        return Err(missing());
    }
    if call.receiver().is_some()
        || call.aborts()
        || call.prints_line()
        || call.result_source() != CallableResultSource::Owned
        || !call.instance().type_arguments().is_empty()
        || signature.receiver().is_some()
        || !signature.type_parameters().is_empty()
        || !signature.has_body()
        || signature.result_source() != CallableResultSource::Owned
    {
        return Err(unsupported());
    }
    // The shared storage planner owns represented payload admission; this proof
    // consumes the Unit Call/source/Drop identities independently of the payload shape.
    if call.expression() != source
        || call.return_type() != descriptor.source_type()
        || call.arguments().len() != arguments.len()
        || call.arguments().len() != signature.parameters().len()
        || typed.expression_category(source) != Some(ExpressionCategory::Temporary)
        || typed.copyability(descriptor.source_type()) != Copyability::MoveOnly
    {
        return Err(missing());
    }
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == source.source_unit()
                && reference.namespace() == Some(Namespace::Value)
                && reference.span() == callee.span()
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        return Err(missing());
    };
    let matches_factory = match reference.target() {
        UnitReferenceTarget::Declaration(target) => *target == factory,
        UnitReferenceTarget::OverloadSet(targets) => targets.contains(&factory),
        _ => false,
    };
    if !matches_factory {
        return Err(missing());
    }
    // Unlike named parameters, a factory result has no parameter-origin or unit phi table.
    let plan = owned.map_iteration(statement).ok_or_else(missing)?;
    if plan.descriptor() != descriptor
        || plan.source_access() != UnitIterationSourceAccess::Temporary
        || plan.source_kind() != &IterationSource::BorrowedSource(UnitLoanTarget::Temporary(source))
        || plan.source() != Some(&UnitLoanTarget::Temporary(source))
    {
        return Err(missing());
    }
    let bindings = descriptor.bindings();
    if bindings[0] == bindings[1]
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
        let loan = &plan.loans()[index];
        if binding.symbol() != bindings[index]
            || binding.kind() != OwnershipBindingKind::Shared
            || owned.binding_kind(bindings[index]) != Some(OwnershipBindingKind::Shared)
            || loan.statement() != statement
            || loan.source() != descriptor.source()
            || loan.component() != component
            || loan.binding() != bindings[index]
            || loan.origin() != &UnitLoanTarget::Temporary(source)
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
        {
            return Err(missing());
        }
    }
    reject_nested(parsed, statement.statement(), span)?;
    if let Some(qualified) = qualify_transfer(parsed, names, typed, owned, descriptor, body, span)?
    {
        return Ok(qualified);
    }
    straight_statement(parsed, body, span)?;
    let body_span = parsed
        .ast()
        .statements()
        .get(body)
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
    if plan.exits().iter().any(|exit| {
        !matches!(
            exit.kind(),
            UnitIterationExitKind::Fallthrough | UnitIterationExitKind::Exhaustion
        )
    }) {
        return Err(unsupported());
    }
    let [body_plan, exhaustion] = plan.exits() else {
        return Err(missing());
    };
    let body_point =
        UnitDropPoint::AfterStatement(UnitStatementId::new(statement.source_unit(), body));
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == UnitDropTarget::Temporary(source))
        .collect::<Vec<_>>();
    let [source_drop] = drops.as_slice() else {
        return Err(missing());
    };
    if source_drop.point() != UnitDropPoint::LoopExit(statement)
        || source_drop.value_origin() != source_node.span()
        || body_plan.kind() != UnitIterationExitKind::Fallthrough
        || body_plan.point() != body_point
        || exhaustion.kind() != UnitIterationExitKind::Exhaustion
        || exhaustion.point() != source_drop.point()
        || owned.iteration_cleanup_at(body_point) != Some(body_plan.actions())
        || owned.iteration_cleanup_at(exhaustion.point()) != Some(exhaustion.actions())
        || exhaustion.actions()
            != [
                Action::FinishProvider(statement),
                Action::EndSource(statement),
                Action::Drop(*source_drop),
            ]
    {
        return Err(missing());
    }
    let body_suffix = [
        Action::EndBinding {
            statement,
            symbol: descriptor.value_binding(),
        },
        Action::EndBinding {
            statement,
            symbol: descriptor.key_binding(),
        },
    ];
    if !body_plan.actions().ends_with(&body_suffix) {
        return Err(missing());
    }
    let prefix = &body_plan.actions()[..body_plan.actions().len() - body_suffix.len()];
    if prefix.iter().any(|action| {
        !matches!(action, Action::Drop(fact)
            if fact.target() != UnitDropTarget::Temporary(source)
                && fact.point() == body_point
                && owned.drops().contains(fact))
    }) {
        return Err(missing());
    }
    if !prefix.is_empty() {
        // These are valid owner lifetimes, but this slice proves only the two component ends.
        return Err(unsupported());
    }
    Ok(QualifiedMap {
        statement,
        source: SourceIdentity::Temporary(source),
        source_drop: Some(*source_drop),
        late_drop: None,
        transfer_drop: None,
        body_exit: UnitIterationExitKind::Fallthrough,
        body_point,
        conditional: None,
        nested: None,
    })
}

/// The normal path above keeps its original no-local-drop boundary.
fn qualify_transfer(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<Option<QualifiedMap>, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed.ast().statements().get(body).map_err(|_| bad())?;
    let body_span = node.span();
    let Statement::Block { elements } = node.payload() else {
        return Ok(None);
    };
    let Some((&last, prefix)) = elements.split_last() else {
        return Ok(None);
    };
    let last = parsed.ast().statements().get(last).map_err(|_| bad())?;
    let Statement::Expression { expression } = last.payload() else {
        return Ok(None);
    };
    let tail = parsed
        .ast()
        .expressions()
        .get(*expression)
        .map_err(|_| bad())?;
    let operand = match tail.payload() {
        Expression::Continue { .. } | Expression::Break { .. } => None,
        Expression::Return {
            value: Some(value), ..
        } => Some(*value),
        _ => return Ok(None),
    };
    let (&local, ordinary) = prefix.split_first().ok_or_else(unsupported)?;
    let statement = descriptor.statement();
    let guard = nested::qualify_guard(
        parsed,
        names,
        typed,
        owned,
        statement.source_unit(),
        local,
        span,
    )?;
    if descriptor.bindings().contains(&guard.symbol)
        || guard.initializer == descriptor.source()
        || typed.expression_category(guard.initializer) != Some(ExpressionCategory::Temporary)
    {
        return Err(bad());
    }
    for child in ordinary {
        let node = parsed.ast().statements().get(*child).map_err(|_| bad())?;
        let Statement::Expression { expression } = node.payload() else {
            return Err(unsupported());
        };
        straight_expression(parsed, *expression, span)?;
    }
    // Constructor identity alone does not prove an unused, uncaptured local owner.
    if names
        .names()
        .references()
        .iter()
        .any(|reference| reference.target() == &UnitReferenceTarget::Symbol(guard.symbol))
        || owned
            .captures()
            .iter()
            .any(|capture| capture.source() == UnitClosureCaptureSource::Symbol(guard.symbol))
    {
        return Err(unsupported());
    }
    // Inspect only the body: the source factory itself is another Resource Call.
    for (id, node) in parsed.ast().expressions().iter() {
        if !contains(body_span, node.span()) || !matches!(node.payload(), Expression::Call { .. }) {
            continue;
        }
        let id = UnitExpressionId::new(statement.source_unit(), id);
        let ty = typed.expression_type(id).ok_or_else(bad)?;
        if builtin_type(typed, ty) == Some(BuiltinType::Nothing) {
            return Err(unsupported());
        }
        if id != guard.initializer {
            match typed.is_resource_type(ty) {
                Some(true) => return Err(unsupported()),
                Some(false) => {}
                None => return Err(bad()),
            }
        }
    }
    if let Some(operand) = operand {
        qualify_return_identity(parsed, names, typed, owned, descriptor, operand, span)?;
    }
    let body_exit = qualify_body(parsed, names, typed, descriptor, body, span)?;
    let body_point = body_point(statement, body, body_exit);
    let plan = owned.map_iteration(statement).ok_or_else(bad)?;
    let IterationSource::BorrowedSource(UnitLoanTarget::Temporary(source)) = plan.source_kind()
    else {
        return Err(bad());
    };
    let source = *source;
    let source_origin = parsed
        .ast()
        .expressions()
        .get(source.expression())
        .map_err(|_| bad())?
        .span();
    let (source_drop, transfer_drop) = cleanup::qualify(
        owned,
        plan,
        body_point,
        body_exit,
        guard,
        source_origin,
        span,
    )?;
    Ok(Some(QualifiedMap {
        statement,
        source: SourceIdentity::Temporary(source),
        source_drop: Some(source_drop),
        late_drop: None,
        transfer_drop,
        body_exit,
        body_point,
        conditional: None,
        nested: None,
    }))
}

/// This additional identity gate applies only to the new temporary Return slice.
fn qualify_return_identity(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    operand: ExpressionId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let source = descriptor.statement().source_unit();
    let expression = UnitExpressionId::new(source, operand);
    let node = parsed.ast().expressions().get(operand).map_err(|_| bad())?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    let callee = parsed.ast().expressions().get(*callee).map_err(|_| bad())?;
    if callee.payload() != &Expression::Name
        || arguments.len() != 2
        || !type_arguments.is_empty()
        || arguments
            .iter()
            .any(|argument| argument.named_prefix.is_some() || argument.mode_marker.is_some())
    {
        return Err(unsupported());
    }
    let call = typed.call(expression).ok_or_else(bad)?;
    let UnitCallTarget::Declaration(target) = call.target() else {
        return Err(unsupported());
    };
    let declaration = typed.signatures().declaration(target).ok_or_else(bad)?;
    let signature = declaration.callable().ok_or_else(bad)?;
    if call.receiver().is_some()
        || call.aborts()
        || call.prints_line()
        || call.result_source() != CallableResultSource::Owned
        || !call.instance().type_arguments().is_empty()
        || signature.receiver().is_some()
        || signature.extension_receiver_symbol().is_some()
        || !signature.has_body()
        || !signature.type_parameters().is_empty()
        || builtin_type(typed, call.return_type()) != Some(BuiltinType::Int)
    {
        return Err(unsupported());
    }
    let declarations = names
        .names()
        .index()
        .declarations()
        .iter()
        .filter(|declaration| declaration.id() == target)
        .collect::<Vec<_>>();
    let [source_declaration] = declarations.as_slice() else {
        return Err(bad());
    };
    if source_declaration.source_unit() != source {
        return Err(unsupported());
    }
    let (_, item, _) = unwrap_modified(parsed, source_declaration.root())?;
    let Item::Function {
        name, parameters, ..
    } = item
    else {
        return Err(bad());
    };
    let target_symbol = names.names().declaration_symbol(target).ok_or_else(bad)?;
    let local = names
        .names()
        .source_units()
        .get(source.index())
        .ok_or_else(bad)?
        .resolution();
    let symbol = local
        .symbols()
        .get(target_symbol.symbol().index())
        .ok_or_else(bad)?;
    if declaration.declaration() != target
        || declaration.symbol() != target_symbol
        || target_symbol.source_unit() != source
        || symbol.kind() != SymbolKind::Function
        || source_declaration.kind() != SymbolKind::Function
        || source_declaration.namespace() != Namespace::Value
        || name != NameMarker::Present(symbol.span())
        || source_declaration.name_span() != symbol.span()
        || signature.name_span() != symbol.span()
    {
        return Err(bad());
    }
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == source
                && reference.namespace() == Some(Namespace::Value)
                && reference.span() == callee.span()
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        return Err(bad());
    };
    let selected = match reference.target() {
        UnitReferenceTarget::Declaration(actual) => *actual == target,
        UnitReferenceTarget::OverloadSet(targets) => targets.contains(&target),
        UnitReferenceTarget::Symbol(actual) => *actual == target_symbol,
        _ => false,
    };
    if !selected
        || call.expression() != expression
        || typed.expression_type(expression) != Some(call.return_type())
        || typed.expression_category(expression) != Some(ExpressionCategory::Temporary)
        || signature.target() != UnitCallableTarget::Declaration(target)
        || signature.return_type() != call.return_type()
        || signature.result_source() != call.result_source()
        || signature.parameters().len() != 2
        || parameters.len() != 2
        || call.arguments().len() != 2
    {
        return Err(bad());
    }
    for (index, argument) in arguments.iter().enumerate() {
        let node = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .map_err(|_| bad())?;
        if node.payload() != &Expression::Name
            || !matches!(
                parameters[index].mode_marker,
                None | Some(ParameterModeMarker::Borrow(_))
            )
        {
            return Err(unsupported());
        }
        let parameter = &signature.parameters()[index];
        if builtin_type(typed, parameter.ty()) != Some(BuiltinType::String) {
            return Err(unsupported());
        }
        let parameter_symbol = parameter.symbol().ok_or_else(bad)?;
        let local_symbol = local
            .symbols()
            .get(parameter_symbol.symbol().index())
            .ok_or_else(bad)?;
        let mapping = call.arguments()[index];
        let ty = [descriptor.key_type(), descriptor.value_type()][index];
        if parameter_symbol.source_unit() != source
            || local_symbol.kind() != SymbolKind::ValueParameter
            || parameters[index].name != NameMarker::Present(local_symbol.span())
            || parameter.span() != parameters[index].span
            || parameter.mode() != ParameterMode::Borrow
            || mapping.mode() != ParameterMode::Borrow
            || mapping.argument_index() != index
            || mapping.parameter_index() != index
            || mapping.crosses_thread()
            || mapping.category() != ExpressionCategory::Place
            || parameter.ty() != ty
            || mapping.parameter_type() != ty
            || typed.expression_type(UnitExpressionId::new(source, argument.value)) != Some(ty)
            || typed.expression_category(UnitExpressionId::new(source, argument.value))
                != Some(ExpressionCategory::Place)
            || typed.symbol_type(parameter_symbol) != Some(ty)
            || owned.binding_kind(parameter_symbol) != Some(OwnershipBindingKind::Shared)
        {
            return Err(bad());
        }
    }
    Ok(())
}
