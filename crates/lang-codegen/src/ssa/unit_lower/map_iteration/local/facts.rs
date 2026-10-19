//! Exact frontend call/loan facts for the bounded local normal row.
use super::*;
use lang_frontend::ownership_checking::{LoanKind, UnitCallArgumentOwnershipKind};

pub(in super::super) fn factory(
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
        callee, arguments, ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    if typed.expression_type(initializer) != Some(ty)
        || typed.expression_category(initializer) != Some(ExpressionCategory::Temporary)
    {
        return Err(bad());
    }
    if let Some(construction) = typed.map_construction(initializer) {
        return if construction.expression() == initializer
            && construction.map_type() == ty
            && arguments.is_empty()
        {
            Ok(())
        } else {
            Err(bad())
        };
    }
    if let Some(consume) = typed.map_consume(initializer) {
        return if consume.expression() == initializer
            && consume.result_type() == ty
            && arguments.is_empty()
            && consume.receiver().source_unit() == initializer.source_unit()
            && typed.expression_type(consume.receiver()) == Some(consume.receiver_type())
        {
            Ok(())
        } else {
            Err(bad())
        };
    }
    let callee = parsed.ast().expressions().get(*callee).map_err(|_| bad())?;
    if !matches!(
        callee.payload(),
        Expression::Name | Expression::Member { .. }
    ) {
        return Err(unsupported());
    }
    let call = typed.call(initializer).ok_or_else(bad)?;
    if call.target() == UnitCallTarget::FunctionValue {
        let callee_id = match node.payload() {
            Expression::Call { callee, .. } => {
                UnitExpressionId::new(initializer.source_unit(), *callee)
            }
            _ => return Err(bad()),
        };
        let callee_type = typed.expression_type(callee_id).ok_or_else(bad)?;
        let Some(UnitTypeKind::Function {
            parameters,
            return_type,
            ..
        }) = typed.types().get(callee_type)
        else {
            return Err(bad());
        };
        if callee.payload() != &Expression::Name {
            return Err(unsupported());
        }
        if call.expression() != initializer
            || call.return_type() != ty
            || call.result_source() != CallableResultSource::Owned
            || call.receiver().is_some()
            || call.aborts()
            || call.prints_line()
            || !call.instance().type_arguments().is_empty()
            || call.arguments().len() != arguments.len()
            || call.arguments().len() != parameters.len()
            || *return_type != ty
            || owned.callable_origin(callee_id).is_none()
        {
            return Err(bad());
        }
        return Ok(());
    }
    let target = match call.target() {
        UnitCallTarget::Declaration(target) => UnitCallableTarget::Declaration(target),
        UnitCallTarget::Symbol(target) => UnitCallableTarget::Symbol(target),
        _ => return Err(unsupported()),
    };
    let signature =
        crate::ssa::unit_plan::unit_callable_signature(typed, target).ok_or_else(bad)?;
    if call.aborts()
        || call.prints_line()
        || call.result_source() != CallableResultSource::Owned
        || (matches!(target, UnitCallableTarget::Declaration(_)) && !signature.has_body())
    {
        return Err(unsupported());
    }
    if call.expression() != initializer
        || call.return_type() != ty
        || call.arguments().len() != arguments.len()
        || call.arguments().len() != signature.parameters().len()
        || typed.expression_type(initializer) != Some(ty)
        || typed.expression_category(initializer) != Some(ExpressionCategory::Temporary)
        || signature.target() != target
        || (call.instance().type_arguments().is_empty() && signature.return_type() != ty)
        || signature.result_source() != call.result_source()
        || signature.receiver().is_some() != call.receiver().is_some()
    {
        return Err(bad());
    }
    if callee.payload() == &Expression::Name {
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
        if !match (reference.target(), target) {
            (UnitReferenceTarget::Declaration(actual), UnitCallableTarget::Declaration(target)) => {
                *actual == target
            }
            (
                UnitReferenceTarget::OverloadSet(targets),
                UnitCallableTarget::Declaration(target),
            ) => targets.contains(&target),
            (UnitReferenceTarget::Symbol(actual), UnitCallableTarget::Symbol(target)) => {
                *actual == target
            }
            (UnitReferenceTarget::Symbols(targets), UnitCallableTarget::Symbol(target)) => {
                targets.contains(&target)
            }
            _ => false,
        } {
            return Err(bad());
        }
    }
    // Member selection, receiver ownership, argument delivery and any effective
    // override/delegation route belong to the ordinary call planner and emitter.
    // The provider anchors their actual result, without replaying those operands.
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
