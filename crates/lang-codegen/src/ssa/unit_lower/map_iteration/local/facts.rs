//! Exact frontend call/loan facts for the bounded local normal row.
use super::*;
use lang_frontend::ownership_checking::{LoanKind, UnitCallArgumentOwnershipKind};

pub(super) fn factory(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    initializer: UnitExpressionId,
    ty: UnitTypeId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .expressions()
        .get(initializer.expression())
        .map_err(|_| bad())?;
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
    if callee.payload() != &Expression::Name || !arguments.is_empty() || !type_arguments.is_empty()
    {
        return Err(unsupported());
    }
    let call = typed.call(initializer).ok_or_else(bad)?;
    let UnitCallTarget::Declaration(target) = call.target() else {
        return Err(unsupported());
    };
    let signature = typed
        .signatures()
        .declaration(target)
        .and_then(|item| item.callable())
        .ok_or_else(bad)?;
    if call.receiver().is_some()
        || call.aborts()
        || call.prints_line()
        || call.result_source() != CallableResultSource::Owned
        || !call.instance().type_arguments().is_empty()
        || signature.receiver().is_some()
        || !signature.parameters().is_empty()
        || !signature.type_parameters().is_empty()
        || !signature.has_body()
    {
        return Err(unsupported());
    }
    if call.expression() != initializer
        || call.return_type() != ty
        || !call.arguments().is_empty()
        || typed.expression_category(initializer) != Some(ExpressionCategory::Temporary)
        || signature.target() != UnitCallableTarget::Declaration(target)
        || signature.return_type() != ty
        || signature.result_source() != call.result_source()
        || owned
            .call_argument_contracts_for(initializer)
            .next()
            .is_some()
        || owned.loans_ending_at(initializer).next().is_some()
    {
        return Err(bad());
    }
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == initializer.source_unit()
                && reference.namespace() == Some(Namespace::Value)
                && reference.span() == callee.span()
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        return Err(bad());
    };
    if !match reference.target() {
        UnitReferenceTarget::Declaration(actual) => *actual == target,
        UnitReferenceTarget::OverloadSet(targets) => targets.contains(&target),
        _ => false,
    } {
        return Err(bad());
    }
    Ok(())
}

pub(super) fn body(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let source = descriptor.statement().source_unit();
    let node = parsed.ast().statements().get(body).map_err(|_| bad())?;
    let Statement::Block { elements } = node.payload() else {
        return Err(unsupported());
    };
    let [statement] = elements.as_slice() else {
        return Err(unsupported());
    };
    let node = parsed
        .ast()
        .statements()
        .get(*statement)
        .map_err(|_| bad())?;
    let Statement::Expression { expression } = node.payload() else {
        return Err(unsupported());
    };
    let node = parsed
        .ast()
        .expressions()
        .get(*expression)
        .map_err(|_| bad())?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    if !type_arguments.is_empty()
        || arguments.len() != 2
        || arguments
            .iter()
            .any(|argument| argument.named_prefix.is_some() || argument.mode_marker.is_some())
    {
        return Err(unsupported());
    }
    let callee = parsed.ast().expressions().get(*callee).map_err(|_| bad())?;
    if callee.payload() != &Expression::Name {
        return Err(unsupported());
    }
    let call_id = UnitExpressionId::new(source, *expression);
    let call = typed.call(call_id).ok_or_else(bad)?;
    let UnitCallTarget::Declaration(target) = call.target() else {
        return Err(unsupported());
    };
    let signature = typed
        .signatures()
        .declaration(target)
        .and_then(|item| item.callable())
        .ok_or_else(bad)?;
    if call.receiver().is_some()
        || call.aborts()
        || call.prints_line()
        || !call.instance().type_arguments().is_empty()
        || signature.receiver().is_some()
        || !signature.has_body()
        || !signature.type_parameters().is_empty()
        || builtin_type(typed, call.return_type()) != Some(BuiltinType::Unit)
    {
        return Err(unsupported());
    }
    if call.expression() != call_id
        || typed.expression_type(call_id) != Some(call.return_type())
        || call.result_source() != CallableResultSource::Owned
        || signature.result_source() != call.result_source()
        || signature.target() != UnitCallableTarget::Declaration(target)
        || signature.return_type() != call.return_type()
        || signature.parameters().len() != 2
        || call.arguments().len() != 2
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
    if !match reference.target() {
        UnitReferenceTarget::Declaration(actual) => *actual == target,
        UnitReferenceTarget::OverloadSet(targets) => targets.contains(&target),
        _ => false,
    } {
        return Err(bad());
    }
    let plan = owned
        .map_iteration(descriptor.statement())
        .ok_or_else(bad)?;
    let bindings = descriptor.bindings();
    if bindings[0] == bindings[1] {
        return Err(bad());
    }
    let contracts = owned
        .call_argument_contracts_for(call_id)
        .collect::<Vec<_>>();
    let loans = owned.loans_ending_at(call_id).collect::<Vec<_>>();
    if contracts.len() != 2 || loans.len() != 2 {
        return Err(bad());
    }
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let symbol = bindings[index];
        let ty = [descriptor.key_type(), descriptor.value_type()][index];
        let binding = plan.bindings()[index];
        let loan = &plan.loans()[index];
        if symbol.source_unit() != source
            || typed.symbol_type(symbol) != Some(ty)
            || binding.symbol() != symbol
            || binding.kind() != OwnershipBindingKind::Shared
            || owned.binding_kind(symbol) != Some(OwnershipBindingKind::Shared)
            || loan.statement() != descriptor.statement()
            || loan.source() != descriptor.source()
            || loan.component() != component
            || loan.binding() != symbol
            || Some(loan.origin()) != plan.source()
            || loan.kind() != LoanKind::Shared
        {
            return Err(bad());
        }
        let argument = &arguments[index];
        let argument_id = UnitExpressionId::new(source, argument.value);
        let argument_node = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .map_err(|_| bad())?;
        if argument_node.payload() != &Expression::Name {
            return Err(unsupported());
        }
        let references = names
            .names()
            .references()
            .iter()
            .filter(|reference| {
                reference.source_unit() == source
                    && reference.namespace() == Some(Namespace::Value)
                    && reference.span() == argument_node.span()
            })
            .collect::<Vec<_>>();
        if !matches!(references.as_slice(), [reference] if reference.target() == &UnitReferenceTarget::Symbol(symbol))
        {
            return Err(bad());
        }
        let parameter = &signature.parameters()[index];
        let mapping = call.arguments()[index];
        let contract = contracts[index];
        let call_loan = loans[index];
        if parameter.mode() != ParameterMode::Borrow
            || parameter.ty() != ty
            || mapping.argument_index() != index
            || mapping.parameter_index() != index
            || mapping.mode() != ParameterMode::Borrow
            || mapping.parameter_type() != ty
            || mapping.crosses_thread()
            || typed.expression_type(argument_id) != Some(ty)
            || typed.expression_category(argument_id) != Some(ExpressionCategory::Place)
            || contract.argument() != argument_id
            || contract.parameter_index() != index
            || contract.parameter_type() != ty
            || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
            || contract.crosses_thread()
            || call_loan.argument() != argument_id
            || call_loan.kind() != LoanKind::Shared
            || call_loan.end_span() != node.span()
            || !matches!(call_loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == symbol)
        {
            return Err(bad());
        }
    }
    Ok(())
}

pub(super) fn straight_control_body(
    parsed: &ParsedFile,
    body: StatementId,
    span: Span,
) -> Result<(), LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(body)
        .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
    if let Statement::ControlBody { elements } = node.payload() {
        for statement in elements {
            straight_statement(parsed, *statement, span)?;
        }
        Ok(())
    } else {
        straight_statement(parsed, body, span)
    }
}
