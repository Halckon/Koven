//! Source-qualified ordinary-call 与 intrinsic container argument contracts。

use std::collections::BTreeSet;

use crate::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    parser::{Expression, ParameterModeMarker, ParsedFile},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypes, ContainerConstructionKind, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, SequentialContainerKind, UnitCallReceiverOrigin,
        UnitCallTarget, UnitCallableSignature, UnitContainerConstructionDescriptor,
        UnitExpressionId, UnitFunctionParameterType, UnitTypeId, UnitTypeKind,
    },
};

use super::{
    OwnershipCheckingError, UnitCallArgumentOwnershipContract, UnitCallArgumentOwnershipKind,
    UnitCallReceiverOwnershipContract,
};

pub(super) fn collect_call_receiver_contracts(
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Result<Vec<UnitCallReceiverOwnershipContract>, OwnershipCheckingError> {
    let mut contracts = Vec::new();
    for call in typed.calls() {
        let Some(receiver) = call.receiver() else {
            continue;
        };
        let call_id = call.expression();
        let parsed = parsed_for_call(inputs, names, call_id)?;
        let call_node = parsed.ast().expressions().get(call_id.expression())?;
        let Expression::Call { callee, .. } = call_node.payload() else {
            return Err(invalid_unit_call(call_id));
        };
        let receiver_span = match receiver.origin() {
            UnitCallReceiverOrigin::Expression(expression) => {
                if expression.source_unit() != call_id.source_unit()
                    || typed.expression_category(expression) != Some(receiver.category())
                {
                    return Err(invalid_unit_call(call_id));
                }
                parsed
                    .ast()
                    .expressions()
                    .get(expression.expression())?
                    .span()
            }
            UnitCallReceiverOrigin::ImplicitThis(_) => {
                parsed.ast().expressions().get(*callee)?.span()
            }
        };
        contracts.push(UnitCallReceiverOwnershipContract::new(
            call_id,
            call.target(),
            receiver.origin(),
            receiver.ty(),
            receiver.category(),
            ownership_kind(receiver.mode()),
            receiver_span,
            call_node.span(),
            source_receiver_span(typed, call.target()),
        ));
    }
    for append in typed.container_appends() {
        let call_id = append.expression();
        let parsed = parsed_for_call(inputs, names, call_id)?;
        let call_node = parsed.ast().expressions().get(call_id.expression())?;
        let receiver_expr = append.receiver();
        let receiver_span = parsed
            .ast()
            .expressions()
            .get(receiver_expr.expression())?
            .span();
        contracts.push(UnitCallReceiverOwnershipContract::new(
            call_id,
            UnitCallTarget::FunctionValue,
            UnitCallReceiverOrigin::Expression(receiver_expr),
            append.container_type(),
            typed
                .expression_category(receiver_expr)
                .unwrap_or(ExpressionCategory::Place),
            ownership_kind(ParameterMode::Inout),
            receiver_span,
            call_node.span(),
            None,
        ));
    }
    for clear in typed.container_clears() {
        let call_id = clear.expression();
        let parsed = parsed_for_call(inputs, names, call_id)?;
        let call_node = parsed.ast().expressions().get(call_id.expression())?;
        let receiver_expr = clear.receiver();
        let receiver_span = parsed
            .ast()
            .expressions()
            .get(receiver_expr.expression())?
            .span();
        contracts.push(UnitCallReceiverOwnershipContract::new(
            call_id,
            UnitCallTarget::FunctionValue,
            UnitCallReceiverOrigin::Expression(receiver_expr),
            clear.container_type(),
            typed
                .expression_category(receiver_expr)
                .unwrap_or(ExpressionCategory::Place),
            ownership_kind(ParameterMode::Inout),
            receiver_span,
            call_node.span(),
            None,
        ));
    }
    for remove_at in typed.container_remove_ats() {
        let call_id = remove_at.expression();
        let parsed = parsed_for_call(inputs, names, call_id)?;
        let call_node = parsed.ast().expressions().get(call_id.expression())?;
        let receiver_expr = remove_at.receiver();
        let receiver_span = parsed
            .ast()
            .expressions()
            .get(receiver_expr.expression())?
            .span();
        contracts.push(UnitCallReceiverOwnershipContract::new(
            call_id,
            UnitCallTarget::FunctionValue,
            UnitCallReceiverOrigin::Expression(receiver_expr),
            remove_at.container_type(),
            typed
                .expression_category(receiver_expr)
                .unwrap_or(ExpressionCategory::Place),
            ownership_kind(ParameterMode::Inout),
            receiver_span,
            call_node.span(),
            None,
        ));
    }
    contracts.sort_by_key(|contract| {
        (
            contract.call().source_unit().index(),
            contract.call_span().start(),
            contract.receiver_span().start(),
        )
    });
    Ok(contracts)
}

pub(super) fn collect_call_argument_contracts(
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Result<Vec<UnitCallArgumentOwnershipContract>, OwnershipCheckingError> {
    let mut contracts = Vec::new();
    let mut seen_calls = BTreeSet::new();
    for call in typed.calls() {
        let call_id = call.expression();
        if !seen_calls.insert(call_id) {
            return Err(invalid_unit_call(call_id));
        }
        let parsed = parsed_for_call(inputs, names, call_id)?;
        let call_node = parsed.ast().expressions().get(call_id.expression())?;
        let Expression::Call { arguments, .. } = call_node.payload() else {
            return Err(invalid_unit_call(call_id));
        };
        if arguments.len() != call.arguments().len() {
            return Err(invalid_unit_call(call_id));
        }
        let mut seen_arguments = vec![false; arguments.len()];
        let mut seen_parameters = vec![false; arguments.len()];
        for descriptor in call.arguments() {
            let argument_index = descriptor.argument_index();
            let Some(argument) = arguments.get(argument_index) else {
                return Err(invalid_unit_call_argument(call_id, argument_index));
            };
            if std::mem::replace(&mut seen_arguments[argument_index], true) {
                return Err(invalid_unit_call_argument(call_id, argument_index));
            }
            let parameter_index = descriptor.parameter_index();
            let Some(seen_parameter) = seen_parameters.get_mut(parameter_index) else {
                return Err(invalid_unit_call_parameter(call_id, parameter_index));
            };
            if std::mem::replace(seen_parameter, true) {
                return Err(invalid_unit_call_parameter(call_id, parameter_index));
            }
            let parameter_span =
                source_parameter_span(typed, call.target(), parameter_index, call_id)?;
            contracts.push(UnitCallArgumentOwnershipContract {
                call: call_id,
                argument: UnitExpressionId::new(call_id.source_unit(), argument.value),
                parameter_index,
                parameter_type: descriptor.parameter_type(),
                category: descriptor.category(),
                kind: ownership_kind(descriptor.mode()),
                crosses_thread: descriptor.crosses_thread(),
                argument_span: parsed.ast().expressions().get(argument.value)?.span(),
                call_span: call_node.span(),
                parameter_span,
                loan_begin_span: match argument.mode_marker {
                    Some(ParameterModeMarker::Inout(span)) => span,
                    _ => parsed.ast().expressions().get(argument.value)?.span(),
                },
            });
        }
        if seen_arguments.iter().any(|seen| !seen) {
            return Err(invalid_unit_call(call_id));
        }
        if let Some(parameter) = seen_parameters.iter().position(|seen| !seen) {
            return Err(invalid_unit_call_parameter(call_id, parameter));
        }
    }

    validate_unique_container_calls(typed.container_constructions(), &mut seen_calls)?;
    for descriptor in typed.container_constructions() {
        collect_container_contracts(inputs, names, typed, descriptor, &mut contracts)?;
    }
    for append in typed.container_appends() {
        let call_id = append.expression();
        if !seen_calls.insert(call_id) {
            return Err(invalid_unit_call(call_id));
        }
        let parsed = parsed_for_call(inputs, names, call_id)?;
        let call_node = parsed.ast().expressions().get(call_id.expression())?;
        let element_expr = append.element();
        let element_span = parsed
            .ast()
            .expressions()
            .get(element_expr.expression())?
            .span();
        contracts.push(UnitCallArgumentOwnershipContract {
            call: call_id,
            argument: element_expr,
            parameter_index: 0,
            parameter_type: append.element_type(),
            category: typed
                .expression_category(element_expr)
                .unwrap_or(ExpressionCategory::Temporary),
            kind: ownership_kind(ParameterMode::Value),
            crosses_thread: false,
            argument_span: element_span,
            call_span: call_node.span(),
            parameter_span: None,
            loan_begin_span: element_span,
        });
    }
    for clear in typed.container_clears() {
        let call_id = clear.expression();
        if !seen_calls.insert(call_id) {
            return Err(invalid_unit_call(call_id));
        }
    }
    for remove_at in typed.container_remove_ats() {
        let call_id = remove_at.expression();
        if !seen_calls.insert(call_id) {
            return Err(invalid_unit_call(call_id));
        }
        let parsed = parsed_for_call(inputs, names, call_id)?;
        let call_node = parsed.ast().expressions().get(call_id.expression())?;
        let index_expr = remove_at.index();
        let index_span = parsed
            .ast()
            .expressions()
            .get(index_expr.expression())?
            .span();
        let int = typed
            .types()
            .builtin(BuiltinType::Int)
            .ok_or_else(|| invalid_unit_call(call_id))?;
        contracts.push(UnitCallArgumentOwnershipContract {
            call: call_id,
            argument: index_expr,
            parameter_index: 0,
            parameter_type: int,
            category: typed
                .expression_category(index_expr)
                .unwrap_or(ExpressionCategory::Temporary),
            kind: ownership_kind(ParameterMode::Value),
            crosses_thread: false,
            argument_span: index_span,
            call_span: call_node.span(),
            parameter_span: None,
            loan_begin_span: index_span,
        });
    }
    contracts.sort_by_key(|contract| {
        (
            contract.call.source_unit().index(),
            contract.call_span.start(),
            contract.argument_span.start(),
        )
    });
    Ok(contracts)
}

fn validate_unique_container_calls(
    descriptors: &[UnitContainerConstructionDescriptor],
    seen_calls: &mut BTreeSet<UnitExpressionId>,
) -> Result<(), OwnershipCheckingError> {
    for descriptor in descriptors {
        let call = descriptor.expression();
        if !seen_calls.insert(call) {
            return Err(invalid_unit_container_construction(call));
        }
    }
    Ok(())
}

fn collect_container_contracts(
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    descriptor: &UnitContainerConstructionDescriptor,
    contracts: &mut Vec<UnitCallArgumentOwnershipContract>,
) -> Result<(), OwnershipCheckingError> {
    let call = descriptor.expression();
    let parsed = parsed_for_call(inputs, names, call)?;
    let call_node = parsed.ast().expressions().get(call.expression())?;
    let Expression::Call { arguments, .. } = call_node.payload() else {
        return Err(invalid_unit_container_construction(call));
    };
    if typed.expression_type(call) != Some(descriptor.container_type())
        || typed.expression_category(call) != Some(ExpressionCategory::Temporary)
        || !valid_container_type(typed, descriptor)
        || !valid_container_shape(descriptor, arguments.len())
    {
        return Err(invalid_unit_container_construction(call));
    }

    for (parameter_index, (argument, &mode)) in arguments
        .iter()
        .zip(descriptor.parameter_modes())
        .enumerate()
    {
        let argument_id = UnitExpressionId::new(call.source_unit(), argument.value);
        let Some(category) = typed.expression_category(argument_id) else {
            return Err(invalid_unit_container_construction(call));
        };
        if argument.named_prefix.is_some()
            || !valid_container_argument_marker(descriptor.kind(), argument.mode_marker)
        {
            return Err(invalid_unit_container_construction(call));
        }
        let parameter_type =
            container_parameter_type(typed, descriptor, argument_id, parameter_index)?;
        let argument_span = parsed.ast().expressions().get(argument.value)?.span();
        contracts.push(UnitCallArgumentOwnershipContract {
            call,
            argument: argument_id,
            parameter_index,
            parameter_type,
            category,
            kind: ownership_kind(mode),
            crosses_thread: false,
            argument_span,
            call_span: call_node.span(),
            parameter_span: None,
            loan_begin_span: argument_span,
        });
    }
    Ok(())
}

fn valid_container_type(
    typed: &CompilationUnitTypes,
    descriptor: &UnitContainerConstructionDescriptor,
) -> bool {
    let constructor = match descriptor.container() {
        SequentialContainerKind::Array => IntrinsicTypeConstructor::Array,
        SequentialContainerKind::List => IntrinsicTypeConstructor::List,
        SequentialContainerKind::MutableList => IntrinsicTypeConstructor::MutableList,
    };
    matches!(
        typed.types().get(descriptor.container_type()),
        Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
            if *actual == constructor && arguments.as_slice() == [descriptor.element_type()]
    ) && typed.types().get(descriptor.element_type()).is_some()
}

fn valid_container_shape(
    descriptor: &UnitContainerConstructionDescriptor,
    argument_count: usize,
) -> bool {
    if descriptor.parameter_modes().len() != argument_count {
        return false;
    }
    match descriptor.kind() {
        ContainerConstructionKind::ListForm => descriptor
            .parameter_modes()
            .iter()
            .all(|mode| *mode == ParameterMode::Value),
        ContainerConstructionKind::RuntimeLength => {
            matches!(
                descriptor.container(),
                SequentialContainerKind::Array | SequentialContainerKind::List
            ) && descriptor.parameter_modes() == [ParameterMode::Borrow, ParameterMode::Borrow]
        }
        ContainerConstructionKind::EmptyMutableList => {
            descriptor.container() == SequentialContainerKind::MutableList
                && descriptor.parameter_modes().is_empty()
        }
    }
}

const fn valid_container_argument_marker(
    kind: ContainerConstructionKind,
    marker: Option<ParameterModeMarker>,
) -> bool {
    match kind {
        ContainerConstructionKind::ListForm | ContainerConstructionKind::EmptyMutableList => {
            marker.is_none()
        }
        ContainerConstructionKind::RuntimeLength => {
            matches!(marker, None | Some(ParameterModeMarker::Borrow(_)))
        }
    }
}

fn container_parameter_type(
    typed: &CompilationUnitTypes,
    descriptor: &UnitContainerConstructionDescriptor,
    argument: UnitExpressionId,
    parameter_index: usize,
) -> Result<UnitTypeId, OwnershipCheckingError> {
    let Some(actual) = typed.expression_type(argument) else {
        return Err(invalid_unit_container_construction(descriptor.expression()));
    };
    let expected = match descriptor.kind() {
        ContainerConstructionKind::ListForm => descriptor.element_type(),
        ContainerConstructionKind::RuntimeLength => {
            runtime_parameter_type(typed, descriptor, parameter_index)?
        }
        ContainerConstructionKind::EmptyMutableList => {
            return Err(invalid_unit_container_construction(descriptor.expression()));
        }
    };
    if !assignable(typed, actual, expected) {
        return Err(invalid_unit_container_construction(descriptor.expression()));
    }
    Ok(expected)
}

fn runtime_parameter_type(
    typed: &CompilationUnitTypes,
    descriptor: &UnitContainerConstructionDescriptor,
    parameter_index: usize,
) -> Result<UnitTypeId, OwnershipCheckingError> {
    let invalid = || invalid_unit_container_construction(descriptor.expression());
    let int = typed
        .types()
        .builtin(BuiltinType::Int)
        .ok_or_else(invalid)?;
    match parameter_index {
        0 => Ok(int),
        1 => {
            let expected = UnitTypeKind::Function {
                move_only: false,
                parameters: vec![UnitFunctionParameterType::new(ParameterMode::Borrow, int)],
                return_type: descriptor.element_type(),
            };
            (0..typed.types().len())
                .map(UnitTypeId::new)
                .find(|&ty| typed.types().get(ty) == Some(&expected))
                .ok_or_else(invalid)
        }
        _ => Err(invalid()),
    }
}

fn assignable(typed: &CompilationUnitTypes, actual: UnitTypeId, expected: UnitTypeId) -> bool {
    actual == expected
        || matches!(typed.types().get(actual), Some(UnitTypeKind::Error))
        || matches!(typed.types().get(expected), Some(UnitTypeKind::Error))
        || typed.types().get(actual) == Some(&UnitTypeKind::Builtin(BuiltinType::Nothing))
        || matches!(
            typed.types().get(actual),
            Some(UnitTypeKind::EnumCase { root, .. }) if *root == expected
        )
        || matches!(
            typed.types().get(actual),
            Some(UnitTypeKind::StaticSelf(interface)) if *interface == expected
        )
        || matches!(
            typed.types().get(expected),
            Some(UnitTypeKind::Nullable(inner))
                if actual == *inner
                    || matches!(
                        typed.types().get(actual),
                        Some(UnitTypeKind::EnumCase { root, .. }) if root == inner
                    )
        )
}

const fn ownership_kind(mode: ParameterMode) -> UnitCallArgumentOwnershipKind {
    match mode {
        ParameterMode::Value => UnitCallArgumentOwnershipKind::Value,
        ParameterMode::Borrow => UnitCallArgumentOwnershipKind::SharedLoan,
        ParameterMode::Inout => UnitCallArgumentOwnershipKind::ExclusiveLoan,
    }
}

fn parsed_for_call<'parsed>(
    inputs: &[SourceUnitInput<'parsed>],
    names: &ValidatedCompilationUnitNames,
    call: UnitExpressionId,
) -> Result<&'parsed ParsedFile, OwnershipCheckingError> {
    let source_unit = call.source_unit();
    let source_id = names
        .names()
        .index()
        .source_units()
        .get(source_unit.index())
        .map(|source| source.source_id())
        .ok_or_else(|| invalid_unit_call(call))?;
    inputs
        .iter()
        .copied()
        .find(|input| input.source_id() == source_id)
        .map(SourceUnitInput::parsed)
        .ok_or_else(|| invalid_unit_call(call))
}

fn source_parameter_span(
    typed: &CompilationUnitTypes,
    target: UnitCallTarget,
    parameter_index: usize,
    call: UnitExpressionId,
) -> Result<Option<Span>, OwnershipCheckingError> {
    let signature = match target {
        UnitCallTarget::Declaration(declaration) => typed
            .signatures()
            .declaration(declaration)
            .and_then(|declaration| declaration.callable()),
        UnitCallTarget::Symbol(symbol) => typed
            .signatures()
            .declarations()
            .iter()
            .flat_map(|declaration| {
                declaration.callable().into_iter().chain(
                    declaration.nominal().into_iter().flat_map(|nominal| {
                        nominal.members().iter().chain(nominal.companion_members())
                    }),
                )
            })
            .find(|callable| {
                callable.target() == crate::type_checking::UnitCallableTarget::Symbol(symbol)
            }),
        UnitCallTarget::External(_)
        | UnitCallTarget::FunctionValue
        | UnitCallTarget::StructuralComponent(_) => return Ok(None),
    };
    signature
        .and_then(|signature| signature.parameters().get(parameter_index))
        .map(|parameter| Some(parameter.span()))
        .ok_or(OwnershipCheckingError::InvalidUnitCallParameter {
            source_unit: call.source_unit().index(),
            expression: call.expression().index(),
            parameter: parameter_index,
        })
}

fn source_receiver_span(typed: &CompilationUnitTypes, target: UnitCallTarget) -> Option<Span> {
    source_callable_signature(typed, target)
        .and_then(UnitCallableSignature::receiver)
        .map(|receiver| {
            receiver
                .marker_span()
                .unwrap_or(receiver.declaration_span())
        })
}

pub(super) fn source_callable_signature(
    typed: &CompilationUnitTypes,
    target: UnitCallTarget,
) -> Option<&UnitCallableSignature> {
    match target {
        UnitCallTarget::Declaration(declaration) => typed
            .signatures()
            .declaration(declaration)
            .and_then(|declaration| declaration.callable()),
        UnitCallTarget::Symbol(symbol) => typed
            .signatures()
            .declarations()
            .iter()
            .flat_map(|declaration| {
                declaration.callable().into_iter().chain(
                    declaration.nominal().into_iter().flat_map(|nominal| {
                        nominal.members().iter().chain(nominal.companion_members())
                    }),
                )
            })
            .find(|callable| {
                callable.target() == crate::type_checking::UnitCallableTarget::Symbol(symbol)
            }),
        UnitCallTarget::External(_)
        | UnitCallTarget::FunctionValue
        | UnitCallTarget::StructuralComponent(_) => None,
    }
}

const fn invalid_unit_call(call: UnitExpressionId) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidUnitCall {
        source_unit: call.source_unit().index(),
        expression: call.expression().index(),
    }
}

const fn invalid_unit_call_argument(
    call: UnitExpressionId,
    argument: usize,
) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidUnitCallArgument {
        source_unit: call.source_unit().index(),
        expression: call.expression().index(),
        argument,
    }
}

const fn invalid_unit_call_parameter(
    call: UnitExpressionId,
    parameter: usize,
) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidUnitCallParameter {
        source_unit: call.source_unit().index(),
        expression: call.expression().index(),
        parameter,
    }
}

const fn invalid_unit_container_construction(call: UnitExpressionId) -> OwnershipCheckingError {
    OwnershipCheckingError::InvalidUnitContainerConstruction {
        source_unit: call.source_unit().index(),
        expression: call.expression().index(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::parse_file,
        source::SourceMap,
        type_checking::{
            ContainerConstructionKind, ParameterMode, SequentialContainerKind,
            UnitContainerConstructionDescriptor, check_compilation_unit_types,
            standard_environments,
        },
    };

    use super::{OwnershipCheckingError, valid_container_shape, validate_unique_container_calls};

    #[test]
    fn duplicate_container_descriptor_is_rejected_before_dataflow() {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(
                "p/source.ko",
                "package p\nfun build(): Unit { val result = listOf(1) }",
            )
            .expect("unique source");
        let lexed = lex(&sources, source).expect("lexing succeeds internally");
        let parsed = parse_file(&sources, &lexed).expect("parsing succeeds internally");
        assert!(parsed.diagnostics().is_empty());
        let inputs = [SourceUnitInput::new("root", "p/source.ko", source, &parsed)];
        let (name_environment, type_environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).expect("valid unit input");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .expect("name resolution succeeds internally")
            .validate()
            .expect("valid names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("type checking succeeds internally")
            .validate()
            .expect("valid types");
        let descriptor = typed.types().container_constructions()[0].clone();
        let invalid_list = UnitContainerConstructionDescriptor::new(
            descriptor.expression(),
            ContainerConstructionKind::ListForm,
            descriptor.container(),
            descriptor.container_type(),
            descriptor.element_type(),
            vec![ParameterMode::Borrow],
        );
        let runtime_mutable = UnitContainerConstructionDescriptor::new(
            descriptor.expression(),
            ContainerConstructionKind::RuntimeLength,
            SequentialContainerKind::MutableList,
            descriptor.container_type(),
            descriptor.element_type(),
            vec![ParameterMode::Borrow, ParameterMode::Borrow],
        );
        let nonempty_empty = UnitContainerConstructionDescriptor::new(
            descriptor.expression(),
            ContainerConstructionKind::EmptyMutableList,
            SequentialContainerKind::MutableList,
            descriptor.container_type(),
            descriptor.element_type(),
            vec![ParameterMode::Value],
        );

        assert!(!valid_container_shape(&invalid_list, 1));
        assert!(!valid_container_shape(&runtime_mutable, 2));
        assert!(!valid_container_shape(&nonempty_empty, 1));

        let error = validate_unique_container_calls(
            &[descriptor.clone(), descriptor],
            &mut BTreeSet::new(),
        )
        .expect_err("duplicate container locator must fail before body traversal");

        assert!(matches!(
            error,
            OwnershipCheckingError::InvalidUnitContainerConstruction { .. }
        ));
    }
}
