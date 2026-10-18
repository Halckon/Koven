//! Unit temporary qualification uses only source-qualified descriptor, plan and Drop facts.
use super::*;

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
    let source = descriptor.source();
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
    if !matches!(callee.payload(), Expression::Name) || !arguments.is_empty() {
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
        || !signature.parameters().is_empty()
        || !signature.type_parameters().is_empty()
        || !signature.has_body()
        || signature.result_source() != CallableResultSource::Owned
        || builtin_type(typed, descriptor.key_type()) != Some(BuiltinType::String)
        || builtin_type(typed, descriptor.value_type()) != Some(BuiltinType::String)
    {
        return Err(unsupported());
    }
    if call.expression() != source
        || call.return_type() != descriptor.source_type()
        || !call.arguments().is_empty()
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
            || loan.source() != source
            || loan.component() != component
            || loan.binding() != bindings[index]
            || loan.origin() != &UnitLoanTarget::Temporary(source)
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
        {
            return Err(missing());
        }
    }
    reject_nested(parsed, statement.statement(), span)?;
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
        transfer_drop: None,
        body_exit: UnitIterationExitKind::Fallthrough,
        body_point,
        conditional: None,
    })
}
