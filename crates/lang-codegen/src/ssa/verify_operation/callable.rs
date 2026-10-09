//! callable 参数、结果、formation 与签名的一致性合同。
use super::*;

pub(super) fn direct_call_contract(
    module: &Module,
    function: &Function,
    callee: FunctionId,
    receiver: Option<EntityId>,
    arguments: &[EntityId],
    results: &[EntityType],
) -> bool {
    let Some(callee) = module.function(callee) else {
        return false;
    };
    let Some(entry) = callee.blocks.first() else {
        return false;
    };
    if callee.borrow_return.is_some() || callee.carrier_return.is_some() {
        return false;
    }
    let Some(parameter_types) = entry
        .parameters
        .iter()
        .map(|parameter| callee.entity(*parameter).map(|data| data.ty))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let receiver_count = usize::from(callee.receiver.is_some());
    if parameter_types.len() != arguments.len() + receiver_count
        || callee.receiver.is_some() != receiver.is_some()
        || !parameter_types.iter().all(|ty| match ty {
            EntityType::Value(ty) => is_first_class(module, *ty),
            EntityType::Loan { target, .. } => is_first_class(module, *target),
            EntityType::Place(_) => false,
        })
        || !callee
            .return_types
            .iter()
            .all(|ty| is_first_class(module, *ty))
    {
        return false;
    }
    let receiver_matches = receiver
        .zip(callee.receiver)
        .is_none_or(|(receiver, expected)| {
            !matches!(receiver, EntityId::Place(_))
                && function.entity(receiver).map(|entity| entity.ty) == Some(expected)
        });
    let arguments_match = arguments
        .iter()
        .zip(parameter_types.into_iter().skip(receiver_count))
        .all(|(argument, parameter)| {
            !matches!(argument, EntityId::Place(_))
                && function.entity(*argument).map(|entity| entity.ty) == Some(parameter)
        });
    let results_match = results
        == callee
            .return_types
            .iter()
            .copied()
            .map(EntityType::Value)
            .collect::<Vec<_>>();
    receiver_matches && arguments_match && results_match
}

pub(super) fn function_address_contract(
    module: &Module,
    target: FunctionId,
    results: &[EntityType],
) -> bool {
    if module.function(target).is_none_or(|function| {
        function.borrow_return.is_some() || function.carrier_return.is_some()
    }) {
        return false;
    }
    let Some(result) = single_value_result(results) else {
        return false;
    };
    let Some(SsaTypeKind::FunctionPointer { signature }) = module.type_kind(result) else {
        return false;
    };
    function_matches_signature(module, target, signature, None)
}

pub(super) fn closure_construct_contract(
    module: &Module,
    function: &Function,
    closure: SsaTypeId,
    thunk: FunctionId,
    operands: &[ClosureCaptureOperand],
    results: &[EntityType],
) -> bool {
    let Some(SsaTypeKind::ConcreteClosure {
        signature,
        environment,
        captures,
        ..
    }) = module.type_kind(closure)
    else {
        return false;
    };
    if single_value_result(results) != Some(closure) || operands.len() != captures.len() {
        return false;
    }
    let captures_match = operands.iter().zip(captures).all(|(operand, capture)| {
        match (operand, capture.mode) {
            (ClosureCaptureOperand::Owned(value), ClosureCaptureMode::Owned) => {
                value_type(function, *value) == Some(capture.ty)
            }
            (ClosureCaptureOperand::Shared(loan), ClosureCaptureMode::Shared) => matches!(
                function.entity(EntityId::Loan(*loan)).map(|entity| entity.ty),
                Some(EntityType::Loan { kind: LoanKind::Shared, target }) if target == capture.ty
            ),
            _ => false,
        }
    });
    captures_match && function_matches_signature(module, thunk, signature, Some(*environment))
}

pub(super) fn callable_invoke_contract(
    module: &Module,
    function: &Function,
    callable: ValueId,
    arguments: &[EntityId],
    results: &[EntityType],
) -> bool {
    let Some(signature) =
        value_type(function, callable).and_then(|ty| module.callable_signature(ty))
    else {
        return false;
    };
    arguments.len() == signature.parameters.len()
        && arguments
            .iter()
            .zip(&signature.parameters)
            .all(|(argument, expected)| {
                !matches!(argument, EntityId::Place(_))
                    && function.entity(*argument).map(|entity| entity.ty) == Some(*expected)
            })
        && results
            == signature
                .returns
                .iter()
                .copied()
                .map(EntityType::Value)
                .collect::<Vec<_>>()
}

pub(super) fn function_matches_signature(
    module: &Module,
    target: FunctionId,
    signature: &CallableSignature,
    environment: Option<SsaTypeId>,
) -> bool {
    let Some(function) = module.function(target) else {
        return false;
    };
    if function.receiver.is_some() {
        return false;
    }
    let Some(entry) = function.blocks.first() else {
        return false;
    };
    let mut expected = environment
        .into_iter()
        .map(|target| EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        })
        .collect::<Vec<_>>();
    expected.extend(signature.parameters.iter().copied());
    entry.parameters.len() == expected.len()
        && entry
            .parameters
            .iter()
            .zip(expected)
            .all(|(parameter, ty)| function.entity(*parameter).map(|data| data.ty) == Some(ty))
        && function.return_types == signature.returns
}
